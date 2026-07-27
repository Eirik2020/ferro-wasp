#![forbid(unsafe_code)]

use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use ferro_configurator_core::{
    BoardProfile, CatalogEntry, ConfigKey, ConversionSummary, DeviceSelector, DfuDetection,
    DownloadSummary, FerroConfig, FerroError, FlashInfo, FlashProgress, FlightSelector, PortInfo,
    PreparedImage, ProfileStore, StatusSnapshot, catalog_device, convert_fwbb_to_ulog, detect_dfu,
    discover_ports, download_flight, find_bundled_firmware, flash_firmware, open_device,
    prepare_elf, resolve_device_flight,
};
use serde::Serialize;

const FLASH_WIZARD_STEP_TITLES: [&str; 6] = [
    "Step 1 of 6 - Check the firmware and make the bench safe",
    "Step 2 of 6 - Disconnect USB",
    "Step 3 of 6 - Enter the STM32 ROM bootloader",
    "Step 4 of 6 - Detect the controller",
    "Step 5 of 6 - Final confirmation",
    "Step 6 of 6 - Program and restart",
];

#[derive(Debug, Parser)]
#[command(
    name = "ferro-configurator",
    version,
    about = "Safely inspect and configure FerroWasp flight controllers",
    long_about = "FerroConfigurator talks to the bounded USB CDC storage/configuration interface in current FerroWasp firmware. Configuration is host-validated, firmware-validated, persisted, and read back."
)]
struct Cli {
    /// Select a COM port explicitly, for example COM7.
    #[arg(long, global = true)]
    port: Option<String>,

    /// Per-command response timeout in milliseconds.
    #[arg(
        long,
        global = true,
        default_value_t = 3000,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    timeout_ms: u64,

    /// Machine-readable output format.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Find and inspect connected controllers.
    Device(DeviceArgs),
    /// Read, validate, export, and apply tuning configuration.
    Config(ConfigArgs),
    /// List, selectively download, convert, and erase onboard flight logs.
    Blackbox(BlackboxArgs),
    /// Convert one CRC-validated FWBB flight to ULog.
    Convert(ConvertArgs),
    /// Validate and flash a FerroWasp ELF through the STM32 ROM DFU bootloader.
    Flash(FlashArgs),
    /// Inspect the bundled flasher and connected STM32 DFU devices.
    Dfu(DfuArgs),
    /// Run read-only host and USB diagnostics.
    Doctor,
}

#[derive(Debug, Args)]
struct BlackboxArgs {
    #[command(subcommand)]
    command: BlackboxCommand,
}

#[derive(Debug, Subcommand)]
enum BlackboxCommand {
    /// List stored flights grouped by recorded MCU boot session.
    Flights,
    /// Download one flight without reading older flights.
    Download {
        /// Numeric flight ID, or `latest`.
        #[arg(long, default_value = "latest")]
        flight: String,
        /// Final raw FWBB evidence path.
        #[arg(long)]
        output: PathBuf,
        /// Validate and continue the matching `.part` file.
        #[arg(long)]
        resume: bool,
        /// Also convert the completed raw download to this ULog path.
        #[arg(long)]
        ulog: Option<PathBuf>,
    },
    /// Download every available flight in an inclusive ID range.
    DownloadRange {
        #[arg(long)]
        from: u32,
        /// Inclusive numeric flight ID, or `latest`.
        #[arg(long, default_value = "latest")]
        to: String,
        #[arg(long)]
        directory: PathBuf,
        /// Resume matching `.fwbb.part` files.
        #[arg(long)]
        resume: bool,
        /// Convert every completed flight to a sibling `.ulg` file.
        #[arg(long)]
        ulog: bool,
    },
    /// Erase every onboard log after explicit confirmation.
    Erase {
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Debug, Args)]
struct ConvertArgs {
    /// CRC-validated onboard FWBB archive.
    input: PathBuf,
    /// Output ULog path. Defaults to the input path with `.ulg`.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Numeric flight ID, or `latest`.
    #[arg(long, default_value = "latest")]
    flight: String,
    /// Explicitly replace an existing ULog output.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct FlashArgs {
    /// Optional developer-supplied FerroWasp ARM ELF32 executable. The
    /// packaged release image is used when omitted.
    firmware: Option<PathBuf>,

    /// Physical board whose linker map and memory limits must match the ELF.
    #[arg(long, value_enum)]
    board: BoardChoice,

    /// Skip the interactive safety wizard (intended for deliberate automation only).
    #[arg(long)]
    yes: bool,

    /// Validate and prepare the ELF without accessing USB or changing flash.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BoardChoice {
    #[value(name = "foxeer-f405-v2")]
    FoxeerF405V2,
}

impl BoardChoice {
    const fn profile(self) -> BoardProfile {
        match self {
            Self::FoxeerF405V2 => BoardProfile::FOXEER_F405_V2,
        }
    }
}

#[derive(Debug, Args)]
struct DfuArgs {
    #[command(subcommand)]
    command: DfuCommand,
}

#[derive(Debug, Subcommand)]
enum DfuCommand {
    /// Show the bundled dfu-util and STM32 ROM DFU devices.
    List,
}

#[derive(Debug, Args)]
struct DeviceArgs {
    #[command(subcommand)]
    command: DeviceCommand,
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// List FerroWasp ports, or every serial port with --all.
    List {
        #[arg(long)]
        all: bool,
    },
    /// Show storage and live safety status.
    Info,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Read the complete active configuration.
    Show,
    /// Export active configuration as reusable TOML.
    Export {
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Validate a TOML file without connecting to a controller.
    Validate { path: PathBuf },
    /// Stage, persist, and verify a complete TOML configuration.
    Apply { path: PathBuf },
    /// Save the connected drone's verified configuration as a named local profile.
    Store {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// List locally stored configuration profiles.
    Stored,
    /// Show a locally stored configuration profile without connecting to a drone.
    ShowStored { name: String },
    /// Apply a locally stored profile to the connected drone and verify it.
    Load { name: String },
    /// Import a TOML file into the named local profile store.
    Import {
        name: String,
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Change one whitelisted key, persist, and verify the whole configuration.
    Set { key: ConfigKey, value: String },
    /// List accepted setting names and ranges.
    Keys,
}

#[derive(Debug, Serialize)]
struct DeviceReport {
    port: String,
    board: &'static str,
    protocol: &'static str,
    flash: FlashInfo,
    status: Option<StatusSnapshot>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    application: &'static str,
    version: &'static str,
    operating_system: &'static str,
    architecture: &'static str,
    serial_ports: Vec<PortInfo>,
    ferrowasp_devices: usize,
    dfu: Option<DfuDetection>,
    dfu_error: Option<String>,
    guidance: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct Success<'a, T: Serialize> {
    ok: bool,
    operation: &'a str,
    result: T,
}

#[derive(Debug, Serialize)]
struct BlackboxDownloadReport {
    download: DownloadSummary,
    ulog: Option<ConversionSummary>,
}

#[derive(Debug, Serialize)]
struct ErrorOutput<'a> {
    ok: bool,
    error: &'a str,
    hint: &'a str,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_error(cli.format, &error);
            ExitCode::from(exit_code(&error))
        }
    }
}

fn run(cli: &Cli) -> Result<(), FerroError> {
    let timeout = Duration::from_millis(cli.timeout_ms);
    match &cli.command {
        Command::Device(args) => match &args.command {
            DeviceCommand::List { all } => {
                let ports = discover_ports()?;
                let visible: Vec<_> = ports
                    .into_iter()
                    .filter(|port| *all || port.is_ferrowasp)
                    .collect();
                emit(cli.format, "device.list", &visible, || {
                    if visible.is_empty() {
                        println!("No FerroWasp USB CDC devices found.");
                    } else {
                        for port in &visible {
                            let identity = match (port.vid, port.pid) {
                                (Some(vid), Some(pid)) => format!("{vid:04x}:{pid:04x}"),
                                _ => "non-USB/unknown".to_owned(),
                            };
                            let product = port.product.as_deref().unwrap_or("serial device");
                            println!("{:<10} {:<9} {}", port.port, identity, product);
                        }
                    }
                })
            }
            DeviceCommand::Info => {
                let mut client = connect(cli, timeout)?;
                let flash = client.flash_info()?;
                let status = client.read_status().ok();
                let report = DeviceReport {
                    port: client.description().to_owned(),
                    board: "Foxeer F405 V2",
                    protocol: "FerroWasp storage CLI v1",
                    flash,
                    status,
                };
                emit(cli.format, "device.info", &report, || {
                    print_device_report(&report)
                })
            }
        },
        Command::Config(args) => match &args.command {
            ConfigCommand::Show => {
                let mut client = connect(cli, timeout)?;
                let config = client.read_config()?;
                let rendered = config.to_toml()?;
                emit(cli.format, "config.show", &config, || {
                    print!("{rendered}");
                })
            }
            ConfigCommand::Export { path, force } => {
                let mut client = connect(cli, timeout)?;
                let config = client.read_config()?;
                config.write_toml_file(path, *force)?;
                emit(
                    cli.format,
                    "config.export",
                    &path.display().to_string(),
                    || {
                        println!("Exported verified configuration to {}", path.display());
                    },
                )
            }
            ConfigCommand::Validate { path } => {
                let config = FerroConfig::from_toml_file(path)?;
                emit(cli.format, "config.validate", &config, || {
                    println!(
                        "{} is valid for FerroWasp configuration schema v2.",
                        path.display()
                    );
                })
            }
            ConfigCommand::Apply { path } => {
                let config = FerroConfig::from_toml_file(path)?;
                let readback = apply(cli, timeout, &config)?;
                emit(cli.format, "config.apply", &readback, || {
                    println!("Configuration was validated, persisted, and verified by readback.");
                })
            }
            ConfigCommand::Store { name, force } => {
                let mut client = connect(cli, timeout)?;
                let config = client.read_config()?;
                let store = ProfileStore::for_current_user()?;
                let profile = store.store(name, &config, *force)?;
                emit(cli.format, "config.store", &profile, || {
                    println!(
                        "Stored the connected drone configuration as '{}' in {}.",
                        profile.name,
                        profile.path.display()
                    );
                })
            }
            ConfigCommand::Stored => {
                let store = ProfileStore::for_current_user()?;
                let profiles = store.list()?;
                emit(cli.format, "config.stored", &profiles, || {
                    if profiles.is_empty() {
                        println!(
                            "No configuration profiles are stored in {}.",
                            store.root().display()
                        );
                    } else {
                        println!("Stored configuration profiles:");
                        for profile in &profiles {
                            println!("  {:<24} {}", profile.name, profile.path.display());
                        }
                    }
                })
            }
            ConfigCommand::ShowStored { name } => {
                let store = ProfileStore::for_current_user()?;
                let config = store.load(name)?;
                let rendered = config.to_toml()?;
                emit(cli.format, "config.show-stored", &config, || {
                    print!("{rendered}");
                })
            }
            ConfigCommand::Load { name } => {
                let store = ProfileStore::for_current_user()?;
                let config = store.load(name)?;
                let readback = apply(cli, timeout, &config)?;
                emit(cli.format, "config.load", &readback, || {
                    println!(
                        "Profile '{name}' was validated, persisted to the drone, and verified by readback."
                    );
                })
            }
            ConfigCommand::Import { name, path, force } => {
                let config = FerroConfig::from_toml_file(path)?;
                let store = ProfileStore::for_current_user()?;
                let profile = store.store(name, &config, *force)?;
                emit(cli.format, "config.import", &profile, || {
                    println!(
                        "Imported {} as profile '{}' in {}.",
                        path.display(),
                        profile.name,
                        profile.path.display()
                    );
                })
            }
            ConfigCommand::Set { key, value } => {
                let mut client = connect(cli, timeout)?;
                let mut config = client.read_config()?;
                config.set_from_str(*key, value)?;
                let readback = client.apply_config(&config)?;
                let persisted = readback.get(*key).ok_or_else(|| {
                    FerroError::InvalidConfiguration(format!(
                        "{} was absent from configuration readback",
                        key.name()
                    ))
                })?;
                emit(cli.format, "config.set", &readback, || {
                    println!(
                        "{} was persisted as {:.4} and verified by readback.",
                        key, persisted
                    );
                })
            }
            ConfigCommand::Keys => {
                let keys = ConfigKey::ALL.map(|key| key.name());
                emit(cli.format, "config.keys", &keys, print_keys)
            }
        },
        Command::Blackbox(args) => match &args.command {
            BlackboxCommand::Flights => {
                let mut client = connect(cli, timeout)?;
                let catalog = catalog_device(&mut client)?;
                emit(cli.format, "blackbox.flights", &catalog, || {
                    print_flight_catalog(&catalog.flights)
                })
            }
            BlackboxCommand::Download {
                flight,
                output,
                resume,
                ulog,
            } => {
                let selector = FlightSelector::parse(flight)?;
                let mut client = connect(cli, timeout)?;
                let (_, span) = resolve_device_flight(&mut client, selector)?;
                let download =
                    download_flight(&mut client, span, output, *resume, |completed, total| {
                        if cli.format == OutputFormat::Human
                            && (completed == 1 || completed % 128 == 0 || completed == total)
                        {
                            eprintln!(
                                "Downloaded {completed}/{total} pages for flight {}.",
                                span.flight_id
                            );
                        }
                    })?;
                let converted = ulog
                    .as_ref()
                    .map(|ulog_path| {
                        convert_fwbb_to_ulog(
                            output,
                            ulog_path,
                            FlightSelector::Id(span.flight_id),
                            false,
                        )
                    })
                    .transpose()?;
                let report = BlackboxDownloadReport {
                    download,
                    ulog: converted,
                };
                emit(cli.format, "blackbox.download", &report, || {
                    println!(
                        "Downloaded flight {} to {} ({} pages, {} bytes).",
                        report.download.flight_id,
                        report.download.output.display(),
                        report.download.pages,
                        report.download.bytes
                    );
                    if report.download.resumed_pages != 0 {
                        println!(
                            "Resumed after {} previously validated pages.",
                            report.download.resumed_pages
                        );
                    }
                    if let Some(converted) = &report.ulog {
                        print_conversion_summary(converted);
                    }
                })
            }
            BlackboxCommand::DownloadRange {
                from,
                to,
                directory,
                resume,
                ulog,
            } => {
                if *from == 0 {
                    return Err(FerroError::Blackbox(
                        "--from must be a flight ID greater than zero".to_owned(),
                    ));
                }
                let mut client = connect(cli, timeout)?;
                let catalog = catalog_device(&mut client)?;
                let last = catalog
                    .flights
                    .last()
                    .map(|entry| entry.flight.flight_id)
                    .ok_or_else(|| FerroError::Blackbox("no stored flights".to_owned()))?;
                let end = match FlightSelector::parse(to)? {
                    FlightSelector::Latest => last,
                    FlightSelector::Id(id) => id,
                };
                if end < *from {
                    return Err(FerroError::Blackbox(
                        "--to must not be less than --from".to_owned(),
                    ));
                }
                let selected = catalog
                    .flights
                    .iter()
                    .filter(|entry| (*from..=end).contains(&entry.flight.flight_id))
                    .cloned()
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    return Err(FerroError::Blackbox(format!(
                        "no stored flights are available in range {from}..={end}"
                    )));
                }
                let mut reports = Vec::with_capacity(selected.len());
                for entry in selected {
                    let id = entry.flight.flight_id;
                    let raw = directory.join(format!("flight-{id}.fwbb"));
                    let download = download_flight(
                        &mut client,
                        entry.flight,
                        &raw,
                        *resume,
                        |completed, total| {
                            if cli.format == OutputFormat::Human
                                && (completed == 1 || completed % 128 == 0 || completed == total)
                            {
                                eprintln!("Downloaded {completed}/{total} pages for flight {id}.");
                            }
                        },
                    )?;
                    let converted = if *ulog {
                        Some(convert_fwbb_to_ulog(
                            &raw,
                            &directory.join(format!("flight-{id}.ulg")),
                            FlightSelector::Id(id),
                            false,
                        )?)
                    } else {
                        None
                    };
                    reports.push(BlackboxDownloadReport {
                        download,
                        ulog: converted,
                    });
                }
                emit(cli.format, "blackbox.download-range", &reports, || {
                    println!(
                        "Downloaded {} flight(s) from {} through {}.",
                        reports.len(),
                        from,
                        end
                    );
                    for report in &reports {
                        println!(
                            "  flight {}: {}",
                            report.download.flight_id,
                            report.download.output.display()
                        );
                    }
                })
            }
            BlackboxCommand::Erase { confirm } => {
                if !confirm {
                    return Err(FerroError::Blackbox(
                        "refusing to erase every onboard log without --confirm".to_owned(),
                    ));
                }
                let mut client = connect(cli, timeout)?;
                let before = client.log_info()?;
                let human_output = cli.format == OutputFormat::Human;
                client.erase_logs_with_progress(|elapsed| {
                    if !human_output {
                        return;
                    }
                    if elapsed.is_zero() {
                        eprintln!(
                            "Onboard erase started for {} used pages. Keep USB connected; this may take several minutes.",
                            before.used_pages
                        );
                    } else {
                        eprintln!(
                            "Still erasing onboard logs ({} seconds elapsed)...",
                            elapsed.as_secs()
                        );
                    }
                })?;
                let info = client.log_info()?;
                if info.used_pages != 0 {
                    return Err(FerroError::VerificationFailed {
                        details: format!(
                            "erase completed but device still reports {} used pages",
                            info.used_pages
                        ),
                    });
                }
                emit(cli.format, "blackbox.erase", &info, || {
                    println!("All onboard flight logs were erased and empty storage was verified.");
                })
            }
        },
        Command::Convert(args) => {
            let selector = FlightSelector::parse(&args.flight)?;
            let output = args
                .output
                .clone()
                .unwrap_or_else(|| args.input.with_extension("ulg"));
            let summary = convert_fwbb_to_ulog(&args.input, &output, selector, args.force)?;
            emit(cli.format, "convert", &summary, || {
                print_conversion_summary(&summary)
            })
        }
        Command::Flash(args) => {
            let profile = args.board.profile();
            let bundled = if args.firmware.is_none() {
                Some(find_bundled_firmware(&profile)?)
            } else {
                None
            };
            let firmware = args
                .firmware
                .clone()
                .or_else(|| bundled.as_ref().map(|release| release.path.clone()))
                .ok_or_else(|| FerroError::InvalidFirmware {
                    reason: "no firmware image was selected".to_owned(),
                })?;
            let image = prepare_elf(&firmware, &profile)?;
            if args.dry_run {
                return emit(cli.format, "flash.validate", &image, || {
                    println!("Firmware image is valid for {}.", profile.display_name);
                    if let Some(release) = &bundled {
                        println!(
                            "Bundled release: {} ({})",
                            release.release_version, release.git_commit
                        );
                        println!("SHA-256: {}", release.sha256);
                    }
                    println!("Flash base:  {:#010x}", image.base_address);
                    println!("Image bytes: {}", image.image_size);
                    println!("Reset vector: {:#010x}", image.reset_vector);
                    println!("Dry run: MCU flash was not accessed.");
                });
            }
            if !args.yes {
                return run_flash_wizard(cli, &firmware, &image, &profile);
            }
            let mut progress = |event: FlashProgress| {
                if cli.format == OutputFormat::Human {
                    print_flash_progress(&event);
                }
            };
            let result = flash_firmware(&image, &profile, &mut progress)?;
            emit(cli.format, "flash", &result, || {
                println!(
                    "Flashed {} bytes at {:#010x} for {}.",
                    result.bytes_written, result.base_address, profile.display_name
                );
                println!("dfu-util completed successfully and requested application start.");
                println!(
                    "For unattended mode, confirm BOOT0 is released before the controller restarts."
                );
            })
        }
        Command::Dfu(args) => match args.command {
            DfuCommand::List => {
                let detection = detect_dfu()?;
                emit(cli.format, "dfu.list", &detection, || print_dfu(&detection))
            }
        },
        Command::Doctor => {
            let ports = discover_ports()?;
            let count = ports.iter().filter(|port| port.is_ferrowasp).count();
            let (dfu, dfu_error) = match detect_dfu() {
                Ok(detection) => (Some(detection), None),
                Err(error) => (None, Some(error.to_string())),
            };
            let report = DoctorReport {
                application: "FerroConfigurator",
                version: env!("CARGO_PKG_VERSION"),
                operating_system: std::env::consts::OS,
                architecture: std::env::consts::ARCH,
                serial_ports: ports,
                ferrowasp_devices: count,
                dfu,
                dfu_error,
                guidance: vec![
                    "Build FerroWasp with flash_storage to expose read-only configuration.",
                    "Build with flash_writes to enable disarmed-only persistence.",
                    "Use --port COMx when USB metadata is unavailable or ambiguous.",
                    "ROM DFU mode is 0483:df11 and may require a one-time WinUSB driver association.",
                ],
            };
            emit(cli.format, "doctor", &report, || print_doctor(&report))
        }
    }
}

fn run_flash_wizard(
    cli: &Cli,
    firmware: &Path,
    image: &PreparedImage,
    profile: &BoardProfile,
) -> Result<(), FerroError> {
    if cli.format != OutputFormat::Human
        || !io::stdin().is_terminal()
        || !io::stdout().is_terminal()
    {
        return Err(FerroError::InvalidConfiguration(
            "interactive flashing requires a terminal and human output; use --dry-run to validate, then --yes for deliberate unattended flashing"
                .to_owned(),
        ));
    }

    let image_end = image
        .base_address
        .saturating_add(u32::try_from(image.image_size).unwrap_or(u32::MAX));
    println!("FerroConfigurator STM32 DFU flash wizard");
    println!("========================================");
    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[0]);
    println!("  Firmware:   {}", firmware.display());
    println!("  Board:      {}", profile.display_name);
    println!(
        "  Flash:      {:#010x}..{:#010x} ({} bytes)",
        image.base_address, image_end, image.image_size
    );
    println!("  Reset:      {:#010x}", image.reset_vector);
    println!();
    println!("  WARNING: This replaces the firmware currently in MCU application flash.");
    println!("  Remove all propellers and keep the flight controller on a safe bench.");
    prompt_enter("Press Enter when you have checked the board and removed the propellers...")?;

    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[1]);
    println!("  Unplug the USB cable from the flight controller now.");
    prompt_enter("Press Enter after USB is disconnected...")?;

    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[2]);
    println!("  1. Hold the BOOT0 button, or bridge the board's BOOT0 pads.");
    println!("  2. While BOOT0 is held, reconnect the USB cable.");
    println!("  3. Wait one second, then release the button or remove the temporary bridge.");
    prompt_enter("Press Enter after reconnecting in BOOT0 mode; detection will begin...")?;

    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[3]);
    println!("  Waiting up to 45 seconds for STM32 BOOTLOADER (0483:df11)...");
    let detection = wait_for_dfu_device(Duration::from_secs(45))?;
    println!("  Detected: {}", detection.devices[0]);
    println!("  Flasher:  {}", detection.utility_version);

    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[4]);
    println!("  Keep USB connected until FerroConfigurator says programming is complete.");
    let confirmation =
        prompt_line("Type FLASH to erase/program application flash, or anything else to cancel: ")?;
    if confirmation.trim() != "FLASH" {
        return Err(FerroError::InvalidConfiguration(
            "flash cancelled; MCU flash was not changed".to_owned(),
        ));
    }

    println!();
    println!("{}", FLASH_WIZARD_STEP_TITLES[5]);
    let mut progress = |event: FlashProgress| print_flash_progress(&event);
    let result = flash_firmware(image, profile, &mut progress)?;
    println!(
        "Successfully flashed {} bytes at {:#010x}.",
        result.bytes_written, result.base_address
    );
    finish_flash_guidance(Duration::from_secs(10));
    Ok(())
}

fn prompt_enter(prompt: &str) -> Result<(), FerroError> {
    let response = prompt_line(prompt)?;
    drop(response);
    Ok(())
}

fn prompt_line(prompt: &str) -> Result<String, FerroError> {
    print!("{prompt}");
    io::stdout().flush().map_err(|error| FerroError::Dfu {
        operation: "write interactive flash prompt".to_owned(),
        reason: error.to_string(),
    })?;
    let mut response = String::new();
    let bytes = io::stdin()
        .read_line(&mut response)
        .map_err(|error| FerroError::Dfu {
            operation: "read interactive flash confirmation".to_owned(),
            reason: error.to_string(),
        })?;
    if bytes == 0 {
        return Err(FerroError::InvalidConfiguration(
            "flash cancelled because interactive input ended".to_owned(),
        ));
    }
    Ok(response)
}

fn wait_for_dfu_device(timeout: Duration) -> Result<DfuDetection, FerroError> {
    let deadline = Instant::now() + timeout;
    loop {
        let detection = detect_dfu()?;
        match detection.devices.len() {
            1 => return Ok(detection),
            count if count > 1 => {
                return Err(FerroError::MultipleDfuDevices {
                    devices: detection.devices,
                });
            }
            _ if Instant::now() >= deadline => return Err(FerroError::DfuDeviceNotFound),
            _ => thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn finish_flash_guidance(timeout: Duration) {
    println!("  Programming is complete. It is now safe to disconnect USB.");
    println!("  Waiting briefly for FerroWasp to restart in normal USB mode...");
    let deadline = Instant::now() + timeout;
    loop {
        match discover_ports() {
            Ok(ports) => {
                if let Some(port) = ports.into_iter().find(|port| port.is_ferrowasp) {
                    println!("  FerroWasp restarted normally on {}.", port.port);
                    println!("  No USB power cycle is required.");
                    return;
                }
            }
            Err(error) => {
                eprintln!("  Could not check normal USB ports: {error}");
                println!(
                    "  Disconnect USB, remove any BOOT0 bridge, and reconnect with BOOT0 released."
                );
                return;
            }
        }
        if Instant::now() >= deadline {
            println!("  No normal FerroWasp USB port appeared within 10 seconds.");
            println!(
                "  Disconnect USB, remove any BOOT0 bridge, and reconnect with BOOT0 released."
            );
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn connect(
    cli: &Cli,
    timeout: Duration,
) -> Result<
    ferro_configurator_core::FerroClient<ferro_configurator_core::SerialTransport>,
    FerroError,
> {
    let selector = cli.port.as_ref().map_or(DeviceSelector::Auto, |port| {
        DeviceSelector::Port(port.clone())
    });
    open_device(selector, timeout)
}

fn apply(cli: &Cli, timeout: Duration, config: &FerroConfig) -> Result<FerroConfig, FerroError> {
    let mut client = connect(cli, timeout)?;
    client.apply_config(config)
}

fn emit<T: Serialize>(
    format: OutputFormat,
    operation: &str,
    value: &T,
    human: impl FnOnce(),
) -> Result<(), FerroError> {
    match format {
        OutputFormat::Human => human(),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&Success {
                ok: true,
                operation,
                result: value,
            })
            .map_err(|error| FerroError::InvalidConfiguration(error.to_string()))?
        ),
    }
    Ok(())
}

fn print_device_report(report: &DeviceReport) {
    println!("Port:       {}", report.port);
    println!("Board:      {}", report.board);
    println!("Protocol:   {}", report.protocol);
    println!(
        "Flash:      {} ({} bytes)",
        report.flash.jedec_id, report.flash.capacity_bytes
    );
    println!("Flash ready: {}", yes_no(report.flash.ready));
    if let Some(status) = &report.status {
        println!("Armed:      {}", yes_no(status.armed));
        println!("Armable:    {}", yes_no(status.armable));
        println!(
            "IMU:        {} ({})",
            status.imu,
            if status.imu_ready {
                "ready"
            } else {
                "not ready"
            }
        );
        println!(
            "Battery:    {:.1} V",
            status.battery_decivolts as f32 / 10.0
        );
    } else {
        println!("Live status: unavailable within the configured timeout");
    }
}

fn print_doctor(report: &DoctorReport) {
    println!("FerroConfigurator: {}", report.version);
    println!(
        "Platform:          {} {}",
        report.operating_system, report.architecture
    );
    println!("Serial ports:      {}", report.serial_ports.len());
    println!("FerroWasp devices: {}", report.ferrowasp_devices);
    match (&report.dfu, &report.dfu_error) {
        (Some(dfu), _) => {
            println!("Bundled flasher:   {}", dfu.utility_version);
            println!("STM32 DFU devices: {}", dfu.devices.len());
        }
        (_, Some(error)) => println!("Bundled flasher:   unavailable ({error})"),
        _ => {}
    }
    for guidance in &report.guidance {
        println!("- {guidance}");
    }
}

fn print_dfu(detection: &DfuDetection) {
    println!("Utility: {}", detection.utility.display());
    println!("Version: {}", detection.utility_version);
    if detection.devices.is_empty() {
        println!("No STM32 ROM DFU device (0483:df11) is connected.");
    } else {
        for device in &detection.devices {
            println!("Device:  {device}");
        }
    }
}

fn print_flash_progress(event: &FlashProgress) {
    match event {
        FlashProgress::ImageValidated {
            bytes,
            base_address,
        } => eprintln!("Validated {bytes} bytes at {base_address:#010x}."),
        FlashProgress::DfuDetected { device } => eprintln!("Using {device}"),
        FlashProgress::Programming { bytes } => {
            eprintln!("Programming {bytes} bytes through STM32 ROM DFU. Do not disconnect USB...")
        }
        FlashProgress::Completed => eprintln!("DFU programming completed."),
    }
}

fn print_flight_catalog(entries: &[CatalogEntry]) {
    if entries.is_empty() {
        println!("No stored flights.");
        return;
    }
    let mut previous_session = Some(u32::MAX);
    for entry in entries {
        if entry.boot_session != previous_session {
            match entry.boot_session {
                Some(session) => println!("boot {session}:"),
                None => println!("boot unknown (recorded before boot markers):"),
            }
            previous_session = entry.boot_session;
        }
        let flight = entry.flight;
        println!(
            "  flight {}: pages {}..{} ({} pages, {} bytes)",
            flight.flight_id,
            flight.start_page,
            flight.end_page - 1,
            flight.page_count(),
            flight.byte_count()
        );
    }
}

fn print_conversion_summary(summary: &ConversionSummary) {
    println!(
        "Converted flight {} to {} ({} samples, {:.3} s, {} dropouts, {} bytes).",
        summary.flight_id,
        summary.output.display(),
        summary.sample_count,
        summary.duration_us as f64 / 1_000_000.0,
        summary.dropout_count,
        summary.output_bytes
    );
}

fn print_keys() {
    println!("Firmware-whitelisted configuration:");
    for key in ConfigKey::ALL {
        let spec = key.value_spec();
        let kind = if spec.integer { "integer" } else { "finite" };
        println!(
            "  {:<24} {} {}..={}",
            key.name(),
            kind,
            spec.minimum,
            spec.maximum
        );
    }
    println!("  each maximum rate must also be greater than or equal to its center rate");
}

fn print_error(format: OutputFormat, error: &FerroError) {
    let hint = error_hint(error);
    match format {
        OutputFormat::Human => {
            eprintln!("error: {error}");
            eprintln!("hint: {hint}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&ErrorOutput {
                ok: false,
                error: &error.to_string(),
                hint,
            })
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialization failure\"}".to_owned())
        ),
    }
}

fn error_hint(error: &FerroError) -> &'static str {
    match error {
        FerroError::InvalidConfiguration(message) if message.contains("interactive flashing") => {
            "Run this command in PowerShell for guided flashing, or use --dry-run and then --yes for automation."
        }
        FerroError::InvalidConfiguration(message) if message.contains("flash cancelled") => {
            "No flash write was started; rerun the command when you are ready."
        }
        FerroError::InvalidConfiguration(_) => {
            "Run `config validate <file>` for configuration files and review the reported field or value."
        }
        FerroError::ReadConfig { .. } => {
            "Check the file or profile name and run `config stored` to list saved profiles."
        }
        FerroError::WriteConfig { .. } => {
            "Choose another file/profile name, or pass --force only when replacement is intentional."
        }
        FerroError::DeviceNotFound => {
            "Connect a FerroWasp USB CDC build, run `device list --all`, or pass --port COMx."
        }
        FerroError::MultipleDevicesFound { .. } => {
            "Select the intended controller with --port COMx."
        }
        FerroError::DeviceRejected { message, .. } if message.contains("armed") => {
            "Disarm the vehicle, remove propellers for bench work, then retry."
        }
        FerroError::DeviceRejected { message, .. } if message.contains("flash_writes") => {
            "Install a FerroWasp firmware build with the flash_writes feature."
        }
        FerroError::Timeout { .. } => {
            "Check the COM port, close other serial tools, and confirm firmware includes flash_storage."
        }
        FerroError::VerificationFailed { .. } => {
            "Do not fly with an unverified change; reconnect and run `config show`."
        }
        FerroError::DfuDeviceNotFound => {
            "Disconnect USB, hold BOOT0, reconnect USB, wait one second, release BOOT0, then retry."
        }
        FerroError::MultipleDfuDevices { .. } => {
            "Disconnect every STM32 DFU device except the intended flight controller."
        }
        FerroError::Dfu { .. } => {
            "Confirm the STM32 BOOTLOADER device uses WinUSB, close other USB tools, and retry."
        }
        FerroError::InvalidFirmware { .. } => {
            "Use the packaged FerroWasp release ZIP, or provide a deliberate developer ELF and verify --board."
        }
        FerroError::Profile(_) => {
            "Use a profile name containing only letters, numbers, '-' or '_'; run `config stored` to list saved profiles."
        }
        FerroError::Blackbox(_) => {
            "Keep the aircraft disarmed, verify `blackbox flights`, and retry with the exact flight ID."
        }
        FerroError::FileIo { .. } => {
            "Check the output path and permissions; use --resume only with the matching `.part` download."
        }
        FerroError::Ulog(_) => {
            "Preserve the raw FWBB file, verify its page CRCs and flight ID, then retry conversion."
        }
        _ => "Run `ferro-configurator doctor` for host and USB diagnostics.",
    }
}

fn exit_code(error: &FerroError) -> u8 {
    match error {
        FerroError::InvalidConfiguration(_)
        | FerroError::ReadConfig { .. }
        | FerroError::WriteConfig { .. }
        | FerroError::Profile(_)
        | FerroError::Blackbox(_)
        | FerroError::Ulog(_) => 2,
        FerroError::DeviceNotFound | FerroError::MultipleDevicesFound { .. } => 3,
        FerroError::DeviceRejected { .. } => 5,
        FerroError::VerificationFailed { .. } => 8,
        FerroError::InvalidFirmware { .. } => 6,
        FerroError::DfuUtilityMissing
        | FerroError::DfuDeviceNotFound
        | FerroError::MultipleDfuDevices { .. }
        | FerroError::Dfu { .. } => 7,
        _ => 4,
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_wizard_titles_are_ascii_for_windows_consoles() {
        assert!(
            FLASH_WIZARD_STEP_TITLES
                .iter()
                .all(|title| title.is_ascii())
        );
    }
}

#![forbid(unsafe_code)]

use std::{
    fs::File,
    io::{self, BufWriter, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use ferro_configurator_core::{
    BoardProfile, CatalogEntry, CheckState, ConfigKey, ConversionSummary, DeviceSelector,
    DfuDetection, DownloadSummary, FerroConfig, FerroError, FlashInfo, FlashProgress,
    FlightSelector, PortInfo, PreparedImage, ProfileStore, SERIAL_FUNCTIONS, SerialBindings,
    StatusSnapshot, SyncedFlight, catalog_device, config::lpf_alpha_for_corner,
    convert_fwbb_to_ulog, detect_dfu, discover_ports, download_flight, find_bundled_firmware,
    flash_firmware, open_device, prearm_checks, prepare_elf, resolve_device_flight, sync_flights,
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
    /// Observe a controller for bench evidence, without commanding it.
    Bench(BenchArgs),
}

#[derive(Debug, Args)]
struct BenchArgs {
    #[command(subcommand)]
    command: BenchCommand,
}

#[derive(Debug, Subcommand)]
enum BenchCommand {
    /// Record live status and every transition in it for a fixed duration.
    ///
    /// Read-only: it polls the controller's own status and never writes. The
    /// operator still performs every hardware action; this only removes the
    /// transcription from the evidence.
    Watch {
        /// How long to observe.
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..))]
        seconds: u64,
        /// Write one JSON object per sample to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Measure whether the control loop is keeping up with its configured rate.
    ///
    /// Read-only. The controller reports both the rate it was built for and the
    /// number of control cycles it has completed, so the shortfall between them
    /// is the answer to "can this board sustain this loop rate".
    Loop {
        /// How long to measure. Longer is more precise; the uptime the
        /// controller reports has millisecond resolution.
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(2..))]
        seconds: u64,
    },
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
    /// Store every flight no host has stored yet, and acknowledge each one.
    ///
    /// Works over USB or over a UART bound to `configurator`, such as a
    /// Bluetooth serial module; select that link with --port. Each flight
    /// lands in its own file named after the flight and its first page, so an
    /// interrupted sync picks up where it stopped.
    Sync {
        #[arg(long)]
        directory: PathBuf,
        /// Afterwards, erase the onboard log. The controller refuses unless
        /// every flight on it is acknowledged.
        #[arg(long)]
        erase: bool,
    },
    /// Erase every onboard log after explicit confirmation.
    Erase {
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Debug, Serialize)]
struct BlackboxSyncReport {
    flights: Vec<SyncedFlight>,
    erased: bool,
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
    /// Show which function each serial port is bound to.
    Ports,
    /// Bind a serial port to a function, persist, and verify by readback.
    ///
    /// The controller applies bindings at boot, so reboot it afterwards.
    Bind {
        /// The UART by its number on the chip, as `config ports` lists it.
        port: String,
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(SERIAL_FUNCTIONS))]
        function: String,
    },
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
                let loop_hz = match cli.format {
                    OutputFormat::Human => client.read_status().ok().map(|s| s.control_loop_hz),
                    OutputFormat::Json => None,
                };
                emit(cli.format, "config.show", &config, || {
                    print!("{rendered}");
                    print_lpf_note(config.imu_lpf_hz, loop_hz);
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
            ConfigCommand::Ports => {
                let mut client = connect(cli, timeout)?;
                let bindings = client.read_serial_bindings()?;
                emit(cli.format, "config.ports", &bindings, || {
                    print_serial_bindings(&bindings)
                })
            }
            ConfigCommand::Bind { port, function } => {
                let mut client = connect(cli, timeout)?;
                let readback = client.apply_serial_binding(port, function)?;
                emit(cli.format, "config.bind", &readback, || {
                    println!(
                        "{port} was saved as {function} and verified by readback. \
                         Reboot the controller to apply it."
                    );
                })
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
            BlackboxCommand::Sync { directory, erase } => {
                let mut client = connect(cli, timeout)?;
                let human_output = cli.format == OutputFormat::Human;
                let flights = sync_flights(&mut client, directory, |id, completed, total| {
                    if human_output
                        && (completed == 1 || completed % 128 == 0 || completed == total)
                    {
                        eprintln!("Downloaded {completed}/{total} pages for flight {id}.");
                    }
                })?;
                if *erase {
                    client.erase_synced_logs_with_progress(|elapsed| {
                        if human_output && elapsed.is_zero() {
                            eprintln!(
                                "Every flight is stored; erasing the onboard log. This may take several minutes."
                            );
                        } else if human_output {
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
                }
                let report = BlackboxSyncReport {
                    flights,
                    erased: *erase,
                };
                emit(cli.format, "blackbox.sync", &report, || {
                    if report.flights.is_empty() {
                        println!("Every onboard flight was already stored.");
                    }
                    for flight in &report.flights {
                        let note = if flight.already_stored {
                            " (already stored)"
                        } else {
                            ""
                        };
                        println!(
                            "  flight {}: {}{note}",
                            flight.flight_id,
                            flight.output.display()
                        );
                    }
                    if report.erased {
                        println!("The onboard log was erased and empty storage was verified.");
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
        Command::Bench(args) => match &args.command {
            BenchCommand::Watch { seconds, out } => {
                bench_watch(cli, timeout, *seconds, out.as_ref())
            }
            BenchCommand::Loop { seconds } => bench_loop(cli, timeout, *seconds),
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
                    "Foxeer firmware includes onboard storage and disarmed-only persistence by default.",
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

/// One polled reading, with the offset from the start of the watch.
#[derive(Debug, Clone, Serialize)]
struct WatchSample {
    elapsed_ms: u64,
    status: StatusSnapshot,
}

/// A change in one of the states an operator would otherwise narrate.
#[derive(Debug, Clone, Serialize)]
struct WatchTransition {
    elapsed_ms: u64,
    field: &'static str,
    from: bool,
    to: bool,
}

#[derive(Debug, Clone, Serialize)]
struct WatchReport {
    requested_seconds: u64,
    /// What the controller actually delivered. It emits status on its own
    /// cadence, so this is the resolution of the evidence: a transition
    /// shorter than one period can be missed entirely.
    observed_rate_hz: f64,
    samples: usize,
    failed_reads: usize,
    armed_ms: u64,
    throttle_max: u32,
    battery_decivolts_min: u32,
    battery_decivolts_max: u32,
    transitions: Vec<WatchTransition>,
    samples_path: Option<String>,
}

/// Records the transitions between two consecutive readings.
fn diff_status(
    elapsed_ms: u64,
    previous: &StatusSnapshot,
    current: &StatusSnapshot,
    into: &mut Vec<WatchTransition>,
) {
    let fields: [(&'static str, bool, bool); 5] = [
        ("armed", previous.armed, current.armed),
        ("armable", previous.armable, current.armable),
        ("arm_switch", previous.arm_switch, current.arm_switch),
        ("rc_valid", previous.rc_valid, current.rc_valid),
        ("imu_ready", previous.imu_ready, current.imu_ready),
    ];
    for (field, from, to) in fields {
        if from != to {
            into.push(WatchTransition {
                elapsed_ms,
                field,
                from,
                to,
            });
        }
    }
}

/// Polls status for a bounded time and reports what changed.
///
/// Nothing here commands the aircraft. A failed read is counted and the watch
/// continues, because a dropped USB line during a bench run is not a reason to
/// discard the evidence either side of it.
fn bench_watch(
    cli: &Cli,
    timeout: Duration,
    seconds: u64,
    out: Option<&PathBuf>,
) -> Result<(), FerroError> {
    let mut client = connect(cli, timeout)?;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);

    let mut writer = match out {
        Some(path) => Some(BufWriter::new(File::create(path).map_err(|error| {
            FerroError::FileIo {
                operation: "create",
                path: path.clone(),
                reason: error.to_string(),
            }
        })?)),
        None => None,
    };

    let mut transitions: Vec<WatchTransition> = Vec::new();
    let mut previous: Option<StatusSnapshot> = None;
    let mut previous_ms: u64 = 0;
    let (mut samples, mut failed, mut armed_ms) = (0usize, 0usize, 0u64);
    let (mut throttle_max, mut vbat_min, mut vbat_max) = (0u32, u32::MAX, 0u32);

    if cli.format == OutputFormat::Human {
        println!("Watching for {seconds}s. The operator performs every action.");
    }

    while Instant::now() < deadline {
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match client.read_status_fresh() {
            Ok(status) => {
                samples += 1;
                // Credit the gap that just elapsed, so armed time reflects the
                // clock rather than an assumed cadence.
                if previous.as_ref().is_some_and(|p| p.armed) {
                    armed_ms += elapsed_ms.saturating_sub(previous_ms);
                }
                previous_ms = elapsed_ms;
                throttle_max = throttle_max.max(status.throttle);
                vbat_min = vbat_min.min(status.battery_decivolts);
                vbat_max = vbat_max.max(status.battery_decivolts);

                if let Some(previous) = previous.as_ref() {
                    let before = transitions.len();
                    diff_status(elapsed_ms, previous, &status, &mut transitions);
                    if cli.format == OutputFormat::Human {
                        for change in &transitions[before..] {
                            println!(
                                "  {:>7.1}s  {} {} -> {}",
                                elapsed_ms as f64 / 1000.0,
                                change.field,
                                change.from,
                                change.to
                            );
                        }
                    }
                }

                if let Some(writer) = writer.as_mut() {
                    let sample = WatchSample {
                        elapsed_ms,
                        status: status.clone(),
                    };
                    let line = serde_json::to_string(&sample).unwrap_or_default();
                    let _ = writeln!(writer, "{line}");
                }
                previous = Some(status);
            }
            Err(_) => failed += 1,
        }
    }
    let elapsed_total_ms = started.elapsed().as_millis() as u64;

    if let Some(mut writer) = writer {
        let _ = writer.flush();
    }

    let report = WatchReport {
        requested_seconds: seconds,
        observed_rate_hz: if elapsed_total_ms > 0 {
            samples as f64 * 1000.0 / elapsed_total_ms as f64
        } else {
            0.0
        },
        samples,
        failed_reads: failed,
        armed_ms,
        throttle_max,
        battery_decivolts_min: if samples == 0 { 0 } else { vbat_min },
        battery_decivolts_max: vbat_max,
        transitions,
        samples_path: out.map(|path| path.display().to_string()),
    };

    emit(cli.format, "bench.watch", &report, || {
        println!();
        println!(
            "Samples:     {} at {:.2} Hz ({} failed reads)",
            report.samples, report.observed_rate_hz, report.failed_reads
        );
        println!("Armed:       {:.1} s", report.armed_ms as f64 / 1000.0);
        println!("Throttle:    max {}", report.throttle_max);
        println!(
            "Battery:     {:.1}..{:.1} V",
            report.battery_decivolts_min as f64 / 10.0,
            report.battery_decivolts_max as f64 / 10.0
        );
        println!("Transitions: {}", report.transitions.len());
        if report.observed_rate_hz < 5.0 {
            println!();
            println!(
                "Note: the controller emits status at {:.2} Hz, so anything shorter than\n\
                 {:.1} s can pass unseen. This records state, not events. For arming and\n\
                 abort transitions, the RTT log is the complete record.",
                report.observed_rate_hz,
                if report.observed_rate_hz > 0.0 {
                    1.0 / report.observed_rate_hz
                } else {
                    0.0
                }
            );
        }
        if let Some(path) = &report.samples_path {
            println!("Samples written to {path}");
        }
    })
}

/// Says which coefficient the stored corner produces on the controller.
///
/// The configuration carries a frequency, which is what an operator can reason
/// about. The firmware turns it into a one-pole coefficient against its own
/// loop rate, and seeing that number is useful when comparing against logs or
/// against Betaflight, where the coefficient is what gets quoted. The rate is
/// the one the controller reports in its status line; the host keeps no copy,
/// because its copy has been wrong twice.
fn print_lpf_note(corner_hz: f32, loop_hz: Option<u32>) {
    let Some(loop_hz) = loop_hz else {
        println!(
            "# imu_lpf_hz {corner_hz} is a corner; the controller did not report its loop rate."
        );
        return;
    };
    if let Some(alpha) = lpf_alpha_for_corner(corner_hz, loop_hz as f32) {
        println!(
            "# imu_lpf_hz {corner_hz} is a one-pole alpha of {alpha:.3} at the controller's {loop_hz} Hz loop rate."
        );
    }
}

/// What the loop achieved against what it was asked for.
#[derive(Debug, Clone, Serialize)]
struct LoopReport {
    configured_hz: u32,
    achieved_hz: f64,
    shortfall_percent: f64,
    cycles: u32,
    span_ms: u32,
    keeping_up: bool,
}

/// Measures the control loop against its own configured rate.
///
/// Both numbers come from the controller: the rate it was built for, and the
/// cycles it has completed since boot. A loop that cannot sustain its rate
/// completes fewer cycles than the clock allows, and that shortfall is the
/// signal - it appears before anything else misbehaves.
fn bench_loop(cli: &Cli, timeout: Duration, seconds: u64) -> Result<(), FerroError> {
    let mut client = connect(cli, timeout)?;
    let first = client.read_status_fresh()?;
    if first.control_loop_hz == 0 {
        return Err(FerroError::UnexpectedResponse {
            operation: "control loop rate".to_owned(),
            response: "this firmware does not report ctl_hz; reflash to measure the loop"
                .to_owned(),
        });
    }
    if cli.format == OutputFormat::Human {
        println!(
            "Measuring for {seconds}s against the reported {} Hz.",
            first.control_loop_hz
        );
    }
    // Read continuously rather than sleeping and reading once. The port
    // buffers status lines, so a read after a sleep returns the oldest queued
    // line and measures a fraction of the requested span.
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut last = first.clone();
    while Instant::now() < deadline {
        match client.read_status_fresh() {
            Ok(status) => last = status,
            Err(_) => continue,
        }
    }

    let span_ms = last.uptime_ms.saturating_sub(first.uptime_ms);
    let cycles = last.control_sequence.saturating_sub(first.control_sequence);
    if span_ms == 0 {
        return Err(FerroError::UnexpectedResponse {
            operation: "control loop rate".to_owned(),
            response: "the controller reported no elapsed time".to_owned(),
        });
    }
    let achieved = f64::from(cycles) * 1000.0 / f64::from(span_ms);
    let configured = f64::from(first.control_loop_hz);
    let shortfall = (configured - achieved) / configured * 100.0;
    // A tenth of a percent is well inside the millisecond resolution of the
    // reported uptime, so anything under it is measurement noise.
    let keeping_up = shortfall < 0.1;

    let report = LoopReport {
        configured_hz: first.control_loop_hz,
        achieved_hz: achieved,
        shortfall_percent: shortfall,
        cycles,
        span_ms,
        keeping_up,
    };

    emit(cli.format, "bench.loop", &report, || {
        println!();
        println!("Configured:  {} Hz", report.configured_hz);
        println!("Achieved:    {:.1} Hz", report.achieved_hz);
        println!(
            "Shortfall:   {:.3} %  ({} cycles in {} ms)",
            report.shortfall_percent, report.cycles, report.span_ms
        );
        println!();
        if report.keeping_up {
            println!("The loop is keeping up.");
        } else {
            println!(
                "The loop is NOT keeping up: {:.0} cycles per second are being lost.",
                f64::from(report.configured_hz) - report.achieved_hz
            );
        }
    })
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
        if !status.armed {
            print_prearm_checks(status);
        }
    } else {
        println!("Live status: unavailable within the configured timeout");
    }
}

fn print_prearm_checks(status: &StatusSnapshot) {
    println!("Pre-arm checks:");
    for check in prearm_checks(status) {
        let mark = match check.state {
            CheckState::Pass => "ok  ",
            CheckState::Fail => "FAIL",
            CheckState::Unknown => "?   ",
        };
        println!("  [{mark}] {}", check.label);
        if check.state == CheckState::Fail {
            println!("         {}", check.hint);
        }
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

fn print_serial_bindings(bindings: &SerialBindings) {
    let source = if bindings.saved {
        "saved"
    } else {
        "board defaults"
    };
    println!("Serial ports ({source}; a change applies after a reboot):");
    for binding in &bindings.ports {
        println!("  {:<8} {}", binding.port, binding.function);
    }
}

fn print_keys() {
    println!("Firmware-whitelisted configuration:");
    for key in ConfigKey::ALL {
        if key == ConfigKey::RcProtocol {
            println!("  {:<24} sbus or crsf, applied at boot", key.name());
            continue;
        }
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
    println!("  rc_map is the channels for roll, pitch, throttle and yaw, like 1234 for AETR");
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
        FerroError::DeviceRejected { message, .. } if message.contains("storage") => {
            "Reconnect the standard Foxeer firmware and confirm onboard flash initialized successfully."
        }
        FerroError::Timeout { .. } => {
            "Check the COM port, close other serial tools, and confirm the standard Foxeer firmware booted."
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

use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use tempfile::Builder;

use crate::{
    error::{FerroError, Result},
    firmware::{board::BoardProfile, elf::PreparedImage},
};

const STM32_DFU_ID: &str = "0483:df11";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DfuDetection {
    pub utility: PathBuf,
    pub utility_version: String,
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum FlashProgress {
    ImageValidated { bytes: usize, base_address: u32 },
    DfuDetected { device: String },
    Programming { bytes: usize },
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlashResult {
    pub board_id: String,
    pub base_address: u32,
    pub bytes_written: usize,
    pub dfu_device: String,
    pub utility_version: String,
    pub utility_output: String,
}

pub fn detect_dfu() -> Result<DfuDetection> {
    detect_with(&resolve_dfu_util()?)
}

pub fn flash_firmware(
    image: &PreparedImage,
    profile: &BoardProfile,
    progress: &mut dyn FnMut(FlashProgress),
) -> Result<FlashResult> {
    validate_prepared_image(image, profile)?;
    progress(FlashProgress::ImageValidated {
        bytes: image.bytes.len(),
        base_address: image.base_address,
    });

    let detection = detect_dfu()?;
    let device = match detection.devices.as_slice() {
        [] => return Err(FerroError::DfuDeviceNotFound),
        [device] => device.clone(),
        devices => {
            return Err(FerroError::MultipleDfuDevices {
                devices: devices.to_vec(),
            });
        }
    };
    progress(FlashProgress::DfuDetected {
        device: device.clone(),
    });

    let mut binary = Builder::new()
        .prefix("ferro-configurator-")
        .suffix(".bin")
        .tempfile()
        .map_err(|error| FerroError::Dfu {
            operation: "create temporary dense firmware image".to_owned(),
            reason: error.to_string(),
        })?;
    binary
        .write_all(&image.bytes)
        .and_then(|_| binary.flush())
        .map_err(|error| FerroError::Dfu {
            operation: "write temporary dense firmware image".to_owned(),
            reason: error.to_string(),
        })?;

    let arguments = build_flash_arguments(image, profile, binary.path());
    progress(FlashProgress::Programming {
        bytes: image.bytes.len(),
    });
    let output = Command::new(&detection.utility)
        .args(&arguments)
        .output()
        .map_err(|error| FerroError::Dfu {
            operation: "launch bundled dfu-util".to_owned(),
            reason: error.to_string(),
        })?;
    let combined = combine_output(&output.stdout, &output.stderr);
    if !output.status.success() {
        return Err(FerroError::Dfu {
            operation: "program STM32 internal flash".to_owned(),
            reason: format!(
                "dfu-util exited with {}; {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "no exit code".to_owned(), |code| code.to_string()),
                combined.trim()
            ),
        });
    }
    progress(FlashProgress::Completed);
    Ok(FlashResult {
        board_id: image.board_id.clone(),
        base_address: image.base_address,
        bytes_written: image.bytes.len(),
        dfu_device: device,
        utility_version: detection.utility_version,
        utility_output: combined,
    })
}

fn validate_prepared_image(image: &PreparedImage, profile: &BoardProfile) -> Result<()> {
    if image.board_id != profile.id || image.base_address != profile.flash_range.start {
        return Err(FerroError::InvalidFirmware {
            reason: "prepared image does not match the selected board profile".to_owned(),
        });
    }
    let length = u32::try_from(image.bytes.len()).map_err(|_| FerroError::InvalidFirmware {
        reason: "prepared image is too large".to_owned(),
    })?;
    let end =
        image
            .base_address
            .checked_add(length)
            .ok_or_else(|| FerroError::InvalidFirmware {
                reason: "prepared image address overflows".to_owned(),
            })?;
    if image.bytes.is_empty() || end > profile.flash_range.end {
        return Err(FerroError::InvalidFirmware {
            reason: "prepared image falls outside the selected board flash".to_owned(),
        });
    }
    Ok(())
}

fn resolve_dfu_util() -> Result<PathBuf> {
    if let Some(path) = env::var_os("FERRO_CONFIGURATOR_DFU_UTIL") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(executable) = env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let adjacent = directory.join(dfu_executable_name());
        if adjacent.is_file() {
            return Ok(adjacent);
        }
    }

    // This path is only for `cargo run` and tests from the source tree. Release
    // packaging always puts the same binary beside FerroConfigurator.
    let development = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("third_party/dfu-util/windows-x86_64/dfu-util.exe");
    if development.is_file() {
        return Ok(development);
    }

    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(dfu_executable_name());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(FerroError::DfuUtilityMissing)
}

fn dfu_executable_name() -> &'static str {
    if cfg!(windows) {
        "dfu-util.exe"
    } else {
        "dfu-util"
    }
}

fn detect_with(utility: &Path) -> Result<DfuDetection> {
    let output = Command::new(utility)
        .args(["-d", STM32_DFU_ID, "-l"])
        .output()
        .map_err(|error| FerroError::Dfu {
            operation: "enumerate STM32 DFU devices".to_owned(),
            reason: error.to_string(),
        })?;
    let combined = combine_output(&output.stdout, &output.stderr);
    if !output.status.success() {
        return Err(FerroError::Dfu {
            operation: "enumerate STM32 DFU devices".to_owned(),
            reason: combined.trim().to_owned(),
        });
    }
    let utility_version = combined
        .lines()
        .find(|line| line.trim_start().starts_with("dfu-util "))
        .unwrap_or("dfu-util (version unavailable)")
        .trim()
        .to_owned();
    Ok(DfuDetection {
        utility: utility.to_path_buf(),
        utility_version,
        devices: parse_dfu_devices(&combined),
    })
}

fn parse_dfu_devices(output: &str) -> Vec<String> {
    let mut devices = BTreeSet::new();
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("found dfu:") || !lower.contains("[0483:df11]") {
            continue;
        }
        // One physical device can advertise multiple alternate settings.
        // Everything before `alt=` is its stable identity in dfu-util output.
        let identity = line
            .split(", alt=")
            .next()
            .unwrap_or(line)
            .trim()
            .to_owned();
        devices.insert(identity);
    }
    devices.into_iter().collect()
}

fn build_flash_arguments(
    image: &PreparedImage,
    profile: &BoardProfile,
    binary_path: &Path,
) -> Vec<OsString> {
    vec![
        "-d".into(),
        STM32_DFU_ID.into(),
        "-a".into(),
        profile.dfu_alt.to_string().into(),
        "-s".into(),
        format!("0x{:08X}:leave", image.base_address).into(),
        "-D".into(),
        binary_path.as_os_str().to_owned(),
    ]
}

fn combine_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut combined = String::from_utf8_lossy(stdout).into_owned();
    if !combined.is_empty() && !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str(&String::from_utf8_lossy(stderr));
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> PreparedImage {
        PreparedImage {
            board_id: "foxeer-f405-v2".to_owned(),
            base_address: 0x0800_0000,
            entry_address: 0x0800_0101,
            initial_stack_pointer: 0x2002_0000,
            reset_vector: 0x0800_0101,
            source_size: 100,
            image_size: 8,
            bytes: vec![0; 8],
        }
    }

    #[test]
    fn exact_arguments_are_board_scoped_and_paths_remain_separate() {
        let path = Path::new(r"C:\firmware builds\ferrowasp.bin");
        let arguments = build_flash_arguments(&image(), &BoardProfile::FOXEER_F405_V2, path);
        assert_eq!(
            arguments,
            [
                "-d",
                "0483:df11",
                "-a",
                "0",
                "-s",
                "0x08000000:leave",
                "-D",
                r"C:\firmware builds\ferrowasp.bin",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn no_destructive_dfu_switch_can_be_generated() {
        let arguments = build_flash_arguments(
            &image(),
            &BoardProfile::FOXEER_F405_V2,
            Path::new("firmware.bin"),
        );
        let joined = arguments
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        for forbidden in ["mass-erase", "unprotect", "option", "otp"] {
            assert!(!joined.contains(forbidden));
        }
    }

    #[test]
    fn alternate_settings_are_collapsed_to_one_physical_device() {
        let output = r#"
dfu-util 0.11
Found DFU: [0483:df11] ver=2200, devnum=7, cfg=1, intf=0, path="1-2", alt=1, name="OTP", serial="123"
Found DFU: [0483:df11] ver=2200, devnum=7, cfg=1, intf=0, path="1-2", alt=0, name="Internal Flash", serial="123"
"#;
        assert_eq!(parse_dfu_devices(output).len(), 1);
    }

    #[test]
    fn non_stm32_dfu_devices_are_ignored() {
        let output = "Found DFU: [1234:5678] ver=0100, devnum=1, alt=0";
        assert!(parse_dfu_devices(output).is_empty());
    }
}

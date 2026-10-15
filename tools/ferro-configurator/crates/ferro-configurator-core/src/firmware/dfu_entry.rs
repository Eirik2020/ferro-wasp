//! Getting a board into the STM32 ROM DFU bootloader and back.
//!
//! A board running FerroWasp enters it on `reboot dfu`, which the firmware
//! accepts only while disarmed with storage idle. Any other board, or one
//! whose firmware does not answer, enters it with the BOOT button.

use std::{
    thread,
    time::{Duration, Instant},
};

use crate::{
    device::{DeviceSelector, PortInfo, discover_ports, open_device},
    error::{FerroError, Result},
    firmware::{BoardProfile, DfuDetection, detect_dfu},
};

/// How long a board takes from `reboot dfu` to a ROM DFU device the host
/// sees, with margin for the operating system to enumerate it.
pub const DFU_ENTRY_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a board takes to come back as FerroWasp after programming.
pub const RESTART_TIMEOUT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(250);

/// The FerroWasp port of `profile`'s board, when exactly one is connected.
///
/// `None` when no FerroWasp port is connected. A port that reports a
/// different board, or several of this one, is an error: rebooting the wrong
/// board into DFU would flash it with this image.
pub fn find_board_port(profile: &BoardProfile) -> Result<Option<PortInfo>> {
    let ports: Vec<_> = discover_ports()?
        .into_iter()
        .filter(|port| port.is_ferrowasp)
        .collect();
    select_board_port(ports, profile)
}

fn select_board_port(ports: Vec<PortInfo>, profile: &BoardProfile) -> Result<Option<PortInfo>> {
    let (matching, other): (Vec<_>, Vec<_>) = ports
        .into_iter()
        .partition(|port| port.serial_number.as_deref() == Some(profile.usb_serial));
    match (matching.as_slice(), other.as_slice()) {
        ([], []) => Ok(None),
        ([only], []) => Ok(Some(only.clone())),
        ([], [port, ..]) => Err(FerroError::InvalidConfiguration(format!(
            "{} runs FerroWasp for {}, not {}; select the matching board",
            port.port,
            port.board_name().unwrap_or("another board"),
            profile.display_name
        ))),
        _ => Err(FerroError::MultipleDevicesFound {
            ports: matching
                .iter()
                .chain(&other)
                .map(|port| port.port.clone())
                .collect(),
        }),
    }
}

/// Asks the FerroWasp board on `port` to reboot into its ROM bootloader, and
/// waits for the DFU device.
///
/// Refuses an armed board before asking; the firmware refuses it as well.
pub fn enter_rom_bootloader(port: &PortInfo, timeout: Duration) -> Result<DfuDetection> {
    let mut client = open_device(DeviceSelector::Port(port.port.clone()), timeout)?;
    let status = client.read_status_fresh()?;
    if status.armed {
        return Err(FerroError::DeviceRejected {
            operation: "reboot dfu".to_owned(),
            message: "the controller is armed".to_owned(),
        });
    }
    client.reboot(true)?;
    drop(client);
    wait_for_dfu_device(DFU_ENTRY_TIMEOUT)
}

/// Waits for exactly one STM32 ROM DFU device.
pub fn wait_for_dfu_device(timeout: Duration) -> Result<DfuDetection> {
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
            _ => thread::sleep(POLL),
        }
    }
}

/// Waits for `profile`'s board to come back as FerroWasp; `None` if it does
/// not within `timeout`.
pub fn wait_for_board_port(profile: &BoardProfile, timeout: Duration) -> Result<Option<PortInfo>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(port) = find_board_port(profile)? {
            return Ok(Some(port));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str, serial: &str) -> PortInfo {
        PortInfo {
            port: name.to_owned(),
            is_ferrowasp: true,
            vid: Some(0x16c0),
            pid: Some(0x27dd),
            manufacturer: None,
            product: None,
            serial_number: Some(serial.to_owned()),
        }
    }

    #[test]
    fn only_the_profiles_board_is_selected() {
        let foxeer = BoardProfile::FOXEER_F405_V2;
        assert_eq!(select_board_port(Vec::new(), &foxeer).unwrap(), None);
        assert_eq!(
            select_board_port(vec![port("COM6", "FW-FOX-F405V2")], &foxeer).unwrap(),
            Some(port("COM6", "FW-FOX-F405V2"))
        );
        let other = select_board_port(vec![port("COM7", "FW-TBS-LUCID-H7")], &foxeer);
        assert!(
            matches!(other, Err(FerroError::InvalidConfiguration(message)) if message.contains("TBS Lucid H7"))
        );
        let both = select_board_port(
            vec![
                port("COM6", "FW-FOX-F405V2"),
                port("COM7", "FW-TBS-LUCID-H7"),
            ],
            &foxeer,
        );
        assert!(matches!(both, Err(FerroError::MultipleDevicesFound { .. })));
    }
}

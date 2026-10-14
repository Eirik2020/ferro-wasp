use std::time::Duration;

use serde::Serialize;

use crate::{
    client::FerroClient,
    error::{FerroError, Result},
    transport::SerialTransport,
};

pub const FERROWASP_USB_VID: u16 = 0x16c0;
pub const FERROWASP_USB_PID: u16 = 0x27dd;

/// The boards FerroWasp firmware identifies by its USB serial number, as each
/// board's `src/board/usb.rs` sets it.
const KNOWN_BOARDS: &[(&str, &str)] = &[
    ("FW-FOX-F405V2", "Foxeer F405 V2"),
    ("FW-TBS-LUCID-H7", "TBS Lucid H7"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
    Auto,
    Port(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortInfo {
    pub port: String,
    pub is_ferrowasp: bool,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

impl PortInfo {
    /// The board this port's firmware says it runs on, from its USB serial
    /// number; `None` for a board this configurator does not know.
    pub fn board_name(&self) -> Option<&'static str> {
        let serial = self.serial_number.as_deref()?;
        KNOWN_BOARDS
            .iter()
            .find(|(known, _)| *known == serial)
            .map(|(_, name)| *name)
    }
}

pub fn discover_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()
        .map_err(|error| FerroError::Enumeration(error.to_string()))?;
    let mut output: Vec<_> = ports
        .into_iter()
        .map(|port| {
            let (vid, pid, manufacturer, product, serial_number) = match port.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    Some(info.vid),
                    Some(info.pid),
                    info.manufacturer,
                    info.product,
                    info.serial_number,
                ),
                _ => (None, None, None, None, None),
            };
            let text_identifies_ferrowasp = manufacturer
                .as_deref()
                .into_iter()
                .chain(product.as_deref())
                .any(|value| value.to_ascii_lowercase().contains("ferrowasp"));
            PortInfo {
                port: port.port_name,
                is_ferrowasp: (vid == Some(FERROWASP_USB_VID) && pid == Some(FERROWASP_USB_PID))
                    || text_identifies_ferrowasp,
                vid,
                pid,
                manufacturer,
                product,
                serial_number,
            }
        })
        .collect();
    output.sort_by(|left, right| left.port.cmp(&right.port));
    Ok(output)
}

pub fn open_device(
    selector: DeviceSelector,
    timeout: Duration,
) -> Result<FerroClient<SerialTransport>> {
    let port = match selector {
        DeviceSelector::Port(port) => port,
        DeviceSelector::Auto => {
            let matches: Vec<_> = discover_ports()?
                .into_iter()
                .filter(|port| port.is_ferrowasp)
                .map(|port| port.port)
                .collect();
            match matches.as_slice() {
                [] => return Err(FerroError::DeviceNotFound),
                [only] => only.clone(),
                _ => return Err(FerroError::MultipleDevicesFound { ports: matches }),
            }
        }
    };
    let transport = SerialTransport::open(&port, timeout)?;
    let mut client = FerroClient::new(transport, timeout);
    client.resynchronize()?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_usb_identity_is_stable() {
        assert_eq!((FERROWASP_USB_VID, FERROWASP_USB_PID), (0x16c0, 0x27dd));
    }

    fn port(serial_number: Option<&str>) -> PortInfo {
        PortInfo {
            port: "/dev/ttyACM0".to_owned(),
            is_ferrowasp: true,
            vid: Some(FERROWASP_USB_VID),
            pid: Some(FERROWASP_USB_PID),
            manufacturer: Some("FerroWasp".to_owned()),
            product: None,
            serial_number: serial_number.map(str::to_owned),
        }
    }

    #[test]
    fn a_port_names_its_board_from_the_usb_serial_number() {
        assert_eq!(
            port(Some("FW-FOX-F405V2")).board_name(),
            Some("Foxeer F405 V2")
        );
        assert_eq!(
            port(Some("FW-TBS-LUCID-H7")).board_name(),
            Some("TBS Lucid H7")
        );
        assert_eq!(port(Some("FW-SOMETHING-NEW")).board_name(), None);
        assert_eq!(port(None).board_name(), None);
    }
}

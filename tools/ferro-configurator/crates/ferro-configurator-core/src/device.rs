use std::time::Duration;

use serde::Serialize;

use crate::{
    client::FerroClient,
    error::{FerroError, Result},
    transport::SerialTransport,
};

pub const FERROWASP_USB_VID: u16 = 0x16c0;
pub const FERROWASP_USB_PID: u16 = 0x27dd;

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
    Ok(FerroClient::new(transport, timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_usb_identity_is_stable() {
        assert_eq!((FERROWASP_USB_VID, FERROWASP_USB_PID), (0x16c0, 0x27dd));
    }
}

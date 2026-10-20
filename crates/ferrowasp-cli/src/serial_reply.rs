//! The `serial` command's reply: which function each routed port serves.

use core::fmt::Write as _;

use ferrowasp_serial::{SerialBindings, SerialRoute};

/// The longest `serial` reply line, inside a USB response frame.
pub const SERIAL_REPLY_LINE_CAPACITY: usize = 64;

/// The `serial` reply, one line at a time so each fits a USB response frame
/// however many ports a board routes: a header saying whether the table is
/// saved or the board's defaults and how many port lines follow, then
/// `uartN=function` for each port the board routes.
pub fn write_serial_bindings(
    stored: Option<SerialBindings>,
    defaults: SerialBindings,
    routes: &[SerialRoute],
    mut line: impl FnMut(&str),
) {
    let source = if stored.is_some() { "saved" } else { "default" };
    let bindings = stored.unwrap_or(defaults);
    let mut text = heapless::String::<SERIAL_REPLY_LINE_CAPACITY>::new();
    // Neither line can exceed the capacity: the header is fixed apart from a
    // port count below ten, and a port line is at most 24 bytes.
    let _ = write!(
        text,
        "OK serial {source} ports={}; applies after save and reboot\r\n",
        routes.len()
    );
    line(&text);
    for route in routes {
        text.clear();
        let _ = write!(
            text,
            "OK {}={}\r\n",
            route.logical.name(),
            bindings.function(route.logical).name()
        );
        line(&text);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ferrowasp_serial::{LogicalSerialPort, SerialCapabilities, SerialFunction, SerialProfile};
    use std::{string::String, vec::Vec};

    const fn route(logical: LogicalSerialPort) -> SerialRoute {
        SerialRoute {
            logical,
            peripheral: "test",
            tx_pin: "test",
            rx_pin: "test",
            profile: SerialProfile::disabled(),
            rx_dma: "test",
            tx_dma: None,
            capabilities: SerialCapabilities {
                sbus: true,
                crsf: false,
                mavlink: false,
                msp: false,
                esc_telemetry: true,
                cli: false,
                tx: false,
            },
        }
    }

    fn reply(stored: Option<SerialBindings>, routes: &[SerialRoute]) -> Vec<String> {
        let defaults =
            SerialBindings::none().with(LogicalSerialPort::Uart2, SerialFunction::RcInput);
        let mut lines = Vec::new();
        write_serial_bindings(stored, defaults, routes, |line| {
            lines.push(String::from(line))
        });
        lines
    }

    #[test]
    fn the_reply_is_a_counted_header_then_one_line_per_routed_port() {
        let routes = [
            route(LogicalSerialPort::Uart1),
            route(LogicalSerialPort::Uart2),
        ];
        assert_eq!(
            reply(None, &routes),
            [
                "OK serial default ports=2; applies after save and reboot\r\n",
                "OK uart1=none\r\n",
                "OK uart2=rc\r\n",
            ]
        );
        let saved =
            SerialBindings::none().with(LogicalSerialPort::Uart1, SerialFunction::EscTelemetry);
        assert_eq!(
            reply(Some(saved), &routes)[..2],
            [
                "OK serial saved ports=2; applies after save and reboot\r\n",
                "OK uart1=esc_telemetry\r\n",
            ]
        );
    }

    #[test]
    fn every_line_fits_a_usb_response_frame() {
        let routes = LogicalSerialPort::ALL.map(route);
        let mut worst = SerialBindings::none();
        for port in LogicalSerialPort::ALL {
            worst.set(port, SerialFunction::EscTelemetry);
        }
        let lines = reply(Some(worst), &routes);
        assert_eq!(lines.len(), 1 + LogicalSerialPort::ALL.len());
        for line in lines {
            assert!(line.ends_with("\r\n"), "{line:?} lost its terminator");
            assert!(line.len() <= SERIAL_REPLY_LINE_CAPACITY);
        }
    }
}

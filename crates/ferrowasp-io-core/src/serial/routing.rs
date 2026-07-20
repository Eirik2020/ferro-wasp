#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum UartConsumer {
    RcInput,
    Osd,
    Tele,
    Gps,
}

pub const UART1_CONSUMER: UartConsumer = UartConsumer::Tele;
pub const UART2_CONSUMER: UartConsumer = UartConsumer::RcInput;
pub const UART3_CONSUMER: UartConsumer = UartConsumer::Gps;
pub const UART4_CONSUMER: UartConsumer = UartConsumer::Osd;

pub fn route_uart_to_task<T>(
    consumer: UartConsumer,
    parser: T,
    rc_input_uart: &mut Option<T>,
    osd_uart: &mut Option<T>,
    tele_uart: &mut Option<T>,
    gps_uart: &mut Option<T>,
) {
    match consumer {
        UartConsumer::RcInput => {
            assert!(rc_input_uart.is_none());
            *rc_input_uart = Some(parser);
        }
        UartConsumer::Osd => {
            assert!(osd_uart.is_none());
            *osd_uart = Some(parser);
        }
        UartConsumer::Tele => {
            assert!(tele_uart.is_none());
            *tele_uart = Some(parser);
        }
        UartConsumer::Gps => {
            assert!(gps_uart.is_none());
            *gps_uart = Some(parser);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uart_consumers_preserve_current_app_routing() {
        assert_eq!(UART1_CONSUMER, UartConsumer::Tele);
        assert_eq!(UART2_CONSUMER, UartConsumer::RcInput);
        assert_eq!(UART3_CONSUMER, UartConsumer::Gps);
        assert_eq!(UART4_CONSUMER, UartConsumer::Osd);
    }

    #[test]
    fn route_uart_to_task_places_parser_in_selected_slot() {
        let mut rc = None;
        let mut osd = None;
        let mut tele = None;
        let mut gps = None;

        route_uart_to_task(
            UartConsumer::Osd,
            42,
            &mut rc,
            &mut osd,
            &mut tele,
            &mut gps,
        );

        assert_eq!(rc, None);
        assert_eq!(osd, Some(42));
        assert_eq!(tele, None);
        assert_eq!(gps, None);
    }
}

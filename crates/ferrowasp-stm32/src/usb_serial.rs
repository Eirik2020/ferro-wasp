pub const USB_ENDPOINT_MEMORY_WORDS: usize = 1024;
pub const USB_CDC_RX_BUFFER_BYTES: usize = 64;
/// Holds a whole `FWDBG1` status line, receiver channels included, so a line
/// is written in one piece.
pub const USB_CDC_TX_BUFFER_BYTES: usize = 512;
pub const FERROWASP_USB_VID: u16 = 0x16c0;
pub const FERROWASP_USB_PID: u16 = 0x27dd;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbCdcIdentity {
    pub manufacturer: &'static str,
    pub product: &'static str,
    pub serial_number: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_usb_policy_is_bounded_and_stable() {
        assert_eq!(USB_ENDPOINT_MEMORY_WORDS, 1024);
        assert_eq!(USB_CDC_RX_BUFFER_BYTES, 64);
        assert_eq!(USB_CDC_TX_BUFFER_BYTES, 512);
        assert_eq!((FERROWASP_USB_VID, FERROWASP_USB_PID), (0x16c0, 0x27dd));
    }
}

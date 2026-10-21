pub use usb_device::UsbError;
use usb_device::bus::UsbBus;
use usb_device::device::{UsbDevice, UsbDeviceState};
use usbd_serial::SerialPort;

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

/// The USB CDC device and its serial class, which are polled together, over
/// any chip's USB bus.
pub struct UsbCdc<B: UsbBus + 'static> {
    device: UsbDevice<'static, B>,
    serial: SerialPort<'static, B, [u8; USB_CDC_RX_BUFFER_BYTES], [u8; USB_CDC_TX_BUFFER_BYTES]>,
}

impl<B: UsbBus> UsbCdc<B> {
    pub fn new(
        device: UsbDevice<'static, B>,
        serial: SerialPort<
            'static,
            B,
            [u8; USB_CDC_RX_BUFFER_BYTES],
            [u8; USB_CDC_TX_BUFFER_BYTES],
        >,
    ) -> Self {
        Self { device, serial }
    }
}

/// What the USB task needs of its port, whatever the chip's USB peripheral.
pub trait UsbCdcPort {
    /// Serve the bus once, for the serial class.
    fn poll(&mut self);
    fn read(&mut self, data: &mut [u8]) -> Result<usize, UsbError>;
    fn write(&mut self, data: &[u8]) -> Result<usize, UsbError>;
    fn flush(&mut self) -> Result<(), UsbError>;
    /// A host has the device configured.
    fn configured(&self) -> bool;
}

impl<B: UsbBus> UsbCdcPort for UsbCdc<B> {
    fn poll(&mut self) {
        let _ = self.device.poll(&mut [&mut self.serial]);
    }

    fn read(&mut self, data: &mut [u8]) -> Result<usize, UsbError> {
        self.serial.read(data)
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, UsbError> {
        self.serial.write(data)
    }

    fn flush(&mut self) -> Result<(), UsbError> {
        self.serial.flush()
    }

    fn configured(&self) -> bool {
        self.device.state() == UsbDeviceState::Configured
    }
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

pub use usb_device::UsbError;
use usb_device::bus::UsbBus;
pub use usb_device::device::{UsbDevice, UsbDeviceState};
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

/// The FerroHAL backend: USB CDC on FerroHAL's full-speed OTG port, on a
/// target with a chip feature.
#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub mod ferrohal {
    use ferrohal_stm32::usb::{UsbBus, UsbFs};
    use usb_device::{
        bus::UsbBusAllocator,
        device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
    };

    use super::{
        FERROWASP_USB_PID, FERROWASP_USB_VID, USB_CDC_RX_BUFFER_BYTES, USB_CDC_TX_BUFFER_BYTES,
        USB_ENDPOINT_MEMORY_WORDS, UsbCdc, UsbCdcIdentity,
    };

    /// The board's USB CDC port, as the USB task's [`UsbCdcPort`] bound sees it.
    pub type UsbCdcLink = UsbCdc<UsbBus>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum UsbCdcInitError {
        EndpointMemoryAlreadyTaken,
        BusAllocatorAlreadyTaken,
        InvalidStringDescriptors,
    }

    pub fn init_usb_cdc_serial(
        usb: UsbFs,
        identity: UsbCdcIdentity,
    ) -> Result<UsbCdcLink, UsbCdcInitError> {
        let endpoint_memory =
            cortex_m::singleton!(: [u32; USB_ENDPOINT_MEMORY_WORDS] = [0; USB_ENDPOINT_MEMORY_WORDS])
                .ok_or(UsbCdcInitError::EndpointMemoryAlreadyTaken)?;
        let usb_bus = cortex_m::singleton!(
            : Option<UsbBusAllocator<UsbBus>> = None
        )
        .ok_or(UsbCdcInitError::BusAllocatorAlreadyTaken)?
        .insert(UsbBus::new(usb, endpoint_memory));

        let serial = usbd_serial::SerialPort::new_with_store(
            usb_bus,
            [0; USB_CDC_RX_BUFFER_BYTES],
            [0; USB_CDC_TX_BUFFER_BYTES],
        );
        let device =
            UsbDeviceBuilder::new(usb_bus, UsbVidPid(FERROWASP_USB_VID, FERROWASP_USB_PID))
                .strings(&[StringDescriptors::default()
                    .manufacturer(identity.manufacturer)
                    .product(identity.product)
                    .serial_number(identity.serial_number)])
                .map_err(|_| UsbCdcInitError::InvalidStringDescriptors)?
                .device_class(usbd_serial::USB_CLASS_CDC)
                .build();
        Ok(UsbCdc::new(device, serial))
    }
}

#[cfg(all(target_arch = "arm", any(feature = "stm32f405", feature = "stm32h743")))]
pub use ferrohal::{UsbCdcInitError, UsbCdcLink, init_usb_cdc_serial};

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

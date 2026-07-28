pub const USB_ENDPOINT_MEMORY_WORDS: usize = 1024;
pub const USB_CDC_RX_BUFFER_BYTES: usize = 64;
pub const USB_CDC_TX_BUFFER_BYTES: usize = 256;
pub const FERROWASP_USB_VID: u16 = 0x16c0;
pub const FERROWASP_USB_PID: u16 = 0x27dd;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbCdcIdentity {
    pub manufacturer: &'static str,
    pub product: &'static str,
    pub serial_number: &'static str,
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub use usb_device::{UsbError, device::UsbDeviceState};

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub type UsbCdcDevice = usb_device::device::UsbDevice<'static, stm32f4xx_hal::otg_fs::UsbBusType>;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub type BufferedUsbCdcSerial = usbd_serial::SerialPort<
    'static,
    stm32f4xx_hal::otg_fs::UsbBusType,
    [u8; USB_CDC_RX_BUFFER_BYTES],
    [u8; USB_CDC_TX_BUFFER_BYTES],
>;

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbCdcInitError {
    EndpointMemoryAlreadyTaken,
    BusAllocatorAlreadyTaken,
    InvalidStringDescriptors,
}

#[cfg(all(target_arch = "arm", feature = "stm32f405"))]
pub fn init_usb_cdc_serial(
    peripherals: (
        stm32f4xx_hal::pac::OTG_FS_GLOBAL,
        stm32f4xx_hal::pac::OTG_FS_DEVICE,
        stm32f4xx_hal::pac::OTG_FS_PWRCLK,
    ),
    pins: (
        impl Into<stm32f4xx_hal::gpio::alt::otg_fs::Dm>,
        impl Into<stm32f4xx_hal::gpio::alt::otg_fs::Dp>,
    ),
    clocks: &stm32f4xx_hal::rcc::Clocks,
    identity: UsbCdcIdentity,
) -> Result<(UsbCdcDevice, BufferedUsbCdcSerial), UsbCdcInitError> {
    use stm32f4xx_hal::otg_fs::{USB, UsbBusType};
    use usb_device::{
        bus::UsbBusAllocator,
        device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
    };

    let endpoint_memory =
        cortex_m::singleton!(: [u32; USB_ENDPOINT_MEMORY_WORDS] = [0; USB_ENDPOINT_MEMORY_WORDS])
            .ok_or(UsbCdcInitError::EndpointMemoryAlreadyTaken)?;
    // The allocator macro evaluates its initializer at most once. Staging the
    // static slice in `Option` makes the closure move it instead of shortening
    // its lifetime through an implicit reborrow.
    let mut endpoint_memory = Some(endpoint_memory as &'static mut [u32]);
    let usb = USB::new(peripherals, pins, clocks);
    let usb_bus = cortex_m::singleton!(
        : UsbBusAllocator<UsbBusType> = UsbBusType::new(
            usb,
            endpoint_memory.take().unwrap()
        )
    )
    .ok_or(UsbCdcInitError::BusAllocatorAlreadyTaken)?;

    let serial = usbd_serial::SerialPort::new_with_store(
        usb_bus,
        [0; USB_CDC_RX_BUFFER_BYTES],
        [0; USB_CDC_TX_BUFFER_BYTES],
    );
    let device = UsbDeviceBuilder::new(usb_bus, UsbVidPid(FERROWASP_USB_VID, FERROWASP_USB_PID))
        .strings(&[StringDescriptors::default()
            .manufacturer(identity.manufacturer)
            .product(identity.product)
            .serial_number(identity.serial_number)])
        .map_err(|_| UsbCdcInitError::InvalidStringDescriptors)?
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

    Ok((device, serial))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_usb_policy_is_bounded_and_stable() {
        assert_eq!(USB_ENDPOINT_MEMORY_WORDS, 1024);
        assert_eq!(USB_CDC_RX_BUFFER_BYTES, 64);
        assert_eq!(USB_CDC_TX_BUFFER_BYTES, 256);
        assert_eq!((FERROWASP_USB_VID, FERROWASP_USB_PID), (0x16c0, 0x27dd));
    }
}

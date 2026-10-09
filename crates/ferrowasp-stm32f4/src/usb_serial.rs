//! USB CDC on the STM32F4's OTG FS peripheral.

pub use ferrowasp_stm32::usb_serial::*;

pub use usb_device::{UsbError, device::UsbDeviceState};

pub type UsbCdcDevice = usb_device::device::UsbDevice<'static, stm32f4xx_hal::otg_fs::UsbBusType>;

pub type BufferedUsbCdcSerial = usbd_serial::SerialPort<
    'static,
    stm32f4xx_hal::otg_fs::UsbBusType,
    [u8; USB_CDC_RX_BUFFER_BYTES],
    [u8; USB_CDC_TX_BUFFER_BYTES],
>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbCdcInitError {
    EndpointMemoryAlreadyTaken,
    BusAllocatorAlreadyTaken,
    InvalidStringDescriptors,
}

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

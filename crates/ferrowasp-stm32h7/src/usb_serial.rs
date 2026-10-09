//! USB CDC debug serial on the STM32H743's full-speed OTG port (OTG2 on
//! PA11/PA12), with the same identity, buffers, and VID/PID policy as the
//! F4 boards.

pub use ferrowasp_stm32::usb_serial::{
    FERROWASP_USB_PID, FERROWASP_USB_VID, USB_CDC_RX_BUFFER_BYTES, USB_CDC_TX_BUFFER_BYTES,
    USB_ENDPOINT_MEMORY_WORDS, UsbCdcIdentity,
};
use stm32h7xx_hal::{
    gpio::{PA11, PA12},
    pac::{OTG2_HS_DEVICE, OTG2_HS_GLOBAL, OTG2_HS_PWRCLK},
    rcc::{CoreClocks, rec},
    usb_hs::{USB2, UsbBus},
};
pub use usb_device::{UsbError, device::UsbDeviceState};
use usb_device::{
    bus::UsbBusAllocator,
    device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};

pub type UsbCdcDevice = usb_device::device::UsbDevice<'static, UsbBus<USB2>>;
pub type BufferedUsbCdcSerial = usbd_serial::SerialPort<
    'static,
    UsbBus<USB2>,
    [u8; USB_CDC_RX_BUFFER_BYTES],
    [u8; USB_CDC_TX_BUFFER_BYTES],
>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbCdcInitError {
    EndpointMemoryAlreadyTaken,
    BusAllocatorAlreadyTaken,
    InvalidStringDescriptors,
}

pub struct UsbResources {
    pub global: OTG2_HS_GLOBAL,
    pub device: OTG2_HS_DEVICE,
    pub pwrclk: OTG2_HS_PWRCLK,
    pub dm: PA11,
    pub dp: PA12,
    pub prec: rec::Usb2Otg,
}

/// The USB kernel clock must already be HSI48; `clocks::freeze_hse` selects
/// it.
pub fn init_usb_cdc_serial(
    resources: UsbResources,
    clocks: &CoreClocks,
    identity: UsbCdcIdentity,
) -> Result<(UsbCdcDevice, BufferedUsbCdcSerial), UsbCdcInitError> {
    let endpoint_memory =
        cortex_m::singleton!(: [u32; USB_ENDPOINT_MEMORY_WORDS] = [0; USB_ENDPOINT_MEMORY_WORDS])
            .ok_or(UsbCdcInitError::EndpointMemoryAlreadyTaken)?;
    let mut endpoint_memory = Some(endpoint_memory as &'static mut [u32]);
    let usb = USB2::new(
        resources.global,
        resources.device,
        resources.pwrclk,
        resources.dm.into_alternate(),
        resources.dp.into_alternate(),
        resources.prec,
        clocks,
    );
    let usb_bus = cortex_m::singleton!(
        : UsbBusAllocator<UsbBus<USB2>> = UsbBus::new(
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

//! USB CDC on FerroHAL's full-speed OTG port.

use ferrohal_stm32::usb::{UsbBus, UsbFs};
pub use ferrowasp_stm32::usb_serial::*;
pub use usb_device::{UsbError, device::UsbDeviceState};
use usb_device::{
    bus::UsbBusAllocator,
    device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
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
    let device = UsbDeviceBuilder::new(usb_bus, UsbVidPid(FERROWASP_USB_VID, FERROWASP_USB_PID))
        .strings(&[StringDescriptors::default()
            .manufacturer(identity.manufacturer)
            .product(identity.product)
            .serial_number(identity.serial_number)])
        .map_err(|_| UsbCdcInitError::InvalidStringDescriptors)?
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();
    Ok(UsbCdc::new(device, serial))
}

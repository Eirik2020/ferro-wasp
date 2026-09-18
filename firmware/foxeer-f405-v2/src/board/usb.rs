use ferrowasp_stm32f4::usb_serial::UsbCdcIdentity;

pub const USB_CDC_IDENTITY: UsbCdcIdentity = UsbCdcIdentity {
    manufacturer: "FerroWasp",
    product: "FerroWasp Foxeer Debug",
    serial_number: "FW-FOX-F405V2",
};

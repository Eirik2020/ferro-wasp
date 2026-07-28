use ferrowasp_stm32f4::usb_serial::UsbCdcIdentity;

pub const USB_CDC_IDENTITY: UsbCdcIdentity = UsbCdcIdentity {
    manufacturer: "FerroWasp",
    product: "FerroWasp USB Serial",
    serial_number: "FW-0001",
};

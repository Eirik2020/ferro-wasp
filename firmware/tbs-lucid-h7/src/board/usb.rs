use ferrowasp_stm32f4::usb_serial::UsbCdcIdentity;

pub const USB_CDC_IDENTITY: UsbCdcIdentity = UsbCdcIdentity {
    manufacturer: "FerroWasp",
    product: "FerroWasp TBS Lucid H7 Debug",
    serial_number: "FW-TBS-LUCID-H7",
};

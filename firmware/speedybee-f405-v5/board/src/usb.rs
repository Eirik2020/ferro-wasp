use ferrowasp_stm32::usb_serial::UsbCdcIdentity;

pub const USB_CDC_IDENTITY: UsbCdcIdentity = UsbCdcIdentity {
    manufacturer: "FerroWasp",
    product: "FerroWasp SpeedyBee Debug",
    serial_number: "FW-SBEE-F405V5",
};

//! Host-checkable stand-ins for target-only golden service resources.
//!
//! Generated firmware imports the concrete shared STM32F4 backends instead.

#![allow(dead_code, missing_docs)]

use ferrowasp_drivers::spi_nor::{JedecId, Status};

pub struct Adc1ObservationTransfer;
pub struct Adc;

impl Adc1ObservationTransfer {
    pub fn start(&mut self, start: impl FnOnce(&mut Adc)) {
        start(&mut Adc);
    }
}

impl Adc {
    pub fn start_conversion(&mut self) {}
}

pub struct Adc1Sample {
    pub buffer: &'static mut [u16; 3],
    pub voltage_mv: u16,
    pub current_mv: u16,
}

#[derive(Clone, Copy)]
pub enum AdcDmaDeliveryError {
    DmaFault,
    NoSpareBuffer,
    TransferNotReady,
}

pub fn take_completed_adc1_sample_for(
    _transfer: &mut Adc1ObservationTransfer,
    _spare: &mut Option<&'static mut [u16; 3]>,
    _planner: &mut ferrowasp_stm32f4::adc::AdcDmaIrqPlanner,
) -> Result<Option<Adc1Sample>, AdcDmaDeliveryError> {
    Ok(None)
}

pub struct Spi2Flash;

impl Spi2Flash {
    pub fn read_jedec_id(&mut self) -> Result<JedecId, ()> {
        Err(())
    }

    pub fn read_status(&mut self) -> Result<Status, ()> {
        Ok(Status(0))
    }

    pub fn read(&mut self, _address: u32, _output: &mut [u8]) -> Result<(), ()> {
        Ok(())
    }

    pub fn page_program(&mut self, _address: u32, _bytes: &[u8]) -> Result<(), ()> {
        Ok(())
    }

    pub fn erase_sector_4k(&mut self, _address: u32) -> Result<(), ()> {
        Ok(())
    }
}

pub struct UsbCdcDevice;
pub struct BufferedUsbCdcSerial;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum UsbDeviceState {
    Default,
    Configured,
}

impl UsbCdcDevice {
    pub fn poll(&mut self, _classes: &mut [&mut BufferedUsbCdcSerial]) -> bool {
        false
    }

    pub fn state(&self) -> UsbDeviceState {
        UsbDeviceState::Default
    }
}

impl BufferedUsbCdcSerial {
    pub fn read(&mut self, _bytes: &mut [u8]) -> Result<usize, ()> {
        Ok(0)
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<usize, ()> {
        Ok(bytes.len())
    }

    pub fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

pub struct IoWatchdog;

pub fn acknowledge_watchdog_tick(_watchdog: &mut IoWatchdog) {}

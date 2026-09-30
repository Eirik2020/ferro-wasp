//! The STM32H7 backend's only unsafe boundary.
//!
//! # Unsafe boundary
//!
//! The HAL's DMA `Transfer` owns the stream, the buffer, and their fencing.
//! What it cannot know is which register a hand-built endpoint names, so this
//! module proves three facts to it:
//!
//! - a UART endpoint's address is the owned UART's own RDR or TDR, accessed a
//!   byte at a time, and its request line is that UART's DMAMUX input;
//! - an SPI endpoint's address is the owned SPI's own RXDR or TXDR, accessed
//!   a byte at a time, and its request line is that SPI's DMAMUX input;
//! - a timer compare endpoint's address is the owned timer's own CCRx,
//!   accessed as a 32-bit word (TIM2 and TIM5 have 32-bit compare registers,
//!   and the H7 bus replicates a halfword write into both halves), and its
//!   request line is that channel's DMAMUX input;
//! - reading a stream's error flags is a side-effect-free register read of
//!   the bits that belong to that stream.
//!
//! Endpoints are constructed only from a reference to the peripheral they
//! name, by the modules that take exclusive ownership of it.

#![deny(unsafe_op_in_unsafe_fn)]

use core::marker::PhantomData;

use stm32h7xx_hal::dma::{
    MemoryToPeripheral, PeripheralToMemory,
    dma::{DMAReq, Instance, StreamX},
    traits::TargetAddress,
};
use stm32h7xx_hal::pac;

/// SPI1's receive data register, as a DMA source.
pub struct Spi1RxEndpoint {
    address: usize,
}

/// SPI1's transmit data register, as a DMA destination.
pub struct Spi1TxEndpoint {
    address: usize,
}

/// Both SPI1 data-register endpoints, taken from the SPI the caller owns.
pub(crate) fn spi1_endpoints(spi: &pac::SPI1) -> (Spi1RxEndpoint, Spi1TxEndpoint) {
    (
        Spi1RxEndpoint {
            address: spi.rxdr.as_ptr() as usize,
        },
        Spi1TxEndpoint {
            address: spi.txdr.as_ptr() as usize,
        },
    )
}

// SAFETY: the address is SPI1's RXDR, taken from the owned peripheral, and
// 8-bit frames are read from it a byte at a time. The request line is SPI1's
// receive DMAMUX input (RM0433 table 121).
unsafe impl TargetAddress<PeripheralToMemory> for Spi1RxEndpoint {
    type MemSize = u8;
    const REQUEST_LINE: Option<u8> = Some(DMAReq::Spi1RxDma as u8);

    fn address(&self) -> usize {
        self.address
    }
}

// SAFETY: the address is SPI1's TXDR, taken from the owned peripheral, and
// 8-bit frames are written to it a byte at a time. The request line is
// SPI1's transmit DMAMUX input.
unsafe impl TargetAddress<MemoryToPeripheral> for Spi1TxEndpoint {
    type MemSize = u8;
    const REQUEST_LINE: Option<u8> = Some(DMAReq::Spi1TxDma as u8);

    fn address(&self) -> usize {
        self.address
    }
}

/// A UART this backend moves by DMA, with its DMAMUX request lines.
pub trait UartDmaPeripheral:
    sealed::Sealed + core::ops::Deref<Target = pac::usart1::RegisterBlock>
{
    const RX_REQUEST: u8;
    const TX_REQUEST: u8;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! uart_dma_peripheral {
    ($($uart:ident: $rx:ident, $tx:ident;)+) => {
        $(
            impl sealed::Sealed for pac::$uart {}
            impl UartDmaPeripheral for pac::$uart {
                const RX_REQUEST: u8 = DMAReq::$rx as u8;
                const TX_REQUEST: u8 = DMAReq::$tx as u8;
            }
        )+
    };
}

uart_dma_peripheral! {
    USART3: Usart3RxDma, Usart3TxDma;
    USART6: Usart6RxDma, Usart6TxDma;
    UART8: Uart8RxDma, Uart8TxDma;
}

/// A UART's receive data register, as a DMA source.
pub struct UartRxEndpoint<U> {
    address: usize,
    _uart: PhantomData<U>,
}

/// A UART's transmit data register, as a DMA destination.
pub struct UartTxEndpoint<U> {
    address: usize,
    _uart: PhantomData<U>,
}

/// Both data-register endpoints of a UART the caller owns.
pub(crate) fn uart_endpoints<U: UartDmaPeripheral>(
    uart: &U,
) -> (UartRxEndpoint<U>, UartTxEndpoint<U>) {
    (
        UartRxEndpoint {
            address: uart.rdr.as_ptr() as usize,
            _uart: PhantomData,
        },
        UartTxEndpoint {
            address: uart.tdr.as_ptr() as usize,
            _uart: PhantomData,
        },
    )
}

// SAFETY: the address is the owned UART's RDR, read a byte at a time, and the
// request line is that UART's receive DMAMUX input.
unsafe impl<U: UartDmaPeripheral> TargetAddress<PeripheralToMemory> for UartRxEndpoint<U> {
    type MemSize = u8;
    const REQUEST_LINE: Option<u8> = Some(U::RX_REQUEST);

    fn address(&self) -> usize {
        self.address
    }
}

// SAFETY: the address is the owned UART's TDR, written a byte at a time, and
// the request line is that UART's transmit DMAMUX input.
unsafe impl<U: UartDmaPeripheral> TargetAddress<MemoryToPeripheral> for UartTxEndpoint<U> {
    type MemSize = u8;
    const REQUEST_LINE: Option<u8> = Some(U::TX_REQUEST);

    fn address(&self) -> usize {
        self.address
    }
}

/// One timer channel's compare register, as a DMA destination. `REQUEST` is
/// that channel's DMAMUX input.
pub struct TimerCompareEndpoint<const REQUEST: u8> {
    address: usize,
}

pub(crate) fn tim3_ch3(timer: &pac::TIM3) -> TimerCompareEndpoint<{ DMAReq::Tim3Ch3 as u8 }> {
    TimerCompareEndpoint {
        address: timer.ccr[2].as_ptr() as usize,
    }
}

pub(crate) fn tim3_ch4(timer: &pac::TIM3) -> TimerCompareEndpoint<{ DMAReq::Tim3Ch4 as u8 }> {
    TimerCompareEndpoint {
        address: timer.ccr[3].as_ptr() as usize,
    }
}

pub(crate) fn tim5_ch1(timer: &pac::TIM5) -> TimerCompareEndpoint<{ DMAReq::Tim5Ch1 as u8 }> {
    TimerCompareEndpoint {
        address: timer.ccr[0].as_ptr() as usize,
    }
}

pub(crate) fn tim5_ch2(timer: &pac::TIM5) -> TimerCompareEndpoint<{ DMAReq::Tim5Ch2 as u8 }> {
    TimerCompareEndpoint {
        address: timer.ccr[1].as_ptr() as usize,
    }
}

// SAFETY: every constructor above takes the address of a CCRx register of a
// timer its caller owns, and passes the DMAMUX input of that same channel.
// Compare registers are written as whole 32-bit words.
unsafe impl<const REQUEST: u8> TargetAddress<MemoryToPeripheral> for TimerCompareEndpoint<REQUEST> {
    type MemSize = u32;
    const REQUEST_LINE: Option<u8> = Some(REQUEST);

    fn address(&self) -> usize {
        self.address
    }
}

/// A stream's error flags. The HAL reads transfer complete but not these.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamErrorFlags {
    pub transfer_error: bool,
    pub direct_mode_error: bool,
    pub fifo_error: bool,
}

impl StreamErrorFlags {
    pub const fn any(self) -> bool {
        self.transfer_error || self.direct_mode_error || self.fifo_error
    }
}

/// A DMA stream that can report its own error flags.
pub trait StreamErrors {
    fn error_flags() -> StreamErrorFlags;
}

impl<I, const S: u8> StreamErrors for StreamX<I, S>
where
    I: Instance,
{
    fn error_flags() -> StreamErrorFlags {
        // SAFETY: a volatile read of LISR or HISR, which has no side effects;
        // only this stream's bits are used.
        let dma = unsafe { &*I::ptr() };
        let status = if S < 4 {
            dma.lisr.read().bits()
        } else {
            dma.hisr.read().bits()
        };
        // Each stream's six flags sit at bit 0, 6, 16, or 22 of its register:
        // FEIF, reserved, DMEIF, TEIF, HTIF, TCIF.
        let flags = status >> [0, 6, 16, 22][usize::from(S % 4)];
        StreamErrorFlags {
            fifo_error: flags & 0b0001 != 0,
            direct_mode_error: flags & 0b0100 != 0,
            transfer_error: flags & 0b1000 != 0,
        }
    }
}

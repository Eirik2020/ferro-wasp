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

use ferrowasp_stm32_critical::dshot_bank::channel::*;
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
    USART1: Usart1RxDma, Usart1TxDma;
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

/// A timer channel DShot can drive: its compare register, and the DMAMUX
/// input of that channel (RM0433 table 121). Declared below for each channel
/// that has a request; a channel without one (TIM4 CH4) has no impl, so a
/// board naming it does not compile.
pub trait CompareChannel<C: LaneChannel> {
    const REQUEST: u8;
    fn compare_address(&self) -> usize;
}

macro_rules! compare_channels {
    ($($TIM:ident: [$($C:ident => $index:literal, $request:ident;)+])+) => {
        $($(
            impl CompareChannel<$C> for pac::$TIM {
                const REQUEST: u8 = DMAReq::$request as u8;
                fn compare_address(&self) -> usize {
                    self.ccr[$index].as_ptr() as usize
                }
            }
        )+)+
    };
}

compare_channels! {
    TIM2: [Ch1 => 0, Tim2Ch1; Ch2 => 1, Tim2Ch2; Ch3 => 2, Tim2Ch3; Ch4 => 3, Tim2Ch4;]
    TIM3: [Ch1 => 0, Tim3Ch1; Ch2 => 1, Tim3Ch2; Ch3 => 2, Tim3Ch3; Ch4 => 3, Tim3Ch4;]
    TIM4: [Ch1 => 0, Tim4Ch1; Ch2 => 1, Tim4Ch2; Ch3 => 2, Tim4Ch3;]
    TIM5: [Ch1 => 0, Tim5Ch1; Ch2 => 1, Tim5Ch2; Ch3 => 2, Tim5Ch3; Ch4 => 3, Tim5Ch4;]
}

/// One timer channel's compare register, as a DMA destination.
pub struct TimerCompareEndpoint<T, C> {
    address: usize,
    _channel: PhantomData<(T, C)>,
}

impl<T, C> TimerCompareEndpoint<T, C>
where
    T: CompareChannel<C>,
    C: LaneChannel,
{
    /// The endpoint for channel `C` of `timer`, which the caller owns.
    pub(crate) fn new(timer: &T) -> Self {
        Self {
            address: timer.compare_address(),
            _channel: PhantomData,
        }
    }
}

// SAFETY: `new` takes the address of channel `C`'s CCRx register from a
// timer its caller owns, and `REQUEST_LINE` is that same channel's DMAMUX
// input, both from the one `CompareChannel` impl for that timer and channel.
// Compare registers are written as whole 32-bit words.
unsafe impl<T, C> TargetAddress<MemoryToPeripheral> for TimerCompareEndpoint<T, C>
where
    T: CompareChannel<C>,
    C: LaneChannel,
{
    type MemSize = u32;
    const REQUEST_LINE: Option<u8> = Some(T::REQUEST);

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

use core::{convert::Infallible, marker::PhantomData};

use embedded_hal::digital::{ErrorType, OutputPin, StatefulOutputPin};

pub(crate) struct DigitalOutput {
    high: bool,
}

impl ErrorType for DigitalOutput {
    type Error = Infallible;
}

impl OutputPin for DigitalOutput {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.high = false;
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.high = true;
        Ok(())
    }
}

impl StatefulOutputPin for DigitalOutput {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.high)
    }

    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.high)
    }
}

pub(crate) struct InterruptInput;

impl InterruptInput {
    pub(crate) fn clear_interrupt_pending_bit(&mut self) {}
}

pub(crate) const UART_RX_BUFFER_SIZE: usize = 70;

pub(crate) struct UartRxDma;

impl UartRxDma {
    pub(crate) fn service_dma_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqOutcome::Ignored
    }

    pub(crate) fn service_idle_irq(&mut self) -> UartRxIrqOutcome {
        UartRxIrqOutcome::Ignored
    }

    pub(crate) fn read_chunk(
        &mut self,
        _output: &mut [u8; UART_RX_BUFFER_SIZE],
    ) -> UartRxReadOutcome {
        UartRxReadOutcome::NoChunk
    }
}

#[allow(dead_code)]
pub(crate) enum UartRxDeliveryError {
    Unavailable,
}

#[allow(dead_code)]
pub(crate) enum UartRxIrqOutcome {
    Ignored,
    Delivered,
    NoChunk,
    DmaError,
    DeliveryError(UartRxDeliveryError),
}

#[allow(dead_code)]
pub(crate) enum UartRxReadOutcome {
    NoChunk,
    Chunk(usize),
    RecycleError,
}

pub(crate) struct Shared<'a, T> {
    marker: PhantomData<&'a mut T>,
}

impl<T> Shared<'_, T> {
    pub(crate) fn lock<R>(&mut self, operation: impl FnOnce(&mut T) -> R) -> R {
        let _ = operation;
        panic!("compile-only shared resource must never execute")
    }
}

pub(crate) type Milliseconds = fugit::MillisDurationU32;

pub(crate) trait DurationExt {
    fn millis(self) -> Milliseconds;
}

impl DurationExt for u32 {
    fn millis(self) -> Milliseconds {
        Milliseconds::millis(self)
    }
}

pub(crate) struct Monotonic;

impl Monotonic {
    pub(crate) async fn delay(duration: Milliseconds) {
        let _ = duration;
    }
}

#[derive(Debug)]
pub(crate) struct SpawnError;

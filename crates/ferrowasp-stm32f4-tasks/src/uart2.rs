//! USART2, the RC receiver link: DMA and idle-line receive into the owned
//! stream the RC task reads. A transport fault invalidates the RC link
//! through the safety master, so a broken receiver fails closed.

use crate::prelude::*;

/// The bridge between USART2's DMA buffers and the owned RC stream.
pub type Uart2OwnedRxBridge = stm32_uart::UartOwnedRxBridge<
    'static,
    { stm32_memory::UART_RX_BUFFER_BYTES },
    { stm32_memory::OWNED_UART_RX_QUEUE_DEPTH },
>;

pub fn record_uart2_dma_error<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    record_uart2_discontinuity(
        bridge,
        Discontinuity::DmaError,
        generation,
        timestamp,
        safety::RcLinkInvalidation::DmaError,
        invalidate,
    );
}

pub fn record_uart2_discontinuity<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    cause: Discontinuity,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    reason: safety::RcLinkInvalidation,
    mut invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    bridge.record_discontinuity(cause, generation, timestamp);
    let _ = invalidate(reason);
}

pub fn publish_uart2_owned<Invalidate>(
    bridge: &mut Uart2OwnedRxBridge,
    generation: ferrowasp_io_core::serial::StreamGeneration,
    timestamp: TimestampMicros,
    mut invalidate: Invalidate,
) where
    Invalidate: FnMut(safety::RcLinkInvalidation) -> bool,
{
    match bridge.publish_next(timestamp) {
        stm32_uart::UartOwnedRxBridgeOutcome::Published => {}
        stm32_uart::UartOwnedRxBridgeOutcome::NoChunk => {
            warn!("USART2 delivered IRQ had no detached RX chunk");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::TransportReset,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::InvalidChunk => {
            warn!("USART2 produced an invalid owned RX chunk");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::FramingError,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::QueueOverflow => {
            warn!("USART2 owned RX queue overflowed");
            let _ = invalidate(safety::RcLinkInvalidation::TransportDiscontinuity);
        }
        stm32_uart::UartOwnedRxBridgeOutcome::Disabled => {
            warn!("USART2 owned RX channel is disabled");
            record_uart2_discontinuity(
                bridge,
                Discontinuity::TransportReset,
                generation,
                timestamp,
                safety::RcLinkInvalidation::TransportDiscontinuity,
                &mut invalidate,
            );
        }
        stm32_uart::UartOwnedRxBridgeOutcome::RecycleFailed => {
            panic!("USART2 detached DMA buffer could not be recycled");
        }
    }
}

/// USART2 RX DMA transfer complete. `uart2_rx` is lock-free, shared with the
/// idle-line handler at one priority.
#[ferroforge::task(
    shared = [
        #[lock_free] uart2_rx: stm32_uart::Uart2RxIrq,
        uart2_bridge: Uart2OwnedRxBridge,
    ],
    spawn = [safety_master(event: safety::SafetyEvent)],
    monotonic = Mono,
)]
pub fn usart2_rx_dma_transfer(mut cx: usart2_rx_dma_transfer::Context) {
    let uart = cx.shared.uart2_rx;
    let timestamp = || TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
    let invalidate = |reason| {
        cx.spawn
            .safety_master(safety::SafetyEvent::RcLinkInvalid(reason))
            .is_ok()
    };
    let delivered = match uart.service_dma_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered => true,
        stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
        stm32_uart::UartRxIrqOutcome::DmaError => {
            warn!("USART2 RX DMA error");
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_dma_error(bridge, uart.rx_generation(), timestamp(), invalidate)
            });
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::NoFreshBuffer,
        ) => {
            panic!("USART2 RX free-buffer pool exhausted");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::TransferNotReady,
        ) => {
            info!("USART2 DMA next_transfer failed");
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    uart.rx_generation(),
                    timestamp(),
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                    invalidate,
                )
            });
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::FilledQueueFull,
        ) => {
            panic!("USART2 filled queue full; RX buffer ownership would be lost");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::PlannerRejected,
        ) => {
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    uart.rx_generation(),
                    timestamp(),
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                    invalidate,
                )
            });
            false
        }
    };

    if delivered {
        let generation = uart.rx_generation();
        cx.shared
            .uart2_bridge
            .lock(|bridge| publish_uart2_owned(bridge, generation, timestamp(), invalidate));
    }
}

/// USART2 idle line: deliver the partly filled buffer.
#[ferroforge::task(
    shared = [
        #[lock_free] uart2_rx: stm32_uart::Uart2RxIrq,
        uart2_bridge: Uart2OwnedRxBridge,
    ],
    spawn = [safety_master(event: safety::SafetyEvent)],
    monotonic = Mono,
)]
pub fn usart2_rx_peripheral(mut cx: usart2_rx_peripheral::Context) {
    let uart = cx.shared.uart2_rx;
    let timestamp = || TimestampMicros(Mono::now().duration_since_epoch().to_micros() as u64);
    let invalidate = |reason| {
        cx.spawn
            .safety_master(safety::SafetyEvent::RcLinkInvalid(reason))
            .is_ok()
    };

    let delivered = match uart.service_idle_irq() {
        stm32_uart::UartRxIrqOutcome::Delivered => true,
        stm32_uart::UartRxIrqOutcome::Ignored | stm32_uart::UartRxIrqOutcome::NoChunk => false,
        stm32_uart::UartRxIrqOutcome::DmaError => {
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_dma_error(bridge, uart.rx_generation(), timestamp(), invalidate)
            });
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::NoFreshBuffer,
        ) => {
            warn!("USART2 RX free-buffer pool exhausted on IDLE");
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    uart.rx_generation(),
                    timestamp(),
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                    invalidate,
                )
            });
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::TransferNotReady,
        ) => {
            info!("USART2 IDLE next_transfer failed");
            cx.shared.uart2_bridge.lock(|bridge| {
                record_uart2_discontinuity(
                    bridge,
                    Discontinuity::TransportReset,
                    uart.rx_generation(),
                    timestamp(),
                    safety::RcLinkInvalidation::TransportDiscontinuity,
                    invalidate,
                )
            });
            false
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::FilledQueueFull,
        ) => {
            panic!("USART2 filled queue full; RX buffer ownership would be lost");
        }
        stm32_uart::UartRxIrqOutcome::DeliveryError(
            stm32_uart::UartRxDeliveryError::PlannerRejected,
        ) => {
            panic!("USART2 RX IDLE planner did not deliver a non-empty buffer");
        }
    };

    if delivered {
        let generation = uart.rx_generation();
        cx.shared
            .uart2_bridge
            .lock(|bridge| publish_uart2_owned(bridge, generation, timestamp(), invalidate));
    }
}

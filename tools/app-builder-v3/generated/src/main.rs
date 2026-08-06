// GENERATED FILE — DO NOT EDIT DIRECTLY
// Generated from xtask/src/target/board.rs and app_composition.rs.

#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
// Endpoint exports may intentionally have no consumer in a partial composition.
#![allow(dead_code)]

use defmt_rtt as _;
use panic_halt as _;

#[rtic::app(
    device = ferrowasp_stm32f4::rtic::hal::pac,
    peripherals = true,
    dispatchers = [EXTI0, EXTI1]
)]
mod app {
    use ferrowasp_io_core::serial::SerialFault;
    use ferrowasp_stm32f4::{
        app_storage::{
            Uart4TxBuffer, UartRxBufferBank, UartRxFilledQueue, UartRxFreeQueue,
            UartRxStorageResources,
        },
        memory::{
            UartOwnedDiscontinuities, UartOwnedReader, UartOwnedRxChannel, UartOwnedRxProducer,
        },
        memory::{UartOwnedTxChannel, UartOwnedTxCompletion, UartOwnedTxOwner, UartOwnedWriter},
        rtic::prelude::*,
        serial::{UartRxIrqOutcome, UartRxIrqService},
        serial::{UartTxDmaService, UartTxIrqOutcome, UartTxStartError},
        uart_dma::{
            UART4_TX_BUFFER_SIZE, Uart4MspResources, Uart4RxIrq, Uart4TxDmaSide, UartRxParserSide,
        },
    };

    const SYSTEM_CLOCK_HZ: u32 = 168_000_000;

    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {
        button_enabled: bool,
        osd_uart_rx: Uart4RxIrq,
        osd_uart_tx_dma: Uart4TxDmaSide,
    }

    #[local]
    struct Local {
        user_button: Pin<'C', 13, Input>,
        led2: Pin<'A', 5, Output<PushPull>>,
        osd_uart_rx_parser: UartRxParserSide,
        osd_uart_rx_producer: UartOwnedRxProducer<'static>,
        osd_uart_rx_reader: UartOwnedReader<'static>,
        osd_uart_rx_discontinuities: UartOwnedDiscontinuities<'static>,
        osd_uart_tx_writer: UartOwnedWriter<'static>,
        osd_uart_tx_owner: UartOwnedTxOwner<'static>,
        osd_uart_tx_completion: UartOwnedTxCompletion<'static>,
    }

    #[init(local = [
        osd_uart_rx_buffers: UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),
        osd_uart_rx_free_queue: UartRxFreeQueue = UartRxFreeQueue::new(),
        osd_uart_rx_filled_queue: UartRxFilledQueue = UartRxFilledQueue::new(),
        osd_uart_rx_channel: UartOwnedRxChannel = UartOwnedRxChannel::new(),
        osd_uart_tx_buffer: Uart4TxBuffer = [0; UART4_TX_BUFFER_SIZE],
        osd_uart_tx_channel: UartOwnedTxChannel = UartOwnedTxChannel::new()
    ])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut rcc = ferrowasp_stm32f4::clocks::freeze_hse(
            cx.device.RCC.constrain(),
            8_000_000,
            SYSTEM_CLOCK_HZ,
            false,
        );

        Mono::start(cx.core.SYST, SYSTEM_CLOCK_HZ);

        let gpioa = cx.device.GPIOA.split(&mut rcc);

        let gpioc = cx.device.GPIOC.split(&mut rcc);

        let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);

        let mut exti = cx.device.EXTI;

        let user_button = Input::new(gpioc.pc13, Pull::Up);
        let user_button =
            ferrowasp_stm32f4::exti::init_input(user_button, &mut syscfg, &mut exti, Edge::Falling);

        let mut led2 = gpioa.pa5.into_push_pull_output_in_state(PinState::Low);
        led2.set_internal_resistor(Pull::None);
        led2.set_speed(Speed::Low);

        let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);

        let osd_uart_parts = ferrowasp_stm32f4::uart_dma::init_uart4_msp_osd(
            Uart4MspResources {
                tx_pin: gpioa.pa0,
                rx_pin: gpioa.pa1,
                uart: cx.device.UART4,
                rx_dma: dma1.2,
                tx_dma: dma1.4,
            },
            &mut rcc,
            UartRxStorageResources {
                buffers: cx.local.osd_uart_rx_buffers,
                free_queue: cx.local.osd_uart_rx_free_queue,
                filled_queue: cx.local.osd_uart_rx_filled_queue,
            },
            cx.local.osd_uart_tx_buffer,
        );
        let osd_uart_rx = osd_uart_parts.rx_irq;
        let osd_uart_rx_parser = osd_uart_parts.parser;
        let (osd_uart_rx_producer, osd_uart_rx_reader, osd_uart_rx_discontinuities) =
            cx.local.osd_uart_rx_channel.split();
        // RX parser and producer remain separate until the endpoint bridge task is added.
        let osd_uart_tx_dma = osd_uart_parts.tx_dma;
        let (osd_uart_tx_writer, osd_uart_tx_owner, osd_uart_tx_completion) =
            cx.local.osd_uart_tx_channel.split();

        blink_led::spawn().expect("init must spawn declared task blink_led");
        osd_uart_tx_worker::spawn().expect("init must spawn declared task osd_uart_tx_worker");

        (
            Shared {
                button_enabled: false,
                osd_uart_rx: osd_uart_rx,
                osd_uart_tx_dma: osd_uart_tx_dma,
            },
            Local {
                user_button,
                led2,
                osd_uart_rx_parser,
                osd_uart_rx_producer,
                osd_uart_rx_reader,
                osd_uart_rx_discontinuities,
                osd_uart_tx_writer,
                osd_uart_tx_owner,
                osd_uart_tx_completion,
            },
        )
    }

    /// Handles one STM32F4 EXTI button interrupt.
    #[task(
        binds = EXTI15_10,
        priority = 2,
        local = [user_button],
        shared = [button_enabled]
    )]
    fn button_exti(mut cx: button_exti::Context) {
        if !cx.local.user_button.check_interrupt() {
            return;
        }

        cx.local.user_button.clear_interrupt_pending_bit();

        let toggle_on_press = true;
        let enabled = cx.shared.button_enabled.lock(|enabled| {
            if toggle_on_press {
                *enabled = !*enabled;
            }
            *enabled
        });

        let _ = observe_button_change::spawn(enabled);
    }

    /// Blinks its local LED while the shared enabled flag is set.
    #[task(priority = 1, local = [led2], shared = [button_enabled])]
    async fn blink_led(mut cx: blink_led::Context) {
        loop {
            let enabled = cx.shared.button_enabled.lock(|enabled| *enabled);
            if enabled {
                let _ = cx.local.led2.toggle();
            } else {
                let _ = cx.local.led2.set_low();
            }

            Mono::delay(500.millis()).await;
        }
    }

    /// Reports the enabled state produced by the button interrupt task.
    #[task(priority = 1)]
    async fn observe_button_change(_cx: observe_button_change::Context, enabled: bool) {
        if enabled {
            defmt::info!("button enabled LED blinking");
        } else {
            defmt::info!("button disabled LED blinking");
        }
    }

    /// Services one STM32F4 UART peripheral IDLE interrupt.
    #[task(binds = UART4, priority = 6, shared = [osd_uart_rx])]
    fn osd_uart_rx_idle_irq(mut cx: osd_uart_rx_idle_irq::Context) {
        match cx
            .shared
            .osd_uart_rx
            .lock(UartRxIrqService::service_idle_irq)
        {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::Delivered | UartRxIrqOutcome::NoChunk => {
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX peripheral error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX IDLE buffer delivery error")
            }
        }
    }

    /// Services one STM32F4 UART receive-DMA interrupt.
    #[task(binds = DMA1_STREAM2, priority = 6, shared = [osd_uart_rx])]
    fn osd_uart_rx_dma_irq(mut cx: osd_uart_rx_dma_irq::Context) {
        match cx
            .shared
            .osd_uart_rx
            .lock(UartRxIrqService::service_dma_irq)
        {
            UartRxIrqOutcome::Ignored | UartRxIrqOutcome::Delivered | UartRxIrqOutcome::NoChunk => {
            }
            UartRxIrqOutcome::DmaError => defmt::warn!("UART RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("UART RX DMA buffer delivery error")
            }
        }
    }

    /// Services one STM32F4 UART transmit-DMA interrupt.
    #[task(
        binds = DMA1_STREAM4,
        priority = 6,
        local = [osd_uart_tx_completion],
        shared = [osd_uart_tx_dma]
    )]
    fn osd_uart_tx_dma_irq(mut cx: osd_uart_tx_dma_irq::Context) {
        let outcome = cx
            .shared
            .osd_uart_tx_dma
            .lock(UartTxDmaService::service_irq);

        match outcome {
            UartTxIrqOutcome::Ignored => {}
            UartTxIrqOutcome::Completed => {
                if cx.local.osd_uart_tx_completion.complete().is_err() {
                    defmt::warn!("UART TX completion arrived without an in-flight chunk");
                }
            }
            UartTxIrqOutcome::DmaError(error) => {
                cx.local
                    .osd_uart_tx_completion
                    .fail(SerialFault::DmaTransfer);
                match error {
                    ferrowasp_stm32f4::serial::UartTxDmaError::Transfer => {
                        defmt::warn!("UART TX DMA transfer error")
                    }
                    ferrowasp_stm32f4::serial::UartTxDmaError::DirectMode => {
                        defmt::warn!("UART TX DMA direct-mode error")
                    }
                }
            }
        }
    }

    /// Drains queued UART chunks through DMA and awaits each IRQ completion.
    #[task(priority = 4, local = [osd_uart_tx_owner], shared = [osd_uart_tx_dma])]
    async fn osd_uart_tx_worker(mut cx: osd_uart_tx_worker::Context) {
        loop {
            let chunk = match cx.local.osd_uart_tx_owner.next_chunk().await {
                Ok(chunk) => chunk,
                Err(_) => {
                    defmt::warn!("UART TX worker stopped before DMA start");
                    return;
                }
            };

            let start = cx
                .shared
                .osd_uart_tx_dma
                .lock(|tx_dma| tx_dma.start_chunk(&chunk));
            if let Err(error) = start {
                let fault = match error {
                    UartTxStartError::InvalidChunk => SerialFault::InvalidChunk,
                    UartTxStartError::Busy | UartTxStartError::TransferMissing => {
                        SerialFault::InvalidState
                    }
                };
                cx.local.osd_uart_tx_owner.fail(fault);
                defmt::warn!("UART TX DMA start failed");
                return;
            }

            if cx.local.osd_uart_tx_owner.wait_completion().await.is_err() {
                defmt::warn!("UART TX worker stopped after DMA start");
                return;
            }
        }
    }
}

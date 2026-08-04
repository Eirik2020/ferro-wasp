#![no_main]
#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]

use panic_halt as _;
use defmt_rtt as _;

#[rtic::app(device = ferrowasp_stm32f4::rtic::hal::pac, peripherals = true, dispatchers = [EXTI0, EXTI1])]
mod app {
    use ferrowasp_stm32f4::uart_dma::{UartRxIrqOutcome, UartRxReadOutcome, UART_RX_BUFFER_SIZE};
    use sbus_rs::StreamingParser;
    use ferrowasp_io_core::digital::prelude::{OutputPin, StatefulOutputPin};
    use ferrowasp_stm32f4::rtic::prelude::*;

    // Monotonic timer declarations generated from the board declaration.
    systick_monotonic!(Mono, 1_000);

    #[shared]
    struct Shared {
        sbus_rx: ferrowasp_stm32f4::uart_dma::Uart2SbusRx,
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        led3: Pin<'A', 5, Output<PushPull>>,
        user_button: Pin<'C', 13, Input>,
    }

    #[init(local = [
        sbus_rx_buffers: ferrowasp_stm32f4::app_storage::UartRxBufferBank = ferrowasp_stm32f4::app_storage::new_uart_rx_buffer_bank(),
        sbus_rx_free_queue: ferrowasp_stm32f4::app_storage::UartRxFreeQueue = ferrowasp_stm32f4::app_storage::UartRxFreeQueue::new(),
        sbus_rx_filled_queue: ferrowasp_stm32f4::app_storage::UartRxFilledQueue = ferrowasp_stm32f4::app_storage::UartRxFilledQueue::new(),
    ])]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Board clock, monotonic, and hardware initialization.
        let mut rcc =
            ferrowasp_stm32f4::clocks::freeze_hsi(cx.device.RCC.constrain(), 84000000, false);
        Mono::start(cx.core.SYST, 84000000);
        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpioc = cx.device.GPIOC.split(&mut rcc);
        let mut syscfg = cx.device.SYSCFG.constrain(&mut rcc);
        let mut exti = cx.device.EXTI;
        let dma1 = StreamsTuple::new(cx.device.DMA1, &mut rcc);
        let mut led3 = gpioa.pa5.into_push_pull_output_in_state(PinState::Low);
        led3.set_internal_resistor(Pull::None);
        led3.set_speed(Speed::Low);
        let user_button = Input::new(gpioc.pc13, Pull::Up);
        let user_button = ferrowasp_stm32f4::exti::init_input(user_button, &mut syscfg, &mut exti, Edge::Falling);
        let sbus_rx = ferrowasp_stm32f4::uart_dma::init_usart2_sbus_rx_only(
            ferrowasp_stm32f4::uart_dma::Usart2SbusRxOnlyResources {
                rx_pin: gpioa.pa3,
                usart: cx.device.USART2,
                rx_dma: dma1.5,
            },
            &mut rcc,
            ferrowasp_stm32f4::app_storage::UartRxStorageResources {
                buffers: cx.local.sbus_rx_buffers,
                free_queue: cx.local.sbus_rx_free_queue,
                filled_queue: cx.local.sbus_rx_filled_queue,
            },
        );

        // Initial tasks selected by AppDeclaration::init.spawns.
        blink_led::spawn().expect("init must spawn declared task blink_led");
        sbus_parse::spawn().expect("init must spawn declared task sbus_parse");

        (Shared { sbus_rx, blink_enabled: true }, Local { led3, user_button })
    }

    #[task(priority = 1, local = [led3], shared = [blink_enabled])]
    async fn blink_led(mut cx: blink_led::Context) {
        let mut blink_count = 0_u32;
        loop {
            Mono::delay(1000.millis()).await;
            let enabled = cx.shared.blink_enabled.lock(|enabled| *enabled);
            if enabled {
                let _ = StatefulOutputPin::toggle(cx.local.led3);
                blink_count = blink_count.wrapping_add(1);
                report_blink::spawn(blink_count)
                    .expect("blink report task queue must have capacity");
            } else {
                let _ = OutputPin::set_low(cx.local.led3);
            }
            Mono::delay(5000.millis()).await;
        }
    }

    #[task(priority = 1)]
    async fn report_blink(cx: report_blink::Context, count: u32) {
        let _ = cx;
        defmt::info!("Blink {}", count);
    }

    #[task(binds = EXTI15_10, priority = 2, local = [user_button], shared = [blink_enabled])]
    fn button_exti(mut cx: button_exti::Context) {
        cx.local.user_button.clear_interrupt_pending_bit();
        let enabled = cx.shared.blink_enabled.lock(|enabled| {
            *enabled = !*enabled;
            *enabled
        });
        defmt::info!("Blink enabled: {}", enabled);
    }

    #[task(binds = DMA1_STREAM5, priority = 3, shared = [sbus_rx])]
    fn sbus_dma_irq(mut cx: sbus_dma_irq::Context) {
        match cx.shared.sbus_rx.lock(|rx| rx.service_dma_irq()) {
            UartRxIrqOutcome::Ignored
            | UartRxIrqOutcome::Delivered
            | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::DmaError => defmt::warn!("SBUS RX DMA error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("SBUS RX DMA buffer delivery error")
            }
        }
    }

    #[task(binds = USART2, priority = 3, shared = [sbus_rx])]
    fn sbus_idle_irq(mut cx: sbus_idle_irq::Context) {
        match cx.shared.sbus_rx.lock(|rx| rx.service_idle_irq()) {
            UartRxIrqOutcome::Ignored
            | UartRxIrqOutcome::Delivered
            | UartRxIrqOutcome::NoChunk => {}
            UartRxIrqOutcome::DmaError => defmt::warn!("SBUS RX UART error"),
            UartRxIrqOutcome::DeliveryError(_) => {
                defmt::warn!("SBUS RX IDLE buffer delivery error")
            }
        }
    }

    #[task(priority = 2, shared = [sbus_rx])]
    async fn sbus_parse(mut cx: sbus_parse::Context) {
        let mut parser = StreamingParser::new();
        let mut bytes = [0_u8; UART_RX_BUFFER_SIZE];

        loop {
            match cx.shared.sbus_rx.lock(|rx| rx.read_chunk(&mut bytes)) {
                UartRxReadOutcome::NoChunk => {}
                UartRxReadOutcome::RecycleError => {
                    defmt::warn!("SBUS RX buffer recycle error");
                }
                UartRxReadOutcome::Chunk(len) => {
                    for packet in parser.push_bytes(&bytes[..len]) {
                        match packet {
                            Ok(packet) => defmt::info!(
                                "SBUS channels: [{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}], d1={}, d2={}, frame_lost={}, failsafe={}",
                                packet.channels[0], packet.channels[1], packet.channels[2],
                                packet.channels[3], packet.channels[4], packet.channels[5],
                                packet.channels[6], packet.channels[7], packet.channels[8],
                                packet.channels[9], packet.channels[10], packet.channels[11],
                                packet.channels[12], packet.channels[13], packet.channels[14],
                                packet.channels[15], packet.flags.d1, packet.flags.d2,
                                packet.flags.frame_lost, packet.flags.failsafe,
                            ),
                            Err(_) => defmt::warn!("Invalid SBUS frame"),
                        }
                    }
                }
            }

            Mono::delay(1.millis()).await;
        }
    }
}

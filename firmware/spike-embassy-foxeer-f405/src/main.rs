//! EXPERIMENT: embassy-stm32 under the FerroForge `app!`, on the Foxeer F405
//! V2 pinout. Compile-only. It has no safety kernel and drives no actuator
//! path; it exists to show which ferro-wasp pieces survive a HAL swap.
//!
//! - `ferrowasp-drivers` (MPU6500) is reused unchanged over embassy's blocking
//!   SPI, because it bounds only on embedded-hal 1.0.
//! - The IMU data-ready interrupt stays an RTIC hardware task (`binds =
//!   EXTI4`). Embassy's `exti` feature is off so it does not claim the vector;
//!   the EXTI registers are then configured through embassy's raw PAC, which
//!   is what any hardware task that embassy has no driver for would look like.
//! - `blink` from FerroForge's portable tasks drives an embassy `Output`.
//! - SBUS-style reception on USART2 uses an embassy async DMA ring buffer in
//!   an RTIC software task, through the portable `receive` task.
//! - embassy-time runs on TIM5; TIM1/TIM8 stay free for DShot.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = embassy_stm32,
    peripherals = false,
    dispatchers = [CAN1_TX, CAN2_TX, CAN1_RX0],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use embassy_stm32::{
        bind_interrupts,
        gpio::{Input, Level, Output, Pull, Speed},
        mode::Blocking,
        pac,
        peripherals,
        spi::{self, Spi, mode::Master},
        time::Hertz,
        usart::{self, RingBufferedUartRx, UartRx},
    };
    use ferroforge_task_blinky::{blink, report};
    use ferroforge_task_spike_embassy_async::{pace, receive};
    use ferrowasp_drivers::mpu6500;

    bind_interrupts!(struct Irqs {
        USART2 => usart::InterruptHandler<peripherals::USART2>;
        DMA1_STREAM5 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH5>;
    });

    #[shared]
    struct Shared {
        blink_enabled: bool,
        gyro_raw: [i16; 3],
    }

    #[local]
    struct Local {
        status_led: Output<'static>,
        blink_count: u32,
        imu_spi: Spi<'static, Blocking, Master>,
        imu_cs: Output<'static>,
        // Held so the pin stays an input with its pull; read through EXTI.
        _imu_int: Input<'static>,
        sbus_rx: RingBufferedUartRx<'static>,
    }

    #[init(local = [sbus_dma: [u8; 128] = [0; 128]])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut config = embassy_stm32::Config::default();
        {
            use embassy_stm32::rcc::*;
            config.rcc.hse = Some(Hse { freq: Hertz(8_000_000), mode: HseMode::Oscillator });
            config.rcc.pll_src = PllSource::HSE;
            config.rcc.pll = Some(Pll {
                prediv: PllPreDiv::DIV4,
                mul: PllMul::MUL168,
                divp: Some(PllPDiv::DIV2),
                divq: Some(PllQDiv::DIV7),
                divr: None,
            });
            config.rcc.ahb_pre = AHBPrescaler::DIV1;
            config.rcc.apb1_pre = APBPrescaler::DIV4;
            config.rcc.apb2_pre = APBPrescaler::DIV2;
            config.rcc.sys = Sysclk::PLL1_P;
        }
        let p = embassy_stm32::init(config);
        Mono::start(cx.core.SYST, 168_000_000);

        // PC15 is assumed from the Betaflight FOXEERF405 target; unverified.
        let status_led = Output::new(p.PC15, Level::Low, Speed::Low);

        let mut spi_config = spi::Config::default();
        spi_config.frequency = Hertz(1_000_000);
        let mut imu_spi = Spi::new_blocking(p.SPI1, p.PA5, p.PA7, p.PA6, spi_config);
        let mut imu_cs = Output::new(p.PA4, Level::High, Speed::VeryHigh);
        let mut delay = embassy_time::Delay;
        match mpu6500::read_reg(&mut imu_spi, &mut imu_cs, mpu6500::Register::WHO_AM_I) {
            Ok(id) => defmt::info!("imu who_am_i={=u8:#x}", id),
            Err(_) => defmt::warn!("imu probe failed"),
        }
        let _ = mpu6500::enable_interrupt(&mut imu_spi, &mut imu_cs, &mut delay);

        // PC4 data-ready -> EXTI4, rising edge, by register because embassy's
        // EXTI driver is async-only and would own the vector.
        let imu_int = Input::new(p.PC4, Pull::Down);
        pac::RCC.apb2enr().modify(|w| w.set_syscfgen(true));
        pac::SYSCFG.exticr(1).modify(|w| w.set_exti(0, 2));
        pac::EXTI.rtsr(0).modify(|w| w.set_line(4, true));
        pac::EXTI.imr(0).modify(|w| w.set_line(4, true));

        let sbus_rx = UartRx::new(p.USART2, p.PA3, p.DMA1_CH5, Irqs, usart::Config::default())
            .unwrap()
            .into_ring_buffered(cx.local.sbus_dma);

        heartbeat::spawn().unwrap();
        sbus::spawn().unwrap();
        control_loop::spawn().unwrap();

        (
            Shared { blink_enabled: true, gyro_raw: [0; 3] },
            Local {
                status_led,
                blink_count: 0,
                imu_spi,
                imu_cs,
                _imu_int: imu_int,
                sbus_rx,
            },
        )
    }

    /// The IMU data-ready interrupt, as the Foxeer firmware binds it today.
    #[task(binds = EXTI4, priority = 4, local = [imu_spi, imu_cs], shared = [gyro_raw])]
    fn imu_ready(mut cx: imu_ready::Context) {
        pac::EXTI.pr(0).write(|w| w.set_line(4, true));
        if let Ok(gyro) = mpu6500::read_gyro_raw(cx.local.imu_spi, cx.local.imu_cs) {
            cx.shared.gyro_raw.lock(|slot| *slot = gyro);
        }
    }

    #[task(
        from = blink,
        priority = 1,
        local = [led = status_led, count = blink_count],
        shared = [enabled = blink_enabled],
        config = [period_ms: u32 = 500],
        spawn = [report = telemetry],
    )]
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    #[task(
        from = receive,
        priority = 2,
        local = [rx = sbus_rx],
        config = [timeout_ms: u32 = 50],
        spawn = [report = telemetry],
    )]
    async fn sbus(cx: sbus::Context) -> !;

    #[task(from = pace, priority = 3, config = [period_us: u32 = 250])]
    async fn control_loop(cx: control_loop::Context) -> !;
}

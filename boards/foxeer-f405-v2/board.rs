//! Board-owned hardware declarations consumed by application components.

use crate::backends::stm32f4::board_prelude::*;

/// Physical TIM2 peripheral available to application composition.
pub const TIM2: TimerHardwareDeclaration =
    TimerHardwareDeclaration::new("tim2", TimerPeripheral::Tim2);

/// Physical TIM4 peripheral reserved for the periodic control scheduler.
pub const TIM4: TimerHardwareDeclaration =
    TimerHardwareDeclaration::new("tim4", TimerPeripheral::Tim4);

/// Physical TIM5 peripheral available to application composition.
pub const TIM5: TimerHardwareDeclaration =
    TimerHardwareDeclaration::new("tim5", TimerPeripheral::Tim5);

/// Physical TIM6 peripheral reserved for the 8 kHz I/O deadline watchdog.
pub const TIM6: TimerHardwareDeclaration =
    TimerHardwareDeclaration::new("tim6", TimerPeripheral::Tim6);

/// Exact Foxeer PC0/PC1 ADC1 observation route.
pub const FOXEER_ADC1: AdcObservationHardwareDeclaration = AdcObservationHardwareDeclaration::new(
    "adc1_observation",
    AdcPeripheral::Adc1,
    PinId::new(GpioPort::C, 0),
    10,
    PinId::new(GpioPort::C, 1),
    11,
    DmaRoute::new(
        DmaController::Dma2,
        DmaStream::Stream4,
        DmaChannel::Channel0,
    ),
);

/// Exact Foxeer onboard SPI2 NOR route.
pub const FOXEER_SPI2_NOR: SpiNorHardwareDeclaration = SpiNorHardwareDeclaration::new(
    "spi2_nor",
    SpiPeripheral::Spi2,
    PinId::new(GpioPort::B, 13),
    PinId::new(GpioPort::C, 2),
    PinId::new(GpioPort::C, 3),
    PinId::new(GpioPort::B, 12),
    SpiMode::Mode0,
    10_000_000,
);

/// Exact Foxeer OTG_FS route and stable CDC identity.
pub const FOXEER_USB_CDC: UsbCdcHardwareDeclaration = UsbCdcHardwareDeclaration::new(
    "usb_cdc",
    UsbPeripheral::OtgFs,
    PinId::new(GpioPort::A, 11),
    PinId::new(GpioPort::A, 12),
    UsbCdcIdentityDeclaration {
        manufacturer: "FerroWasp",
        product: "FerroWasp Foxeer Debug",
        serial_number: "FW-FOX-F405V2",
    },
);

/// Exact reviewed Foxeer F405 V2 physical DShot bank.
pub const FOXEER_DSHOT: DshotBankHardwareDeclaration = DshotBankHardwareDeclaration::new(
    "foxeer_dshot",
    [
        DshotLaneHardwareDeclaration::new(
            1,
            1,
            DshotTimerChannel::Tim1Ch1,
            PinId::new(GpioPort::A, 8),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream1,
                DmaChannel::Channel6,
            ),
        ),
        DshotLaneHardwareDeclaration::new(
            2,
            2,
            DshotTimerChannel::Tim8Ch4,
            PinId::new(GpioPort::C, 9),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream7,
                DmaChannel::Channel7,
            ),
        ),
        DshotLaneHardwareDeclaration::new(
            3,
            3,
            DshotTimerChannel::Tim8Ch3,
            PinId::new(GpioPort::C, 8),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream2,
                DmaChannel::Channel0,
            ),
        ),
        DshotLaneHardwareDeclaration::new(
            4,
            4,
            DshotTimerChannel::Tim1Ch3N,
            PinId::new(GpioPort::B, 15),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream6,
                DmaChannel::Channel6,
            ),
        ),
    ],
);

/// Verified Foxeer sensor-frame to Forward-Right-Down body-frame rotation.
///
/// The SPI parser applies this physical installation fact exactly once. A
/// future control task must apply only `BODY_RATE_TO_RATE_CONTROLLER_MAP` to
/// the resulting body rates; it must not repeat this board rotation.
pub const FOXEER_IMU_SENSOR_TO_BODY: ImuOrientation = ImuOrientation::new([1, 0, 2], [-1, -1, -1]);

/// Hardware registry consumed by the example application composition.
pub const BOARD: BoardDeclaration = BoardDeclaration::new(
    "foxeer_f405_v2",
    McuDeclaration::new(
        Mcu::Stm32f405,
        ClockDeclaration::hse(8_000_000, 168_000_000),
    ),
    &[
        HardwareEndpointDeclaration::adc_observation(FOXEER_ADC1),
        HardwareEndpointDeclaration::serial(
            SerialHardwareDeclaration::new("esc_telemetry", SerialPeripheral::Usart1).rx(
                SerialRoute::dma(
                    PinId::new(GpioPort::A, 10),
                    DmaRoute::new(
                        DmaController::Dma2,
                        DmaStream::Stream5,
                        DmaChannel::Channel4,
                    ),
                ),
            ),
        ),
        HardwareEndpointDeclaration::serial(
            SerialHardwareDeclaration::new("serial1", SerialPeripheral::Usart2)
                .rx(SerialRoute::dma(
                    PinId::new(GpioPort::A, 3),
                    DmaRoute::new(
                        DmaController::Dma1,
                        DmaStream::Stream5,
                        DmaChannel::Channel4,
                    ),
                ))
                .tx(SerialRoute::dma(
                    PinId::new(GpioPort::A, 2),
                    DmaRoute::new(
                        DmaController::Dma1,
                        DmaStream::Stream6,
                        DmaChannel::Channel4,
                    ),
                )),
        ),
        HardwareEndpointDeclaration::serial(
            SerialHardwareDeclaration::new("serial2", SerialPeripheral::Uart4)
                .rx(SerialRoute::dma(
                    PinId::new(GpioPort::A, 1),
                    DmaRoute::new(
                        DmaController::Dma1,
                        DmaStream::Stream2,
                        DmaChannel::Channel4,
                    ),
                ))
                .tx(SerialRoute::dma(
                    PinId::new(GpioPort::A, 0),
                    DmaRoute::new(
                        DmaController::Dma1,
                        DmaStream::Stream4,
                        DmaChannel::Channel4,
                    ),
                )),
        ),
        HardwareEndpointDeclaration::spi(
            SpiHardwareDeclaration::new(
                "spi1",
                SpiBus::new(
                    SpiPeripheral::Spi1,
                    SpiPins::new(
                        PinId::new(GpioPort::A, 5),
                        PinId::new(GpioPort::A, 6),
                        PinId::new(GpioPort::A, 7),
                    ),
                    SpiDmaRoutes::new(
                        DmaRoute::new(
                            DmaController::Dma2,
                            DmaStream::Stream0,
                            DmaChannel::Channel3,
                        ),
                        DmaRoute::new(
                            DmaController::Dma2,
                            DmaStream::Stream3,
                            DmaChannel::Channel3,
                        ),
                    ),
                ),
            )
            .with_imus(&[ImuInstallationDeclaration::new(
                ImuInstallationId::new(1),
                PinId::new(GpioPort::A, 4),
                PinId::new(GpioPort::C, 4),
                InterruptEdge::Rising,
                FOXEER_IMU_SENSOR_TO_BODY,
            )]),
        ),
        HardwareEndpointDeclaration::spi_nor(FOXEER_SPI2_NOR),
        HardwareEndpointDeclaration::usb_cdc(FOXEER_USB_CDC),
    ],
)
.with_timers(&[TIM2, TIM4, TIM5, TIM6])
.with_dshot_banks(&[FOXEER_DSHOT]);

#[cfg(test)]
mod tests {
    use ferrowasp_core::frames::BODY_RATE_TO_RATE_CONTROLLER_MAP;

    use super::*;

    fn declared_imu_orientation() -> ImuOrientation {
        BOARD
            .imu(ImuInstallationId::new(1))
            .expect("Foxeer SPI1 IMU installation must exist")
            .1
            .orientation
    }

    #[test]
    fn imu_installation_matches_the_current_golden_foxeer_profile() {
        let sensor_to_body = declared_imu_orientation();

        assert_eq!(sensor_to_body, ImuOrientation::new([1, 0, 2], [-1, -1, -1]));
        assert_eq!(sensor_to_body.map_raw([10, 20, -30]), [-20, -10, 30]);
    }

    #[test]
    fn measured_pitch_sign_keeps_controller_compatibility_separate() {
        let sensor_to_body = declared_imu_orientation();
        let sensor_to_controller = sensor_to_body.then(BODY_RATE_TO_RATE_CONTROLLER_MAP);

        let sensor_nose_up = [-100, 0, 0];
        assert_eq!(sensor_to_body.map_raw(sensor_nose_up), [0, 100, 0]);
        assert_eq!(sensor_to_controller.map_raw(sensor_nose_up), [0, -100, 0]);

        let sensor_nose_down = [100, 0, 0];
        assert_eq!(sensor_to_body.map_raw(sensor_nose_down), [0, -100, 0]);
        assert_eq!(sensor_to_controller.map_raw(sensor_nose_down), [0, 100, 0]);
    }

    #[test]
    fn controller_compatibility_preserves_verified_roll_and_yaw_signs() {
        let sensor_to_controller =
            declared_imu_orientation().then(BODY_RATE_TO_RATE_CONTROLLER_MAP);

        assert_eq!(sensor_to_controller.map_raw([0, -100, 0]), [100, 0, 0]);
        assert_eq!(sensor_to_controller.map_raw([0, 0, -100]), [0, 0, 100]);
    }

    #[test]
    fn physical_actuator_routes_match_the_controlled_foxeer_map() {
        let bank = BOARD.dshot_bank("foxeer_dshot").unwrap();
        assert_eq!(bank.lanes, FOXEER_DSHOT.lanes);
        assert!(bank.lanes[3].timer_channel.is_complementary());
        assert!(
            bank.lanes[..3]
                .iter()
                .all(|lane| !lane.timer_channel.is_complementary())
        );

        let telemetry = BOARD.serial("esc_telemetry").unwrap();
        assert_eq!(telemetry.port.peripheral, SerialPeripheral::Usart1);
        assert_eq!(telemetry.port.rx.unwrap().pin, PinId::new(GpioPort::A, 10));
        assert_eq!(
            telemetry.port.rx.unwrap().dma.unwrap(),
            DmaRoute::new(
                DmaController::Dma2,
                DmaStream::Stream5,
                DmaChannel::Channel4,
            )
        );
    }

    #[test]
    fn mandatory_service_routes_match_the_golden_foxeer_map() {
        assert_eq!(BOARD.adc_observation(FOXEER_ADC1.id), Some(&FOXEER_ADC1));
        assert_eq!(BOARD.spi_nor(FOXEER_SPI2_NOR.id), Some(&FOXEER_SPI2_NOR));
        assert_eq!(BOARD.usb_cdc(FOXEER_USB_CDC.id), Some(&FOXEER_USB_CDC));
        assert_eq!(BOARD.timer(TIM6.id), Some(&TIM6));
    }
}

//! Bounded observation state shared by non-authoritative flight services.
//!
//! These values may be displayed, logged, or reported over USB. They never
//! grant arming permission and never carry actuator commands.

use ferrowasp_mspv1::MspOsdTelemetry;

/// Maximum age of an ADC observation presented as current battery data.
pub const ADC_OBSERVATION_MAX_AGE_MS: u32 = 500;

/// Latest converted voltage/current observation and its provenance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryObservation {
    /// Pack voltage in decivolts.
    pub voltage_decivolts: u8,
    /// Latched detected cell count.
    pub cell_count: u8,
    /// Per-cell voltage in centivolts.
    pub cell_voltage_centivolts: u16,
    /// Pack current in centiamps.
    pub current_centiamps: i16,
    /// Direct voltage-sense ADC conversion in millivolts.
    pub adc_voltage_mv: u16,
    /// Direct current-sense ADC conversion in millivolts.
    pub adc_current_mv: u16,
    /// Monotonic accepted-sample sequence.
    pub sequence: u32,
    /// Millisecond timestamp of the latest accepted sample.
    pub observed_at_ms: u32,
    /// Number of transport or conversion faults observed by the ADC owner.
    pub faults: u32,
    /// Whether at least one complete conversion has been accepted.
    pub valid: bool,
}

impl BatteryObservation {
    /// Replaces all sample fields atomically under the caller's RTIC lock.
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        voltage_decivolts: u8,
        cell_count: u8,
        cell_voltage_centivolts: u16,
        current_centiamps: i16,
        adc_voltage_mv: u16,
        adc_current_mv: u16,
        observed_at_ms: u32,
    ) {
        self.voltage_decivolts = voltage_decivolts;
        self.cell_count = cell_count;
        self.cell_voltage_centivolts = cell_voltage_centivolts;
        self.current_centiamps = current_centiamps;
        self.adc_voltage_mv = adc_voltage_mv;
        self.adc_current_mv = adc_current_mv;
        self.sequence = self.sequence.wrapping_add(1);
        self.observed_at_ms = observed_at_ms;
        self.valid = true;
    }

    /// Records a bounded fault without making an old sample appear fresh.
    pub fn record_fault(&mut self) {
        self.faults = self.faults.saturating_add(1);
    }

    /// Returns whether the observation is valid at the supplied time.
    pub const fn is_fresh(self, now_ms: u32) -> bool {
        self.valid && now_ms.wrapping_sub(self.observed_at_ms) <= ADC_OBSERVATION_MAX_AGE_MS
    }
}

/// Latest observation-only inputs used by OSD, USB, heartbeat, and logging.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightServiceTelemetry {
    /// Safety-owned arming state copied for display and configuration gating.
    pub armed: bool,
    /// Whether the safety-owned RC link is currently valid.
    pub rc_link_valid: bool,
    /// Golden mapped roll, pitch, and yaw requests in degrees per second.
    pub rc_rates_dps: [i16; 3],
    /// Golden mapped throttle request.
    pub rc_throttle: u32,
    /// Latest body attitude estimate in degrees.
    pub angles_deg: [f32; 3],
    /// Latest filtered controller rates in degrees per second.
    pub rates_dps: [f32; 3],
    /// Latest decoded IMU sequence observed by control.
    pub imu_sequence: u32,
    /// Latest completed 400 Hz control sequence.
    pub control_sequence: u32,
    /// Whether control currently considers the IMU stale.
    pub imu_stale: bool,
    /// Freshness-tracked ADC observation.
    pub battery: BatteryObservation,
}

impl FlightServiceTelemetry {
    /// Empty, fail-closed boot observation.
    pub const fn new() -> Self {
        Self {
            armed: false,
            rc_link_valid: false,
            rc_rates_dps: [0; 3],
            rc_throttle: 0,
            angles_deg: [0.0; 3],
            rates_dps: [0.0; 3],
            imu_sequence: 0,
            control_sequence: 0,
            imu_stale: true,
            battery: BatteryObservation {
                voltage_decivolts: 0,
                cell_count: 0,
                cell_voltage_centivolts: 0,
                current_centiamps: 0,
                adc_voltage_mv: 0,
                adc_current_mv: 0,
                sequence: 0,
                observed_at_ms: 0,
                faults: 0,
                valid: false,
            },
        }
    }

    /// Builds one bounded MSP snapshot, suppressing stale ADC values.
    pub fn msp_snapshot(self, now_ms: u32) -> MspOsdTelemetry {
        let battery = self.battery.is_fresh(now_ms).then_some(self.battery);
        MspOsdTelemetry {
            armed: self.armed,
            battery_voltage_v10: battery.map_or(0, |value| value.voltage_decivolts),
            battery_cell_count: battery.map_or(0, |value| value.cell_count),
            battery_cell_voltage_v100: battery.map_or(0, |value| value.cell_voltage_centivolts),
            amperage_ca: battery.map_or(0, |value| value.current_centiamps),
            rc_roll: crate::osd::map_rate_to_msp_rc(self.rc_rates_dps[0]),
            rc_pitch: crate::osd::map_rate_to_msp_rc(self.rc_rates_dps[1]),
            rc_yaw: crate::osd::map_rate_to_msp_rc(self.rc_rates_dps[2]),
            rc_throttle: crate::osd::map_throttle_to_msp_rc(self.rc_throttle),
            osd_throttle: self.rc_throttle.min(2_000) as u16,
            roll_deg10: (self.angles_deg[0] * 10.0) as i16,
            pitch_deg10: (self.angles_deg[1] * 10.0) as i16,
            yaw_deg: self.angles_deg[2] as i16,
            imu_roll_dps: self.rates_dps[0] as i16,
            imu_pitch_dps: self.rates_dps[1] as i16,
            imu_yaw_dps: self.rates_dps[2] as i16,
            imu_roll_dps10: (self.rates_dps[0] * 10.0) as i16,
            imu_pitch_dps10: (self.rates_dps[1] * 10.0) as i16,
            imu_yaw_dps10: (self.rates_dps[2] * 10.0) as i16,
            imu_sequence: self.imu_sequence,
            imu_stale: self.imu_stale,
            control_sequence: self.control_sequence,
            ..MspOsdTelemetry::default()
        }
    }
}

impl Default for FlightServiceTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Observation-only status of the onboard flash owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageStatus {
    /// A plausible JEDEC device and supported layout were detected.
    pub ready: bool,
    /// Raw JEDEC identity bytes.
    pub jedec: [u8; 3],
    /// Detected flash capacity.
    pub capacity_bytes: u32,
    /// Next append-only log page.
    pub next_page: u32,
    /// Next flight identity.
    pub next_flight: u32,
    /// Records rejected by the bounded producer queue.
    pub dropped_records: u32,
    /// Terminal or recoverable flash-operation faults.
    pub faults: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adc_freshness_is_wrap_safe_and_stale_values_are_hidden_from_osd() {
        let mut telemetry = FlightServiceTelemetry::new();
        telemetry
            .battery
            .observe(250, 6, 417, 1_234, 2_273, 86, u32::MAX - 10);

        assert!(telemetry.battery.is_fresh(20));
        assert_eq!(telemetry.msp_snapshot(20).battery_voltage_v10, 250);
        assert!(!telemetry.battery.is_fresh(600));
        assert_eq!(telemetry.msp_snapshot(600).battery_voltage_v10, 0);
    }

    #[test]
    fn service_snapshot_is_observation_only_and_boots_fail_closed() {
        let snapshot = FlightServiceTelemetry::new();
        assert!(!snapshot.armed);
        assert!(!snapshot.rc_link_valid);
        assert!(snapshot.imu_stale);
        assert!(!snapshot.battery.valid);
    }
}

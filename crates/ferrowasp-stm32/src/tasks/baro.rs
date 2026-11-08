use dps310::{Coefficients as Dps310Coefficients, Dps310};
use ferrowasp_boundary_contracts::messages as observed;
use ferrowasp_io_contracts::link::LinkProducer;
use ferrowasp_io_contracts::time::TimestampMicros;
use fugit::ExtU64 as _;
use spl06::{Coefficients as Spl06Coefficients, Spl06};

/// The barometer's bus itself, as a board hands it over and the absence
/// sweep walks it.
pub type BaroI2c = crate::hal::i2c::I2c;

/// An I2C failure on the barometer's bus.
pub type BaroError = crate::hal::i2c::Error;

/// Which barometer a board's bus carries: the board's facts name the
/// part its footprint holds, the probe runs this driver's, and the
/// status line reports the kind beside the chip ID it answered with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartKind {
    /// The Goertek SPL06-007.
    Spl06,
    /// The Infineon DPS310, and the DPS368 that shares its interface:
    /// the parts answer the same chip ID, so the board's facts pick
    /// the driver.
    Dps310,
}

impl PartKind {
    /// The `BaroStatus` `PART_` code the status line carries.
    pub const fn status_code(self) -> u8 {
        match self {
            Self::Spl06 => observed::BaroStatus::PART_SPL06,
            Self::Dps310 => observed::BaroStatus::PART_DPS310,
        }
    }

    /// The part's first strapping, its driver's own address.
    pub const fn address(self) -> u8 {
        match self {
            Self::Spl06 => spl06::ADDRESS,
            Self::Dps310 => dps310::ADDRESS,
        }
    }

    /// The part's other strapping, its address pin's complement: both
    /// parts strap the same way.
    pub const fn other_address(self) -> u8 {
        self.address() | 0x01
    }

    /// Probe one address with this driver.
    fn probe(self, bus: BaroI2c, address: u8) -> Result<BaroProbe, ProbeFailed> {
        match self {
            Self::Spl06 => probe_spl06(bus, address),
            Self::Dps310 => probe_dps310(bus, address),
        }
    }
}

/// One compensated measurement pair.
pub struct BaroSample {
    /// Pascals.
    pub pressure_pa: u32,
    /// Centidegrees Celsius.
    pub temperature_dc: i16,
}

/// The board's barometer after its probe, whichever driver claimed it:
/// the part on its bus, the calibration page the probe read, and the
/// chip ID it answered with. The board's support probes the part,
/// configures its oversampling and moves this in whole; this task only
/// measures.
pub enum BaroProbe {
    /// An SPL06-007, its driver on its bus.
    Spl06 {
        part: Spl06<BaroI2c>,
        coefficients: Spl06Coefficients,
        chip_id: u8,
    },
    /// A DPS310, or the DPS368 that shares its interface: its driver on
    /// its bus, and the flag that the first temperature after the
    /// probe's configure is still to be flushed, as the part's driver
    /// documents.
    Dps310 {
        part: Dps310<BaroI2c>,
        coefficients: Dps310Coefficients,
        chip_id: u8,
        flush_pending: bool,
    },
}

impl BaroProbe {
    /// The chip ID the probe read.
    pub const fn chip_id(&self) -> u8 {
        match self {
            Self::Spl06 { chip_id, .. } | Self::Dps310 { chip_id, .. } => *chip_id,
        }
    }

    /// The address the probe found the part at.
    pub const fn address(&self) -> u8 {
        match self {
            Self::Spl06 { part, .. } => part.address(),
            Self::Dps310 { part, .. } => part.address(),
        }
    }

    /// Which driver measured the part, as the status line reports it.
    pub const fn part(&self) -> PartKind {
        match self {
            Self::Spl06 { .. } => PartKind::Spl06,
            Self::Dps310 { .. } => PartKind::Dps310,
        }
    }

    /// Command one temperature measurement; the part runs it on its own
    /// clock, and the caller gives it time before reading.
    pub fn start_temperature(&mut self) -> Result<(), BaroError> {
        match self {
            Self::Spl06 { part, .. } => part.start_temperature_measurement(),
            Self::Dps310 { part, .. } => part.start_temperature_measurement(),
        }
    }

    /// The last temperature measurement's raw value.
    pub fn read_temperature_raw(&mut self) -> Result<i32, BaroError> {
        match self {
            Self::Spl06 { part, .. } => part.read_temperature_raw(),
            Self::Dps310 { part, .. } => part.read_temperature_raw(),
        }
    }

    /// Command one pressure measurement, as the temperature's.
    pub fn start_pressure(&mut self) -> Result<(), BaroError> {
        match self {
            Self::Spl06 { part, .. } => part.start_pressure_measurement(),
            Self::Dps310 { part, .. } => part.start_pressure_measurement(),
        }
    }

    /// The last pressure measurement's raw value.
    pub fn read_pressure_raw(&mut self) -> Result<i32, BaroError> {
        match self {
            Self::Spl06 { part, .. } => part.read_pressure_raw(),
            Self::Dps310 { part, .. } => part.read_pressure_raw(),
        }
    }

    /// One measurement pair through the part's compensation, or `None`
    /// where the formula is not finite - or where a DPS310's first
    /// temperature after its probe's configure is still to be flushed,
    /// so one not-ready cycle passes before the part measures clean.
    /// Both probes configure 1x oversampling, and the compensation
    /// divides by its scale.
    pub fn compensate(&mut self, pressure_raw: i32, temperature_raw: i32) -> Option<BaroSample> {
        let (temperature_c, pressure_pa) = match self {
            Self::Spl06 { coefficients, .. } => {
                let oversampling = spl06::Oversampling::Times1;
                (
                    coefficients.temperature_celsius(temperature_raw, oversampling)?,
                    coefficients.pressure_pascals(
                        pressure_raw,
                        oversampling,
                        temperature_raw,
                        oversampling,
                    )?,
                )
            }
            Self::Dps310 {
                coefficients,
                flush_pending,
                ..
            } => {
                if *flush_pending {
                    *flush_pending = false;
                    return None;
                }
                let oversampling = dps310::Oversampling::Times1;
                (
                    coefficients.temperature_celsius(temperature_raw, oversampling)?,
                    coefficients.pressure_pascals(
                        pressure_raw,
                        oversampling,
                        temperature_raw,
                        oversampling,
                    )?,
                )
            }
        };
        Some(BaroSample {
            pressure_pa: pressure_pa.max(0.0) as u32,
            temperature_dc: temperature_c.clamp(i16::MIN as f32, i16::MAX as f32) as i16,
        })
    }
}

/// What a board's support hands the barometer task: the probe when its own
/// first probe found the part, the open bus and its part kind when it did
/// not, and nothing when the board has no barometer bus at all.
pub enum BaroHandover {
    /// The board's probe found the barometer; measure with it.
    Probe(BaroProbe),
    /// The board's probe found no barometer, but the bus is open: keep
    /// probing the board's kind, and sweep the bus for what does answer.
    Open { bus: BaroI2c, kind: PartKind },
    /// The board has no barometer bus: none is fitted, or its open was
    /// refused.
    Absent,
}

/// A probe that failed: the bus the part's driver handed back, the stage
/// the status line reports, and the chip ID it managed to read.
pub struct ProbeFailed {
    bus: BaroI2c,
    stage: u8,
    chip_id: u8,
}

impl ProbeFailed {
    /// How far the probe reached, one of the
    /// [`PROBE_`](observed::BaroStatus) stages.
    pub const fn stage(&self) -> u8 {
        self.stage
    }

    /// The chip ID the probe read, when it read one; 0 when it did not.
    pub const fn chip_id(&self) -> u8 {
        self.chip_id
    }

    /// The bus back, for the caller that keeps probing.
    pub fn release(self) -> BaroI2c {
        self.bus
    }
}

/// How a chip-ID read failed, as the stage the status line reports: a NACK
/// is no part at the address; every other error is the lines misbehaving.
fn chip_id_stage(error: BaroError) -> u8 {
    match error {
        BaroError::Nack => observed::BaroStatus::PROBE_NO_ANSWER,
        _ => observed::BaroStatus::PROBE_LINES,
    }
}

/// Probe the SPL06-007 at one address: its chip ID, its calibration page
/// and its oversampling, gathered into the probe the polling task
/// measures with. On any failure the bus comes back with the stage it
/// reached, for the caller that keeps probing.
pub fn probe_spl06(bus: BaroI2c, address: u8) -> Result<BaroProbe, ProbeFailed> {
    let mut part = Spl06::new(bus, address);
    let chip_id = match part.read_chip_id() {
        Ok(chip_id) => chip_id,
        Err(error) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: chip_id_stage(error),
                chip_id: 0,
            });
        }
    };
    if chip_id != spl06::CHIP_ID {
        return Err(ProbeFailed {
            bus: part.into_inner(),
            stage: observed::BaroStatus::PROBE_WRONG_CHIP_ID,
            chip_id,
        });
    }
    let coefficients = match part.read_coefficients() {
        Ok(coefficients) => coefficients,
        Err(_) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: observed::BaroStatus::PROBE_COEFFICIENTS,
                chip_id,
            });
        }
    };
    match part.configure(spl06::Oversampling::Times1, spl06::Oversampling::Times1) {
        Ok(()) => Ok(BaroProbe::Spl06 {
            part,
            coefficients,
            chip_id,
        }),
        Err(_) => Err(ProbeFailed {
            bus: part.into_inner(),
            stage: observed::BaroStatus::PROBE_CONFIGURE,
            chip_id,
        }),
    }
}

/// Probe the DPS310, or the DPS368 that shares its interface, at one
/// address: its chip ID, its boot readiness, its calibration page and
/// its oversampling, gathered into the probe the polling task measures
/// with. The part's first temperature after the configure is flushed,
/// as its driver documents, so the probe's measurement discards one
/// cycle before it publishes. On any failure the bus comes back with
/// the stage it reached, for the caller that keeps probing.
pub fn probe_dps310(bus: BaroI2c, address: u8) -> Result<BaroProbe, ProbeFailed> {
    let mut part = Dps310::new(bus, address);
    let chip_id = match part.read_chip_id() {
        Ok(chip_id) => chip_id,
        Err(error) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: chip_id_stage(error),
                chip_id: 0,
            });
        }
    };
    if chip_id != dps310::CHIP_ID {
        return Err(ProbeFailed {
            bus: part.into_inner(),
            stage: observed::BaroStatus::PROBE_WRONG_CHIP_ID,
            chip_id,
        });
    }
    // A part still booting holds its readiness bits clear: the same
    // stage as an unreadable page, re-probed until the part is up.
    let status = match part.read_status() {
        Ok(status) => status,
        Err(error) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: chip_id_stage(error),
                chip_id,
            });
        }
    };
    if !status.coefficients_ready || !status.sensor_ready {
        return Err(ProbeFailed {
            bus: part.into_inner(),
            stage: observed::BaroStatus::PROBE_COEFFICIENTS,
            chip_id,
        });
    }
    let temperature_external = match part.read_temperature_source() {
        Ok(temperature_external) => temperature_external,
        Err(_) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: observed::BaroStatus::PROBE_COEFFICIENTS,
                chip_id,
            });
        }
    };
    let coefficients = match part.read_coefficients() {
        Ok(coefficients) => coefficients,
        Err(_) => {
            return Err(ProbeFailed {
                bus: part.into_inner(),
                stage: observed::BaroStatus::PROBE_COEFFICIENTS,
                chip_id,
            });
        }
    };
    match part.configure(
        dps310::Oversampling::Times1,
        dps310::Oversampling::Times1,
        temperature_external,
    ) {
        Ok(()) => Ok(BaroProbe::Dps310 {
            part,
            coefficients,
            chip_id,
            flush_pending: true,
        }),
        Err(_) => Err(ProbeFailed {
            bus: part.into_inner(),
            stage: observed::BaroStatus::PROBE_CONFIGURE,
            chip_id,
        }),
    }
}

/// How long a commanded measurement takes at 1x oversampling, plus margin:
/// the part converts on its own clock, and the wait is a yield, not a hold.
const CONVERSION_WAIT_MS: u64 = 10;

/// One measurement cycle this often: a barometer's world moves slowly, and
/// the diagnostics reading it moves slower.
const BARO_POLL_MS: u64 = 500;

/// The absence sweep asks this many addresses per poll cycle: the whole
/// bus, one slice at a time, beside the strappings' probes.
const SWEEP_CHUNK: u8 = 8;

/// The sweep's first address: the first any I2C device may have.
const SWEEP_FIRST: u8 = 0x08;

/// The sweep's last address: the last any I2C device may have.
const SWEEP_LAST: u8 = 0x77;

/// The task's state: measuring the part it found, or probing for one.
enum Phase {
    /// The part answered; measure with it.
    Measuring(BaroProbe),
    /// The part has not answered: the open bus, the board's part kind,
    /// the sweep's next address, and how many addresses this sweep
    /// round has seen answer.
    Probing {
        bus: BaroI2c,
        kind: PartKind,
        next: u8,
        acks: u8,
    },
}

/// One probing cycle: the board's part is fully probed at its two
/// strappings, then a slice of the bus swept for what does answer, so
/// a part that answers is measured from the next cycle, and a bus that
/// misbehaves says which way. The sweep restarts its round when the
/// slice passes the last address.
fn probe_cycle(
    mut bus: BaroI2c,
    kind: PartKind,
    mut next: u8,
    mut acks: u8,
) -> (observed::BaroStatus, Phase) {
    // The strappings first: the part's own two addresses, fully probed, the
    // deepest failure of the two reported.
    let mut stage = observed::BaroStatus::PROBE_NO_ANSWER;
    let mut chip_id = 0;
    for address in [kind.address(), kind.other_address()] {
        match kind.probe(bus, address) {
            Ok(probe) => {
                return (
                    observed::BaroStatus {
                        chip_id: probe.chip_id(),
                        address: probe.address(),
                        part: kind.status_code(),
                        probe: observed::BaroStatus::PROBE_MEASURING,
                        ..observed::BaroStatus::default()
                    },
                    Phase::Measuring(probe),
                );
            }
            Err(failed) => {
                stage = stage.max(failed.stage());
                if failed.chip_id() != 0 {
                    chip_id = failed.chip_id();
                }
                bus = failed.release();
            }
        }
    }
    // Then a slice of the sweep: every address that answers the chip-ID
    // read is counted, so a dead bus is told from a quiet one. Both
    // drivers read the same ID register, so the kind's driver walks it.
    let mut slice = SWEEP_CHUNK;
    while slice > 0 && next <= SWEEP_LAST {
        let answered = match kind {
            PartKind::Spl06 => {
                let mut part = Spl06::new(bus, next);
                let answered = part.read_chip_id().is_ok();
                bus = part.into_inner();
                answered
            }
            PartKind::Dps310 => {
                let mut part = Dps310::new(bus, next);
                let answered = part.read_chip_id().is_ok();
                bus = part.into_inner();
                answered
            }
        };
        if answered {
            acks = acks.saturating_add(1);
        }
        next += 1;
        slice -= 1;
    }
    let status = observed::BaroStatus {
        chip_id,
        address: 0,
        probe: stage,
        scan_acks: acks,
        ..observed::BaroStatus::default()
    };
    if next > SWEEP_LAST {
        (
            status,
            Phase::Probing {
                bus,
                kind,
                next: SWEEP_FIRST,
                acks: 0,
            },
        )
    } else {
        (
            status,
            Phase::Probing {
                bus,
                kind,
                next,
                acks,
            },
        )
    }
}

/// The onboard barometer: take one temperature and one pressure measurement
/// a half-second apart, compensate them with the part's calibration page,
/// and publish a [`BaroStatus`]. A board whose probe found no barometer
/// keeps probing the part's two strappings and sweeping the bus for one,
/// reporting how far each attempt reached, so the diagnostics tell a part
/// that is not there from a bus that misbehaves; a board without a
/// barometer bus at all publishes absence and returns without touching
/// any bus.
#[ferroforge::task(
    local = [
        baro: BaroHandover,
        baro_status_link: LinkProducer<'static, observed::BaroStatus>,
    ],
    monotonic = Mono,
)]
pub async fn baro_polling(mut cx: baro_polling::Context) {
    let mut phase = match core::mem::replace(cx.local.baro, BaroHandover::Absent) {
        BaroHandover::Absent => {
            cx.local.baro_status_link.push(
                TimestampMicros(Mono::now().duration_since_epoch().to_micros()),
                observed::BaroStatus::default(),
            );
            return;
        }
        BaroHandover::Probe(probe) => Phase::Measuring(probe),
        BaroHandover::Open { bus, kind } => Phase::Probing {
            bus,
            kind,
            next: SWEEP_FIRST,
            acks: 0,
        },
    };
    loop {
        let status = match phase {
            Phase::Measuring(ref mut baro) => {
                // The part measures on command: one temperature, then one
                // pressure, each given its conversion time before its
                // registers are read. Any failure publishes not-ready and
                // is retried next cycle.
                let mut failed = baro.start_temperature().is_err();
                Mono::delay(CONVERSION_WAIT_MS.millis()).await;
                let temperature_raw = baro.read_temperature_raw();
                failed |= baro.start_pressure().is_err();
                Mono::delay(CONVERSION_WAIT_MS.millis()).await;
                let pressure_raw = baro.read_pressure_raw();

                let sample = match (temperature_raw, pressure_raw) {
                    (Ok(temperature_raw), Ok(pressure_raw)) if !failed => {
                        baro.compensate(pressure_raw, temperature_raw)
                    }
                    _ => None,
                };
                let (ready, pressure_pa, temperature_dc) = match sample {
                    Some(sample) => (true, sample.pressure_pa, sample.temperature_dc),
                    None => (false, 0, 0),
                };
                observed::BaroStatus {
                    chip_id: baro.chip_id(),
                    address: baro.address(),
                    part: baro.part().status_code(),
                    probe: observed::BaroStatus::PROBE_MEASURING,
                    scan_acks: 0,
                    ready,
                    pressure_pa,
                    temperature_dc,
                }
            }
            Phase::Probing {
                bus,
                kind,
                next,
                acks,
            } => {
                let (status, next_phase) = probe_cycle(bus, kind, next, acks);
                phase = next_phase;
                status
            }
        };
        cx.local.baro_status_link.push(
            TimestampMicros(Mono::now().duration_since_epoch().to_micros()),
            status,
        );

        Mono::delay(BARO_POLL_MS.millis()).await;
    }
}

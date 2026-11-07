use ferrowasp_boundary_contracts::messages as observed;
use ferrowasp_io_contracts::link::LinkProducer;
use ferrowasp_io_contracts::time::TimestampMicros;
use fugit::ExtU64 as _;
use spl06::{Coefficients, Oversampling, Spl06};

/// The SPL06-007 on a FerroHAL I2C bus.
pub type BaroBus = Spl06<crate::hal::i2c::I2c>;

/// The barometer's bus itself, as a board hands it over and the absence
/// sweep walks it.
pub type BaroI2c = crate::hal::i2c::I2c;

/// An I2C failure on the barometer's bus.
pub type BaroError = crate::hal::i2c::Error;

/// One compensated measurement pair.
pub struct BaroSample {
    /// Pascals.
    pub pressure_pa: u32,
    /// Centidegrees Celsius.
    pub temperature_dc: i16,
}

/// The board's barometer after its probe: the driver on its bus, the
/// calibration page the probe read, and the chip ID it answered with. The
/// board's support probes the part, configures its oversampling and moves
/// this in whole; this task only measures.
pub struct BaroProbe {
    spl06: BaroBus,
    coefficients: Coefficients,
    chip_id: u8,
    oversampling: Oversampling,
}

impl BaroProbe {
    /// From the board's probe: the bus, the calibration page, the chip ID
    /// and the oversampling the probe configured.
    pub const fn new(
        spl06: BaroBus,
        coefficients: Coefficients,
        chip_id: u8,
        oversampling: Oversampling,
    ) -> Self {
        Self {
            spl06,
            coefficients,
            chip_id,
            oversampling,
        }
    }

    /// The chip ID the probe read.
    pub const fn chip_id(&self) -> u8 {
        self.chip_id
    }

    /// The address the probe found the part at.
    pub const fn address(&self) -> u8 {
        self.spl06.address()
    }

    /// Command one temperature measurement; the part runs it on its own
    /// clock, and the caller gives it time before reading.
    pub fn start_temperature(&mut self) -> Result<(), BaroError> {
        self.spl06.start_temperature_measurement()
    }

    /// The last temperature measurement's raw value.
    pub fn read_temperature_raw(&mut self) -> Result<i32, BaroError> {
        self.spl06.read_temperature_raw()
    }

    /// Command one pressure measurement, as the temperature's.
    pub fn start_pressure(&mut self) -> Result<(), BaroError> {
        self.spl06.start_pressure_measurement()
    }

    /// The last pressure measurement's raw value.
    pub fn read_pressure_raw(&mut self) -> Result<i32, BaroError> {
        self.spl06.read_pressure_raw()
    }

    /// One measurement pair through the part's compensation, or `None`
    /// where the formula is not finite.
    pub fn compensate(&self, pressure_raw: i32, temperature_raw: i32) -> Option<BaroSample> {
        let temperature_c = self
            .coefficients
            .temperature_celsius(temperature_raw, self.oversampling)?;
        let pressure_pa = self.coefficients.pressure_pascals(
            pressure_raw,
            self.oversampling,
            temperature_raw,
            self.oversampling,
        )?;
        Some(BaroSample {
            pressure_pa: pressure_pa.max(0.0) as u32,
            temperature_dc: temperature_c.clamp(i16::MIN as f32, i16::MAX as f32) as i16,
        })
    }
}

/// What a board's support hands the barometer task: the probe when its own
/// first probe found the part, the open bus when it did not, and nothing
/// when the board has no barometer bus at all.
pub enum BaroHandover {
    /// The board's probe found the barometer; measure with it.
    Probe(BaroProbe),
    /// The board's probe found no barometer, but the bus is open: keep
    /// probing, and sweep the bus for what does answer.
    Open(BaroI2c),
    /// The board has no barometer bus: none is fitted, or its open was
    /// refused.
    Absent,
}

/// A probe that failed: the part's driver still on its bus, the stage the
/// status line reports, and the chip ID it managed to read.
pub struct ProbeFailed {
    spl06: BaroBus,
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
        self.spl06.into_inner()
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

/// Probe the SPL06-007 at its driver's address: its chip ID, its
/// calibration page and its oversampling, gathered into the probe the
/// polling task measures with. On any failure the part comes back on its
/// bus with the stage it reached, for the caller that keeps probing.
pub fn probe_spl06(mut spl06: BaroBus) -> Result<BaroProbe, ProbeFailed> {
    let chip_id = match spl06.read_chip_id() {
        Ok(chip_id) => chip_id,
        Err(error) => {
            return Err(ProbeFailed {
                spl06,
                stage: chip_id_stage(error),
                chip_id: 0,
            });
        }
    };
    if chip_id != spl06::CHIP_ID {
        return Err(ProbeFailed {
            spl06,
            stage: observed::BaroStatus::PROBE_WRONG_CHIP_ID,
            chip_id,
        });
    }
    let coefficients = match spl06.read_coefficients() {
        Ok(coefficients) => coefficients,
        Err(_) => {
            return Err(ProbeFailed {
                spl06,
                stage: observed::BaroStatus::PROBE_COEFFICIENTS,
                chip_id,
            });
        }
    };
    match spl06.configure(spl06::Oversampling::Times1, spl06::Oversampling::Times1) {
        Ok(()) => Ok(BaroProbe::new(
            spl06,
            coefficients,
            chip_id,
            spl06::Oversampling::Times1,
        )),
        Err(_) => Err(ProbeFailed {
            spl06,
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

/// The part's other strapping: the SPL06-007 straps its address to
/// [`spl06::ADDRESS`] or this, by its address pin.
const STRAPPING_OTHER: u8 = spl06::ADDRESS | 0x01;

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
    /// The part has not answered: the open bus, the sweep's next address,
    /// and how many addresses this sweep round has seen answer.
    Probing {
        bus: BaroI2c,
        next: u8,
        acks: u8,
    },
}

/// One probing cycle: the part's two strappings are fully probed, then a
/// slice of the bus swept for what does answer, so a part that answers is
/// measured from the next cycle, and a bus that misbehaves says which way.
/// The sweep restarts its round when the slice passes the last address.
fn probe_cycle(mut bus: BaroI2c, mut next: u8, mut acks: u8) -> (observed::BaroStatus, Phase) {
    // The strappings first: the part's own two addresses, fully probed, the
    // deepest failure of the two reported.
    let mut stage = observed::BaroStatus::PROBE_NO_ANSWER;
    let mut chip_id = 0;
    for address in [spl06::ADDRESS, STRAPPING_OTHER] {
        match probe_spl06(Spl06::new(bus, address)) {
            Ok(probe) => {
                return (
                    observed::BaroStatus {
                        chip_id: probe.chip_id(),
                        address: probe.address(),
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
    // read is counted, so a dead bus is told from a quiet one.
    let mut slice = SWEEP_CHUNK;
    while slice > 0 && next <= SWEEP_LAST {
        let mut part = Spl06::new(bus, next);
        if part.read_chip_id().is_ok() {
            acks = acks.saturating_add(1);
        }
        bus = part.into_inner();
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
        (status, Phase::Probing {
            bus,
            next: SWEEP_FIRST,
            acks: 0,
        })
    } else {
        (status, Phase::Probing { bus, next, acks })
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
        BaroHandover::Open(bus) => Phase::Probing {
            bus,
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
                    probe: observed::BaroStatus::PROBE_MEASURING,
                    scan_acks: 0,
                    ready,
                    pressure_pa,
                    temperature_dc,
                }
            }
            Phase::Probing { bus, next, acks } => {
                let (status, next_phase) = probe_cycle(bus, next, acks);
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

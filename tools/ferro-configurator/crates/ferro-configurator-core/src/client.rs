use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{
    config::{ConfigKey, FerroConfig},
    error::{FerroError, Result},
    transport::LineTransport,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlashInfo {
    pub jedec_id: String,
    pub capacity_bytes: u32,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusSnapshot {
    pub uptime_ms: u32,
    pub imu: String,
    pub imu_ready: bool,
    pub rc_valid: bool,
    pub armable: bool,
    pub throttle: u32,
    pub arm_switch: bool,
    pub armed: bool,
    pub battery_decivolts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogInfo {
    pub used_pages: u32,
    pub next_flight: u32,
    pub total_pages: u32,
    pub writable: bool,
}

pub struct FerroClient<T: LineTransport> {
    transport: T,
    timeout: Duration,
    last_status: Option<StatusSnapshot>,
}

impl<T: LineTransport> FerroClient<T> {
    pub fn new(transport: T, timeout: Duration) -> Self {
        Self {
            transport,
            timeout,
            last_status: None,
        }
    }

    pub fn description(&self) -> &str {
        self.transport.description()
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    pub fn flash_info(&mut self) -> Result<FlashInfo> {
        let response = self.request("flash info", 2)?;
        parse_flash_info(&response).ok_or_else(|| FerroError::UnexpectedResponse {
            operation: "flash info".to_owned(),
            response,
        })
    }

    pub fn read_status(&mut self) -> Result<StatusSnapshot> {
        if let Some(status) = self.last_status.clone() {
            return Ok(status);
        }
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Some(line) = self
                .transport
                .read_line(deadline.saturating_duration_since(Instant::now()))?
                && let Some(status) = parse_status(&line)
            {
                self.last_status = Some(status.clone());
                return Ok(status);
            }
        }
        Err(FerroError::Timeout {
            operation: "status telemetry".to_owned(),
            attempts: 1,
        })
    }

    pub fn read_config(&mut self) -> Result<FerroConfig> {
        let mut config = FerroConfig::default();
        for key in ConfigKey::ALL {
            let operation = format!("config get {}", key.name());
            let response = self.request(&operation, 2)?;
            let expected = format!("OK {}=", key.name());
            let value = response
                .strip_prefix(&expected)
                .ok_or_else(|| FerroError::UnexpectedResponse {
                    operation: operation.clone(),
                    response: response.clone(),
                })?
                .parse::<f32>()
                .map_err(|_| FerroError::UnexpectedResponse {
                    operation: operation.clone(),
                    response: response.clone(),
                })?;
            config.set(key, value)?;
        }
        config.ensure_valid()?;
        Ok(config)
    }

    pub fn log_info(&mut self) -> Result<LogInfo> {
        let response = self.request("logs list", 2)?;
        parse_log_info(&response).ok_or_else(|| FerroError::UnexpectedResponse {
            operation: "logs list".to_owned(),
            response,
        })
    }

    pub fn read_log_page(&mut self, page_index: u32) -> Result<[u8; 256]> {
        let operation = format!("logs read-page {page_index}");
        self.transport.write_line(&operation)?;
        let deadline = Instant::now() + self.timeout;
        let mut page = [0u8; 256];
        let mut seen = 0u16;

        while seen.count_ones() < 16 && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(line) = self.transport.read_line(remaining)? else {
                break;
            };
            let (line, status) = split_status_suffix(&line);
            if let Some(status) = status {
                self.last_status = Some(status);
            }
            if line.is_empty() {
                continue;
            }
            if let Some(status) = parse_status(line) {
                self.last_status = Some(status);
                continue;
            }
            if line.starts_with("ERR ") {
                return self
                    .accept_response(&operation, line.to_owned())
                    .map(|_| page);
            }
            let Some((response_page, offset, chunk)) = parse_page_chunk(line) else {
                continue;
            };
            if response_page != page_index {
                return Err(FerroError::UnexpectedResponse {
                    operation,
                    response: line.to_owned(),
                });
            }
            page[offset..offset + chunk.len()].copy_from_slice(&chunk);
            seen |= 1 << (offset / 16);
        }

        if seen.count_ones() == 16 {
            Ok(page)
        } else {
            Err(FerroError::Timeout {
                operation: format!("{operation} ({}/16 chunks received)", seen.count_ones()),
                attempts: 1,
            })
        }
    }

    pub fn erase_logs(&mut self) -> Result<()> {
        self.erase_logs_with_progress(|_| {})
    }

    pub fn erase_logs_with_progress(&mut self, mut progress: impl FnMut(Duration)) -> Result<()> {
        let response = self.request("logs erase CONFIRM", 1)?;
        if response == "OK logs erased" {
            return Ok(());
        }
        if response != "OK log erase started" {
            return Err(FerroError::UnexpectedResponse {
                operation: "logs erase CONFIRM".to_owned(),
                response,
            });
        }
        let started = Instant::now();
        let deadline = started + Duration::from_secs(600);
        let update_interval = Duration::from_secs(10);
        let mut next_update = update_interval;
        progress(Duration::ZERO);

        while Instant::now() < deadline {
            let elapsed = started.elapsed();
            if elapsed >= next_update {
                progress(elapsed);
                next_update = next_update.saturating_add(update_interval);
                continue;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let until_update = next_update.saturating_sub(elapsed);
            let Some(line) = self.transport.read_line(remaining.min(until_update))? else {
                continue;
            };
            let (line, status) = split_status_suffix(&line);
            if let Some(status) = status {
                self.last_status = Some(status);
            }
            if line.is_empty() {
                continue;
            }
            if let Some(status) = parse_status(line) {
                self.last_status = Some(status);
                continue;
            }
            if line == "OK logs erased" || line.starts_with("ERR ") {
                return self
                    .accept_response("logs erase CONFIRM", line.to_owned())
                    .map(|_| ());
            }
        }
        Err(FerroError::Timeout {
            operation: "log erase completion".to_owned(),
            attempts: 1,
        })
    }

    /// Stage every whitelisted setting. The current firmware validates each
    /// value and keeps it in RAM until `save_config` is called.
    pub fn stage_config(&mut self, config: &FerroConfig) -> Result<()> {
        config.ensure_complete()?;

        // A maximum-rate reduction can be invalid until its center rate has
        // also moved, while a center-rate increase can require the opposite
        // order. Stage a bounded temporary maximum first. Staged values are
        // not applied or persisted until the final explicit save.
        for key in [
            ConfigKey::RollMaxRate,
            ConfigKey::PitchMaxRate,
            ConfigKey::YawMaxRate,
        ] {
            self.stage_value(key, key.value_spec().maximum)?;
        }
        for key in [
            ConfigKey::RollCenterRate,
            ConfigKey::PitchCenterRate,
            ConfigKey::YawCenterRate,
        ] {
            self.stage_value(key, complete_value(config, key)?)?;
        }
        for key in [
            ConfigKey::RollMaxRate,
            ConfigKey::PitchMaxRate,
            ConfigKey::YawMaxRate,
        ] {
            self.stage_value(key, complete_value(config, key)?)?;
        }
        for key in ConfigKey::ALL.into_iter().filter(|key| {
            !matches!(
                key,
                ConfigKey::RollCenterRate
                    | ConfigKey::RollMaxRate
                    | ConfigKey::PitchCenterRate
                    | ConfigKey::PitchMaxRate
                    | ConfigKey::YawCenterRate
                    | ConfigKey::YawMaxRate
            )
        }) {
            self.stage_value(key, complete_value(config, key)?)?;
        }
        Ok(())
    }

    fn stage_value(&mut self, key: ConfigKey, value: f32) -> Result<()> {
        let operation = if key.value_spec().integer {
            format!("config set {} {}", key.name(), value as u32)
        } else {
            // Firmware's readback contract is four decimals, so send the same
            // canonical precision to make verification meaningful.
            format!("config set {} {value:.4}", key.name())
        };
        let response = self.request(&operation, 1)?;
        if response != "OK staged; use config save" {
            return Err(FerroError::UnexpectedResponse {
                operation,
                response,
            });
        }
        Ok(())
    }

    pub fn save_config(&mut self) -> Result<()> {
        let response = self.request("config save", 1)?;
        if response == "OK config saved" {
            return Ok(());
        }
        if response != "OK config save started" {
            return Err(FerroError::UnexpectedResponse {
                operation: "config save".to_owned(),
                response,
            });
        }
        self.wait_for_response("config save completion", Duration::from_secs(10), |line| {
            line == "OK config saved" || line.starts_with("ERR ")
        })
        .and_then(|line| self.accept_response("config save", line))
        .map(|_| ())
    }

    pub fn apply_config(&mut self, config: &FerroConfig) -> Result<FerroConfig> {
        let original = self.read_config()?;
        let desired = config.merged_over(&original)?;
        if let Err(error) = self.stage_config(&desired) {
            // Current firmware stages keys individually. Best-effort restore
            // prevents an interrupted, unpersisted stage from leaving a
            // surprising RAM value. Persistence is never attempted here.
            let _ = self.stage_config(&original);
            return Err(error);
        }
        self.save_config()?;
        let readback = self.read_config()?;
        let differences = desired.differences(&readback);
        if differences.is_empty() {
            Ok(readback)
        } else {
            Err(FerroError::VerificationFailed {
                details: differences.join("; "),
            })
        }
    }

    fn request(&mut self, command: &str, attempts: u8) -> Result<String> {
        for attempt in 1..=attempts {
            self.transport.write_line(command)?;
            match self.wait_for_response(command, self.timeout, |line| {
                line.starts_with("OK ") || line.starts_with("ERR ")
            }) {
                Ok(line) => return self.accept_response(command, line),
                Err(FerroError::Timeout { .. }) if attempt < attempts => continue,
                Err(error) => return Err(error),
            }
        }
        Err(FerroError::Timeout {
            operation: command.to_owned(),
            attempts,
        })
    }

    fn wait_for_response(
        &mut self,
        operation: &str,
        timeout: Duration,
        accept: impl Fn(&str) -> bool,
    ) -> Result<String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(line) = self.transport.read_line(remaining)? else {
                break;
            };
            let (line, status) = split_status_suffix(&line);
            if let Some(status) = status {
                self.last_status = Some(status);
            }
            if line.is_empty() {
                continue;
            }
            if let Some(status) = parse_status(line) {
                self.last_status = Some(status);
                continue;
            }
            if accept(line) {
                return Ok(line.to_owned());
            }
        }
        Err(FerroError::Timeout {
            operation: operation.to_owned(),
            attempts: 1,
        })
    }

    fn accept_response(&self, operation: &str, line: String) -> Result<String> {
        if let Some(message) = line.strip_prefix("ERR ") {
            Err(FerroError::DeviceRejected {
                operation: operation.to_owned(),
                message: message.to_owned(),
            })
        } else {
            Ok(line)
        }
    }
}

fn complete_value(config: &FerroConfig, key: ConfigKey) -> Result<f32> {
    config.get(key).ok_or_else(|| {
        FerroError::InvalidConfiguration(format!(
            "{} is absent from a configuration being staged",
            key.name()
        ))
    })
}

fn parse_flash_info(line: &str) -> Option<FlashInfo> {
    let fields = parse_fields(line.strip_prefix("OK ")?);
    Some(FlashInfo {
        jedec_id: fields.iter().find(|(key, _)| *key == "jedec")?.1.to_owned(),
        capacity_bytes: fields
            .iter()
            .find(|(key, _)| *key == "bytes")?
            .1
            .parse()
            .ok()?,
        ready: fields.iter().find(|(key, _)| *key == "ready")?.1 == "1",
    })
}

fn parse_log_info(line: &str) -> Option<LogInfo> {
    let fields = parse_fields(line.strip_prefix("OK ")?);
    let get = |long: &str, short: &str| {
        fields
            .iter()
            .find(|(name, _)| *name == long || *name == short)
            .map(|(_, value)| *value)
    };
    Some(LogInfo {
        used_pages: get("used_pages", "u")?.parse().ok()?,
        next_flight: get("next_flight", "n")?.parse().ok()?,
        total_pages: get("total_pages", "t")?.parse().ok()?,
        writable: match get("writable", "w")? {
            "0" => false,
            "1" => true,
            _ => return None,
        },
    })
}

fn parse_page_chunk(line: &str) -> Option<(u32, usize, [u8; 16])> {
    let mut fields = line.split_ascii_whitespace();
    if fields.next()? != "PAGE" {
        return None;
    }
    let page = fields.next()?.parse().ok()?;
    let offset: usize = fields.next()?.parse().ok()?;
    let encoded = fields.next()?;
    if fields.next().is_some()
        || !offset.is_multiple_of(16)
        || offset + 16 > 256
        || encoded.len() != 32
    {
        return None;
    }
    let mut chunk = [0u8; 16];
    for (index, output) in chunk.iter_mut().enumerate() {
        let start = index * 2;
        *output = u8::from_str_radix(&encoded[start..start + 2], 16).ok()?;
    }
    Some((page, offset, chunk))
}

fn parse_status(line: &str) -> Option<StatusSnapshot> {
    let fields = parse_fields(line.strip_prefix("FWDBG1 ")?);
    let get = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    let boolean = |key: &str| {
        get(key).and_then(|value| match value {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        })
    };
    Some(StatusSnapshot {
        uptime_ms: get("ms")?.parse().ok()?,
        imu: get("imu")?.to_owned(),
        imu_ready: boolean("ready")?,
        rc_valid: boolean("rc")?,
        armable: boolean("armable")?,
        throttle: get("thr")?.parse().ok()?,
        arm_switch: boolean("arm_sw")?,
        armed: boolean("armed")?,
        battery_decivolts: get("vbat_dV")?.parse().ok()?,
    })
}

fn split_status_suffix(line: &str) -> (&str, Option<StatusSnapshot>) {
    if let Some(index) = line.find("FWDBG1 ")
        && let Some(status) = parse_status(&line[index..])
    {
        return (line[..index].trim_end_matches(['\r', '\n']), Some(status));
    }
    (line, None)
}

fn parse_fields(line: &str) -> Vec<(&str, &str)> {
    line.split_ascii_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn config_responses() -> Vec<String> {
        FerroConfig::default().pipe(|config| {
            ConfigKey::ALL
                .into_iter()
                .map(|key| {
                    format!(
                        "OK {}={:.4}",
                        key.name(),
                        config.get(key).expect("default config is complete")
                    )
                })
                .collect()
        })
    }

    const STAGE_COMMAND_COUNT: usize = 24;

    trait Pipe: Sized {
        fn pipe<R>(self, function: impl FnOnce(Self) -> R) -> R {
            function(self)
        }
    }
    impl<T> Pipe for T {}

    #[test]
    fn parses_current_flash_info() {
        let info = parse_flash_info("OK jedec=ef:40:18 bytes=16777216 ready=1").unwrap();
        assert_eq!(info.capacity_bytes, 16 * 1024 * 1024);
        assert!(info.ready);
    }

    #[test]
    fn parses_current_log_info() {
        let info =
            parse_log_info("OK used_pages=551 next_flight=2 total_pages=65488 writable=1").unwrap();
        assert_eq!(info.used_pages, 551);
        assert_eq!(info.next_flight, 2);
        assert_eq!(info.total_pages, 65_488);
        assert!(info.writable);
    }

    #[test]
    fn parses_bounded_log_info_response() {
        let info = parse_log_info("OK u=55316 n=38 t=65488 w=1").unwrap();

        assert_eq!(info.used_pages, 55_316);
        assert_eq!(info.next_flight, 38);
        assert_eq!(info.total_pages, 65_488);
        assert!(info.writable);
    }

    #[test]
    fn recovers_log_info_concatenated_with_status_from_legacy_firmware() {
        let mock = MockTransport::with_lines([
            "OK used_pages=55316 next_flight=38 total_pages=65488 writable=1FWDBG1 ms=44032 imu=icm42688p ready=1 seq=44550 gyro=0,9,-6 stale=0 ctl=17613 rc=0 armable=0 thr=0 arm_sw=0 armed=0 vbat_dV=0 current_cA=0 adc_v_mV=2 adc_i_mV=7",
        ]);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));

        let info = client.log_info().unwrap();
        assert_eq!(info.used_pages, 55_316);
        assert_eq!(info.next_flight, 38);
        assert_eq!(info.total_pages, 65_488);
        assert!(info.writable);
        assert_eq!(client.read_status().unwrap().uptime_ms, 44_032);
    }

    #[test]
    fn assembles_one_page_from_bounded_hex_chunks() {
        let lines = (0..16).map(|chunk| {
            let offset = chunk * 16;
            let bytes = (0..16)
                .map(|index| format!("{:02x}", offset + index))
                .collect::<String>();
            format!("PAGE 7 {offset:03} {bytes}")
        });
        let mock = MockTransport::with_lines(lines);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        let page = client.read_log_page(7).unwrap();
        assert_eq!(page[0], 0);
        assert_eq!(page[255], 255);
    }

    #[test]
    fn telemetry_is_ignored_while_waiting_for_command_response() {
        let mock = MockTransport::with_lines([
            "FerroWasp Foxeer F405 V2 storage CLI v1; type help",
            "FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9 gyro=0,0,0 stale=0 ctl=4 rc=1 armable=1 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=0 adc_v_mV=0 adc_i_mV=0",
            "OK jedec=ef:40:18 bytes=16777216 ready=1",
        ]);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        assert_eq!(client.flash_info().unwrap().jedec_id, "ef:40:18");
        assert!(!client.read_status().unwrap().armed);
    }

    #[test]
    fn reads_every_whitelisted_config_key() {
        let mock = MockTransport::with_lines(config_responses());
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        assert_eq!(client.read_config().unwrap(), FerroConfig::default());
        assert_eq!(client.into_transport().writes.len(), ConfigKey::ALL.len());
    }

    #[test]
    fn apply_stages_saves_and_verifies() {
        let staged = (0..STAGE_COMMAND_COUNT).map(|_| "OK staged; use config save".to_owned());
        let responses = config_responses()
            .into_iter()
            .chain(staged)
            .chain([
                "OK config save started".to_owned(),
                "OK config saved".to_owned(),
            ])
            .chain(config_responses());
        let mock = MockTransport::with_lines(responses);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        assert_eq!(
            client.apply_config(&FerroConfig::default()).unwrap(),
            FerroConfig::default()
        );
        let mock = client.into_transport();
        assert_eq!(mock.writes[0], "config get roll_p");
        assert!(mock.writes.contains(&"config set roll_p 2.5000".to_owned()));
        assert!(mock.writes.contains(&"config save".to_owned()));
    }

    #[test]
    fn failed_partial_stage_attempts_to_restore_snapshot_without_saving() {
        let rollback = (0..STAGE_COMMAND_COUNT).map(|_| "OK staged; use config save".to_owned());
        let responses = config_responses()
            .into_iter()
            .chain((0..9).map(|_| "OK staged; use config save".to_owned()))
            .chain(["ERR simulated interruption".to_owned()])
            .chain(rollback);
        let mock = MockTransport::with_lines(responses);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        let mut desired = FerroConfig::default();
        desired.roll.p = 0.4;

        assert!(client.apply_config(&desired).is_err());
        let mock = client.into_transport();
        assert!(mock.writes.contains(&"config set roll_p 0.4000".to_owned()));
        assert_eq!(mock.writes.last().unwrap(), "config set yaw_expo 0.5000");
        assert!(!mock.writes.contains(&"config save".to_owned()));
    }

    #[test]
    fn firmware_rejection_is_preserved() {
        let mock = MockTransport::with_lines(["ERR config changes disabled while armed"]);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        let error = client.stage_config(&FerroConfig::default()).unwrap_err();
        assert!(error.to_string().contains("while armed"));
    }

    #[test]
    fn erase_reports_start_and_waits_for_verified_completion() {
        let mock = MockTransport::with_lines([
            "OK log erase started",
            "FWDBG1 ms=12345 imu=icm42688p ready=1 seq=9 gyro=0,0,0 stale=0 ctl=4 rc=0 armable=0 thr=1000 arm_sw=0 armed=0 vbat_dV=230 current_cA=0 adc_v_mV=0 adc_i_mV=0",
            "OK logs erased",
        ]);
        let mut client = FerroClient::new(mock, Duration::from_millis(20));
        let mut progress = Vec::new();

        client
            .erase_logs_with_progress(|elapsed| progress.push(elapsed))
            .unwrap();

        assert_eq!(progress, vec![Duration::ZERO]);
        assert_eq!(client.into_transport().writes, vec!["logs erase CONFIRM"]);
    }
}

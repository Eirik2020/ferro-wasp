//! The storage owner's answers to MSPv2 configurator requests: configuration
//! staging, commit and reset, and blackbox listing and reads.
//!
//! MSPv2 is an opt-in feature, not expanded without approval; this is the
//! same handling the text command line has, over its own wire format. A
//! commit or reset starts a [`ConfigSave`] and answers when it completes,
//! through [`ConfigRpcCompletion`].

use ferrowasp_mspv2::rpc;
use ferrowasp_storage::nor_storage::NorStorage;

use crate::blackbox_storage as blackbox_task;
use crate::commands::StoredConfigRpc;
use ferrowasp_blackbox::log_writer::LogWriter;
use ferrowasp_blackbox::storage_maintenance::Maintenance;
use ferrowasp_config::stored::StoredConfig;
use ferrowasp_storage::config_save::{ConfigRpcCompletion, ConfigSave};
use ferrowasp_storage::layout::StorageLayout;

use ferrowasp_mspv2 as mspv2;

/// What a request reads and may change.
pub struct RpcContext<'a, F: NorStorage> {
    pub flash: &'a mut F,
    pub layout: StorageLayout,
    pub armed: bool,
    pub log: &'a LogWriter,
    pub maintenance: &'a Maintenance,
    pub config_save: &'a mut ConfigSave,
    /// An accelerometer calibration is sampling.
    pub calibrating: bool,
    pub stored_config: &'a StoredConfig,
    /// A configuration staged by an earlier request, awaiting commit.
    pub staged: &'a mut Option<StoredConfig>,
    /// The request a running save answers when it completes.
    pub completion: &'a mut Option<ConfigRpcCompletion>,
    pub config_sequence: u32,
    pub config_active_slot: u8,
    /// The board's hello.
    pub device_info: fn() -> rpc::DeviceInfo,
    pub default_config: StoredConfig,
}

fn blackbox_info<F: NorStorage>(
    flash: &mut F,
    layout: StorageLayout,
    used_pages: u32,
    id: rpc::BlackboxId,
    active_id: Option<u32>,
) -> Result<Option<rpc::BlackboxInfo>, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::blackbox_info(&mut read, layout, used_pages, id, active_id)
}

fn list_blackboxes<F: NorStorage>(
    flash: &mut F,
    layout: StorageLayout,
    used_pages: u32,
    active_id: Option<u32>,
) -> Result<rpc::BlackboxList, ()> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::list_blackboxes(&mut read, layout, used_pages, active_id)
}

fn read_blackbox_chunk<F: NorStorage>(
    flash: &mut F,
    layout: StorageLayout,
    used_pages: u32,
    id: rpc::BlackboxId,
    offset: u32,
    requested_length: u16,
) -> Result<rpc::BlackboxChunk, rpc::DeviceError> {
    let mut read = |address, output: &mut [u8]| flash.read(address, output).map_err(|_| ());
    blackbox_task::read_blackbox_chunk(&mut read, layout, used_pages, id, offset, requested_length)
}

/// Answers one request; `None` while a save it started is still running.
pub fn handle<F: NorStorage>(
    request: rpc::RpcRequest,
    ctx: &mut RpcContext<'_, F>,
) -> Option<rpc::RpcResponse> {
    let request_id = request.request_id;
    let maintenance_busy =
        ctx.maintenance.is_active() || ctx.config_save.is_active() || ctx.calibrating;
    let active_blackbox_id = ctx
        .log
        .recording()
        .then(|| ctx.log.next_flight().wrapping_sub(1).max(1));
    match request.operation {
        mspv2::rpc::Request::Hello => Some(mspv2::rpc::ok(
            request_id,
            mspv2::rpc::Response::Hello((ctx.device_info)()),
        )),
        mspv2::rpc::Request::GetConfig => {
            let crc = ctx.stored_config.crc32();
            Some(mspv2::rpc::ok(
                request_id,
                mspv2::rpc::Response::Config(mspv2::rpc::ConfigSnapshot {
                    schema_version: mspv2::rpc::CONFIG_SCHEMA_VERSION,
                    active_crc32: crc,
                    persisted_crc32: crc,
                    config: ctx.stored_config.to_rpc(),
                }),
            ))
        }
        mspv2::rpc::Request::StageConfig(config) => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else if maintenance_busy {
                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
            } else {
                match ctx.stored_config.apply_rpc(config) {
                    Ok(candidate) => {
                        let crc = candidate.crc32();
                        *ctx.staged = Some(candidate);
                        Some(mspv2::rpc::ok(
                            request_id,
                            mspv2::rpc::Response::ConfigStaged {
                                staged_crc32: crc,
                                reboot_required: false,
                            },
                        ))
                    }
                    Err(field) => Some(mspv2::rpc::RpcResponse {
                        protocol_version: mspv2::rpc::FWSP_RPC_VERSION,
                        request_id,
                        result: mspv2::rpc::RpcResult::Err(mspv2::rpc::ErrorDetail {
                            code: mspv2::rpc::DeviceError::InvalidConfiguration,
                            field: Some(field),
                            argument: None,
                        }),
                    }),
                }
            }
        }
        mspv2::rpc::Request::CommitConfig {
            expected_staged_crc32,
        } => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else if maintenance_busy {
                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
            } else if let Some(candidate) = *ctx.staged {
                if candidate.crc32() != expected_staged_crc32 {
                    Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::ConfigurationConflict,
                    ))
                } else {
                    match ctx.config_save.begin(
                        candidate,
                        ctx.config_sequence,
                        ctx.config_active_slot,
                    ) {
                        Ok(()) => {
                            *ctx.completion = Some(ConfigRpcCompletion::Commit(request_id));
                            *ctx.staged = None;
                            None
                        }
                        Err(_) => Some(mspv2::rpc::error(
                            request_id,
                            mspv2::rpc::DeviceError::Internal,
                        )),
                    }
                }
            } else {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::ConfigurationConflict,
                ))
            }
        }
        mspv2::rpc::Request::ResetConfigToDefaults => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else if maintenance_busy {
                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
            } else {
                match ctx.config_save.begin(
                    ctx.default_config,
                    ctx.config_sequence,
                    ctx.config_active_slot,
                ) {
                    Ok(()) => {
                        *ctx.completion = Some(ConfigRpcCompletion::Defaults(request_id));
                        *ctx.staged = None;
                        None
                    }
                    Err(_) => Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::Internal,
                    )),
                }
            }
        }
        mspv2::rpc::Request::ListBlackboxes => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else {
                match list_blackboxes(
                    ctx.flash,
                    ctx.layout,
                    ctx.log.next_page(),
                    active_blackbox_id,
                ) {
                    Ok(list) => Some(mspv2::rpc::ok(
                        request_id,
                        mspv2::rpc::Response::BlackboxList(list),
                    )),
                    Err(_) => Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::ReadFailure,
                    )),
                }
            }
        }
        mspv2::rpc::Request::GetBlackboxInfo { id } => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else {
                match blackbox_info(
                    ctx.flash,
                    ctx.layout,
                    ctx.log.next_page(),
                    id,
                    active_blackbox_id,
                ) {
                    Ok(Some(info)) => Some(mspv2::rpc::ok(
                        request_id,
                        mspv2::rpc::Response::BlackboxInfo(info),
                    )),
                    Ok(None) => Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::BlackboxNotFound,
                    )),
                    Err(_) => Some(mspv2::rpc::error(
                        request_id,
                        mspv2::rpc::DeviceError::ReadFailure,
                    )),
                }
            }
        }
        mspv2::rpc::Request::ReadBlackboxChunk {
            id,
            offset,
            requested_length,
        } => {
            if ctx.armed {
                Some(mspv2::rpc::error(
                    request_id,
                    mspv2::rpc::DeviceError::Armed,
                ))
            } else if active_blackbox_id == Some(id.0) {
                Some(mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy))
            } else {
                match read_blackbox_chunk(
                    ctx.flash,
                    ctx.layout,
                    ctx.log.next_page(),
                    id,
                    offset,
                    requested_length,
                ) {
                    Ok(chunk) => Some(mspv2::rpc::ok(
                        request_id,
                        mspv2::rpc::Response::BlackboxChunk(chunk),
                    )),
                    Err(error) => Some(mspv2::rpc::error(request_id, error)),
                }
            }
        }
        mspv2::rpc::Request::EraseBlackbox { .. } | mspv2::rpc::Request::Reboot { .. } => Some(
            mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::UnsupportedOperation),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrowasp_storage::test_nor::MemoryNor;

    const CAPACITY: u32 = 64 * 1024;

    fn hello() -> rpc::DeviceInfo {
        unreachable!("no test says hello")
    }

    /// The storage owner's MSPv2 state.
    struct Owner {
        nor: MemoryNor,
        log: LogWriter,
        maintenance: Maintenance,
        config_save: ConfigSave,
        stored_config: StoredConfig,
        staged: Option<StoredConfig>,
        completion: Option<ConfigRpcCompletion>,
        armed: bool,
    }

    impl Owner {
        fn new() -> Self {
            let mut log = LogWriter::new();
            log.recover(0, 1, true);
            Self {
                nor: MemoryNor::erased(CAPACITY),
                log,
                maintenance: Maintenance::Idle,
                config_save: ConfigSave::idle(),
                stored_config: StoredConfig::first_hop_default(),
                staged: None,
                completion: None,
                armed: false,
            }
        }

        fn send(&mut self, request_id: u16, operation: rpc::Request) -> Option<rpc::RpcResponse> {
            handle(
                rpc::RpcRequest {
                    protocol_version: rpc::FWSP_RPC_VERSION,
                    request_id,
                    operation,
                },
                &mut RpcContext {
                    flash: &mut self.nor,
                    layout: StorageLayout::new(CAPACITY).unwrap(),
                    armed: self.armed,
                    log: &self.log,
                    maintenance: &self.maintenance,
                    config_save: &mut self.config_save,
                    calibrating: false,
                    stored_config: &self.stored_config,
                    staged: &mut self.staged,
                    completion: &mut self.completion,
                    config_sequence: 0,
                    config_active_slot: 1,
                    device_info: hello,
                    default_config: StoredConfig::first_hop_default(),
                },
            )
        }
    }

    fn error_code(response: Option<rpc::RpcResponse>) -> rpc::DeviceError {
        match response.expect("an answer").result {
            rpc::RpcResult::Err(detail) => detail.code,
            rpc::RpcResult::Ok(response) => panic!("expected an error, got {response:?}"),
        }
    }

    fn tuned() -> rpc::ConfigV1 {
        let mut config = StoredConfig::first_hop_default().to_rpc();
        config.roll_p = 3.0;
        config
    }

    #[test]
    fn a_staged_configuration_commits_only_with_its_own_crc() {
        let mut owner = Owner::new();
        let staged_crc = match owner
            .send(1, rpc::Request::StageConfig(tuned()))
            .unwrap()
            .result
        {
            rpc::RpcResult::Ok(rpc::Response::ConfigStaged { staged_crc32, .. }) => staged_crc32,
            other => panic!("{other:?}"),
        };
        assert!(!owner.config_save.is_active(), "staging writes nothing");
        assert_eq!(
            error_code(owner.send(
                2,
                rpc::Request::CommitConfig {
                    expected_staged_crc32: staged_crc ^ 1
                }
            )),
            rpc::DeviceError::ConfigurationConflict
        );
        assert_eq!(
            owner.send(
                3,
                rpc::Request::CommitConfig {
                    expected_staged_crc32: staged_crc
                }
            ),
            None,
            "answered when the save completes"
        );
        assert!(owner.config_save.is_active());
        assert_eq!(owner.completion, Some(ConfigRpcCompletion::Commit(3)));
        assert_eq!(owner.staged, None);
    }

    #[test]
    fn a_commit_with_nothing_staged_conflicts() {
        let mut owner = Owner::new();
        assert_eq!(
            error_code(owner.send(
                1,
                rpc::Request::CommitConfig {
                    expected_staged_crc32: 0
                }
            )),
            rpc::DeviceError::ConfigurationConflict
        );
    }

    #[test]
    fn armed_or_busy_refuses_changes_and_reads() {
        let mut owner = Owner::new();
        owner.armed = true;
        for operation in [
            rpc::Request::StageConfig(tuned()),
            rpc::Request::CommitConfig {
                expected_staged_crc32: 0,
            },
            rpc::Request::ResetConfigToDefaults,
            rpc::Request::ListBlackboxes,
        ] {
            assert_eq!(
                error_code(owner.send(1, operation)),
                rpc::DeviceError::Armed
            );
        }
        let mut owner = Owner::new();
        owner.maintenance = Maintenance::erase_log();
        assert_eq!(
            error_code(owner.send(1, rpc::Request::ResetConfigToDefaults)),
            rpc::DeviceError::Busy
        );
    }

    #[test]
    fn resetting_to_defaults_saves_them_and_answers_on_completion() {
        let mut owner = Owner::new();
        owner.staged = Some(StoredConfig::first_hop_default());
        assert_eq!(owner.send(7, rpc::Request::ResetConfigToDefaults), None);
        assert!(owner.config_save.is_active());
        assert_eq!(owner.completion, Some(ConfigRpcCompletion::Defaults(7)));
        assert_eq!(owner.staged, None);
    }

    #[test]
    fn an_empty_log_lists_nothing_and_erase_and_reboot_are_not_offered() {
        let mut owner = Owner::new();
        match owner.send(1, rpc::Request::ListBlackboxes).unwrap().result {
            rpc::RpcResult::Ok(rpc::Response::BlackboxList(list)) => {
                assert!(list.entries.is_empty() && !list.truncated)
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            error_code(owner.send(
                2,
                rpc::Request::EraseBlackbox {
                    id: rpc::BlackboxId(1)
                }
            )),
            rpc::DeviceError::UnsupportedOperation
        );
        assert_eq!(
            error_code(owner.send(
                3,
                rpc::Request::Reboot {
                    mode: rpc::RebootMode::RomDfu
                }
            )),
            rpc::DeviceError::UnsupportedOperation
        );
    }
}

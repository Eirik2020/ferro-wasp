//! The bounded queues between the flight tasks, created once in init. Each
//! constructor makes its queues static and returns their split ends, so a
//! board's init names what it receives and nothing else.

use crate::prelude::*;
use crate::{FlashRpcCommandConsumer, FlashRpcResponseProducer};
use ferrowasp_cli::tasks::usb::{FlashRpcCommandProducer, FlashRpcResponseConsumer};

/// The ESC manager and its three queues: telemetry requests to the DShot
/// service, its frame-start acknowledgements back, and observations for the
/// actuator.
pub struct EscChannels {
    pub esc_manager_state: esc::EscManager,
    pub esc_request_producer: esc::EscRequestProducer,
    pub esc_request_consumer: esc::EscRequestConsumer,
    pub esc_ack_producer: esc::EscAckProducer,
    pub esc_ack_consumer: esc::EscAckConsumer,
    pub esc_telemetry_update_producer: esc::EscTelemetryUpdateProducer,
    pub esc_telemetry_update_consumer: esc::EscTelemetryUpdateConsumer,
}

/// Create the ESC manager's queues. Call once.
pub fn esc_channels() -> EscChannels {
    let requests =
        cortex_m::singleton!(: esc::EscRequestQueue = esc::EscRequestQueue::new()).unwrap();
    let acknowledgements =
        cortex_m::singleton!(: esc::EscAckQueue = esc::EscAckQueue::new()).unwrap();
    let updates = cortex_m::singleton!(
        : esc::EscTelemetryUpdateQueue = esc::EscTelemetryUpdateQueue::new()
    )
    .unwrap();
    let (esc_request_producer, esc_request_consumer) = requests.split();
    let (esc_ack_producer, esc_ack_consumer) = acknowledgements.split();
    let (esc_telemetry_update_producer, esc_telemetry_update_consumer) = updates.split();
    EscChannels {
        esc_manager_state: esc::EscManager::new(esc::EscManagerConfig::legacy_uart(), 0),
        esc_request_producer,
        esc_request_consumer,
        esc_ack_producer,
        esc_ack_consumer,
        esc_telemetry_update_producer,
        esc_telemetry_update_consumer,
    }
}

/// The control tick link holds 8 ms of ticks at 2 kHz, eight observer runs.
const CONTROL_TICK_LINK_DEPTH: usize = 17;
/// Every `state` link holds four messages, two observer runs at the fastest
/// producer's 2 kHz.
const STATE_LINK_DEPTH: usize = 5;

/// The links between the IMU, the control loop and the pre-arm checks
/// (section 9), each a `state` link of four: the parser's sample to the
/// control loop, and the control loop's IMU status to the safety master and
/// to actuator output, one link each, since a link has one consumer.
pub struct ImuChannels {
    pub imu_sample_link: LinkProducer<'static, ferrowasp_io_contracts::imu::ImuSample>,
    pub imu_sample_consumer: LinkConsumer<'static, ferrowasp_io_contracts::imu::ImuSample>,
    pub safety_prearm_link: LinkProducer<'static, safety::prearm::ImuPreArmStatus>,
    pub safety_imu_prearm: safety::prearm::ImuPreArmReader<'static>,
    pub actuator_prearm_link: LinkProducer<'static, safety::prearm::ImuPreArmStatus>,
    pub actuator_imu_prearm: safety::prearm::ImuPreArmReader<'static>,
}

/// Create the IMU and pre-arm links. Call once.
pub fn imu_channels() -> ImuChannels {
    use ferrowasp_io_contracts::imu::ImuSample;
    use ferrowasp_io_contracts::link::link;
    use heapless::spsc::Queue;
    use safety::prearm::{ImuPreArmReader, ImuPreArmStatus};

    let (imu_sample_link, imu_sample_consumer) = link(
        cortex_m::singleton!(: Queue<Stamped<ImuSample>, STATE_LINK_DEPTH> = Queue::new()).unwrap(),
    );
    let (safety_prearm_link, safety) = link(
        cortex_m::singleton!(
            : Queue<Stamped<ImuPreArmStatus>, STATE_LINK_DEPTH> = Queue::new()
        )
        .unwrap(),
    );
    let (actuator_prearm_link, actuator) = link(
        cortex_m::singleton!(
            : Queue<Stamped<ImuPreArmStatus>, STATE_LINK_DEPTH> = Queue::new()
        )
        .unwrap(),
    );
    ImuChannels {
        imu_sample_link,
        imu_sample_consumer,
        safety_prearm_link,
        safety_imu_prearm: ImuPreArmReader::new(safety),
        actuator_prearm_link,
        actuator_imu_prearm: ImuPreArmReader::new(actuator),
    }
}

/// The observer and the producing end of every link into it (section 10):
/// each producer's end is a local resource of that task alone. The
/// blackbox stream's consuming end is the flash manager's.
pub struct ObserverChannels {
    pub observer: ferrowasp_border::observer::Observer<'static>,
    pub control_tick_link: LinkProducer<'static, observed::ControlTick>,
    pub imu_status_link: LinkProducer<'static, observed::ImuStatus>,
    pub drdy_status_link: LinkProducer<'static, observed::DrdyStatus>,
    pub rc_status_link: LinkProducer<'static, observed::RcStatus>,
    pub safety_status_link: LinkProducer<'static, observed::SafetyStatus>,
    pub output_status_link: LinkProducer<'static, observed::OutputStatus>,
    pub esc_status_link: LinkProducer<'static, observed::EscStatus>,
    pub battery_status_link: LinkProducer<'static, observed::BatteryStatus>,
    pub storage_status_link: LinkProducer<'static, observed::StorageStatus>,
    pub blackbox_stream: LinkConsumer<'static, observed::ControlTick>,
}

/// Create the observer's links around `stream`, the blackbox stream, which
/// the board sizes to ride out its storage's longest write stall. Call once.
pub fn observer_channels<const N: usize>(
    stream: &'static mut heapless::spsc::Queue<Stamped<observed::ControlTick>, N>,
) -> ObserverChannels {
    use ferrowasp_io_contracts::link::link;
    use heapless::spsc::Queue;

    macro_rules! state_link {
        ($message:ty) => {
            link(
                cortex_m::singleton!(
                    : Queue<Stamped<$message>, STATE_LINK_DEPTH> = Queue::new()
                )
                .unwrap(),
            )
        };
    }

    let (control_tick_link, control) = link(
        cortex_m::singleton!(
            : Queue<Stamped<observed::ControlTick>, CONTROL_TICK_LINK_DEPTH> = Queue::new()
        )
        .unwrap(),
    );
    let (imu_status_link, imu) = state_link!(observed::ImuStatus);
    let (drdy_status_link, drdy) = state_link!(observed::DrdyStatus);
    let (rc_status_link, rc) = state_link!(observed::RcStatus);
    let (safety_status_link, safety) = state_link!(observed::SafetyStatus);
    let (output_status_link, output) = state_link!(observed::OutputStatus);
    let (esc_status_link, esc) = state_link!(observed::EscStatus);
    let (battery_status_link, battery) = state_link!(observed::BatteryStatus);
    let (storage_status_link, storage) = state_link!(observed::StorageStatus);
    let (stream_producer, blackbox_stream) = link(stream);
    ObserverChannels {
        observer: ferrowasp_border::observer::Observer::new(
            ferrowasp_border::observer::ObserverLinks {
                control,
                imu,
                drdy,
                rc,
                safety,
                output,
                esc,
                battery,
                storage,
            },
            stream_producer,
        ),
        control_tick_link,
        imu_status_link,
        drdy_status_link,
        rc_status_link,
        safety_status_link,
        output_status_link,
        esc_status_link,
        battery_status_link,
        storage_status_link,
        blackbox_stream,
    }
}

/// The flash manager's queues: its own staging of flight records between
/// the blackbox stream and the log writer, the USB command line's commands
/// and responses, and the MSPv2 RPC queues (`()` without the MSPv2
/// configurator).
pub struct StorageChannels {
    pub flash_record_producer: flash_task::RecordProducer,
    pub flash_record_consumer: flash_task::RecordConsumer,
    pub flash_command_producer: flash_task::CommandProducer,
    pub flash_command_consumer: flash_task::CommandConsumer,
    pub flash_response_producer: flash_task::ResponseProducer,
    pub flash_response_consumer: flash_task::ResponseConsumer,
    pub flash_rpc_command_producer: FlashRpcCommandProducer,
    pub flash_rpc_command_consumer: FlashRpcCommandConsumer,
    pub flash_rpc_response_producer: FlashRpcResponseProducer,
    pub flash_rpc_response_consumer: FlashRpcResponseConsumer,
}

/// Create the flash manager's queues. Call once.
pub fn storage_channels() -> StorageChannels {
    let records =
        cortex_m::singleton!(: flash_task::RecordQueue = flash_task::RecordQueue::new()).unwrap();
    let (flash_record_producer, flash_record_consumer) = records.split();
    let commands =
        cortex_m::singleton!(: flash_task::CommandQueue = flash_task::CommandQueue::new()).unwrap();
    let responses =
        cortex_m::singleton!(: flash_task::ResponseQueue = flash_task::ResponseQueue::new())
            .unwrap();
    let (flash_command_producer, flash_command_consumer) = commands.split();
    let (flash_response_producer, flash_response_consumer) = responses.split();

    #[cfg(feature = "mspv2_configurator")]
    let (
        flash_rpc_command_producer,
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_response_consumer,
    ) = {
        let commands = cortex_m::singleton!(
            : flash_task::RpcCommandQueue = flash_task::RpcCommandQueue::new()
        )
        .unwrap();
        let responses = cortex_m::singleton!(
            : flash_task::RpcResponseQueue = flash_task::RpcResponseQueue::new()
        )
        .unwrap();
        let (command_producer, command_consumer) = commands.split();
        let (response_producer, response_consumer) = responses.split();
        (
            command_producer,
            command_consumer,
            response_producer,
            response_consumer,
        )
    };
    #[cfg(not(feature = "mspv2_configurator"))]
    let (
        flash_rpc_command_producer,
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_response_consumer,
    ) = ((), (), (), ());

    StorageChannels {
        flash_record_producer,
        flash_record_consumer,
        flash_command_producer,
        flash_command_consumer,
        flash_response_producer,
        flash_response_consumer,
        flash_rpc_command_producer,
        flash_rpc_command_consumer,
        flash_rpc_response_producer,
        flash_rpc_response_consumer,
    }
}

/// A second command line into the flash manager, for a UART bound to the
/// configurator function.
pub struct CommandLinkChannels {
    pub configurator_command_producer: flash_task::CommandProducer,
    pub configurator_command_consumer: flash_task::CommandConsumer,
    pub configurator_response_producer: flash_task::ResponseProducer,
    pub configurator_response_consumer: flash_task::ResponseConsumer,
}

/// Create the configurator link's command and response queues. Call once.
pub fn command_link_channels() -> CommandLinkChannels {
    let commands =
        cortex_m::singleton!(: flash_task::CommandQueue = flash_task::CommandQueue::new()).unwrap();
    let responses =
        cortex_m::singleton!(: flash_task::ResponseQueue = flash_task::ResponseQueue::new())
            .unwrap();
    let (configurator_command_producer, configurator_command_consumer) = commands.split();
    let (configurator_response_producer, configurator_response_consumer) = responses.split();
    CommandLinkChannels {
        configurator_command_producer,
        configurator_command_consumer,
        configurator_response_producer,
        configurator_response_consumer,
    }
}

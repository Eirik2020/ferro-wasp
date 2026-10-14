//! The bounded queues between the flight tasks, created once in init. Each
//! constructor makes its queues static and returns their split ends, so a
//! board's init names what it receives and nothing else.

use crate::app::{FlashRpcCommandProducer, FlashRpcResponseConsumer};
use crate::prelude::*;
use crate::{FlashRpcCommandConsumer, FlashRpcResponseProducer};

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

/// The flash manager's queues: records from the control loop, the USB
/// command line's commands and responses, and the MSPv2 RPC queues (`()`
/// without the MSPv2 configurator).
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

/// Create the flash manager's queues around `records`, which the board
/// sizes to ride out its storage's longest write stall. Call once.
pub fn storage_channels<const N: usize>(
    records: &'static mut flash_task::RecordQueue<N>,
) -> StorageChannels {
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

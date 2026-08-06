//! DMA-backed STM32F4 serial hardware endpoint.
//!
//! The reusable definition owns the transport task graph and common resource
//! roles. An application declaration supplies one board serial resource,
//! protocol, scheduling priorities, and bounded storage sizes.

use ferrowasp_io_core::serial::{SerialProfile, SerialProtocol};

use crate::{
    hardware_definitions::stm32f4::{board_declaration::BoardDeclaration, tasks},
    rtic::task::TaskContract,
};

/// Task contracts expanded for one serial endpoint instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialEndpointTasks {
    /// UART peripheral interrupt used for RX IDLE handling.
    pub peripheral_irq: &'static TaskContract,

    /// Receive DMA interrupt service.
    pub rx_dma_irq: &'static TaskContract,

    /// Transmit DMA completion interrupt service.
    pub tx_dma_irq: &'static TaskContract,

    /// Software task that serializes queued transmit chunks.
    pub tx_worker: &'static TaskContract,
}

/// Logical resource owned or exported by one serial endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialEndpointResourceRole {
    /// Receive DMA and UART peripheral state.
    RxService,
    /// Receive parser retained for the future owned-channel bridge task.
    RxParser,
    /// Static receive DMA buffer bank.
    RxBuffers,
    /// Queue of receive buffers available to DMA.
    RxFreeQueue,
    /// Queue of completed receive buffers.
    RxFilledQueue,
    /// Portable owned receive channel.
    RxChannel,
    /// Private producer side of the receive channel.
    RxProducer,
    /// Portable receive reader exported to a consumer task.
    RxReader,
    /// Receive discontinuity reader exported to a consumer task.
    RxDiscontinuities,
    /// Transmit DMA transfer state.
    TxDma,
    /// Static transmit DMA buffer.
    TxBuffer,
    /// Portable owned transmit channel.
    TxChannel,
    /// Portable serial writer exported to a consumer task.
    TxWriter,
    /// Private queue owner held by the transmit worker.
    TxOwner,
    /// Private completion handle held by the transmit DMA interrupt.
    TxCompletion,
}

/// RTIC ownership class of an endpoint resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialEndpointResourceOwnership {
    /// Constructed temporarily or split during RTIC initialization.
    InitLocal,
    /// Owned exclusively by one task.
    Local,
    /// Accessed by multiple tasks through RTIC locking.
    Shared,
}

/// Visibility of an endpoint resource outside its component instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialEndpointResourceVisibility {
    /// Only endpoint-owned tasks and initialization may access it.
    Private,
    /// Consumer tasks may bind the resource through a typed endpoint export.
    Exposed,
}

/// Directional condition controlling whether a resource is expanded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialEndpointResourceActivation {
    /// Present for receive-only and bidirectional endpoints.
    Always,
    /// Present only for bidirectional endpoints.
    Transmit,
}

/// One common endpoint resource and its ownership policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialEndpointResource {
    /// Stable semantic role used during endpoint lowering.
    pub role: SerialEndpointResourceRole,

    /// RTIC ownership class assigned during lowering.
    pub ownership: SerialEndpointResourceOwnership,

    /// Whether application consumer tasks may bind it.
    pub visibility: SerialEndpointResourceVisibility,

    /// Directional condition controlling resource expansion.
    pub activation: SerialEndpointResourceActivation,
}

impl SerialEndpointResource {
    const fn new(
        role: SerialEndpointResourceRole,
        ownership: SerialEndpointResourceOwnership,
        visibility: SerialEndpointResourceVisibility,
        activation: SerialEndpointResourceActivation,
    ) -> Self {
        Self {
            role,
            ownership,
            visibility,
            activation,
        }
    }
}

const PRIVATE: SerialEndpointResourceVisibility = SerialEndpointResourceVisibility::Private;
const EXPOSED: SerialEndpointResourceVisibility = SerialEndpointResourceVisibility::Exposed;
const ALWAYS: SerialEndpointResourceActivation = SerialEndpointResourceActivation::Always;
const TRANSMIT: SerialEndpointResourceActivation = SerialEndpointResourceActivation::Transmit;
const INIT_LOCAL: SerialEndpointResourceOwnership = SerialEndpointResourceOwnership::InitLocal;
const LOCAL: SerialEndpointResourceOwnership = SerialEndpointResourceOwnership::Local;
const SHARED: SerialEndpointResourceOwnership = SerialEndpointResourceOwnership::Shared;

/// Common resource graph owned by every DMA serial endpoint definition.
pub const SERIAL_ENDPOINT_RESOURCES: &[SerialEndpointResource] = &[
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxService,
        SHARED,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(SerialEndpointResourceRole::RxParser, LOCAL, PRIVATE, ALWAYS),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxBuffers,
        INIT_LOCAL,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxFreeQueue,
        INIT_LOCAL,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxFilledQueue,
        INIT_LOCAL,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxChannel,
        INIT_LOCAL,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxProducer,
        LOCAL,
        PRIVATE,
        ALWAYS,
    ),
    SerialEndpointResource::new(SerialEndpointResourceRole::RxReader, LOCAL, EXPOSED, ALWAYS),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::RxDiscontinuities,
        LOCAL,
        EXPOSED,
        ALWAYS,
    ),
    SerialEndpointResource::new(SerialEndpointResourceRole::TxDma, SHARED, PRIVATE, TRANSMIT),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::TxBuffer,
        INIT_LOCAL,
        PRIVATE,
        TRANSMIT,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::TxChannel,
        INIT_LOCAL,
        PRIVATE,
        TRANSMIT,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::TxWriter,
        LOCAL,
        EXPOSED,
        TRANSMIT,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::TxOwner,
        LOCAL,
        PRIVATE,
        TRANSMIT,
    ),
    SerialEndpointResource::new(
        SerialEndpointResourceRole::TxCompletion,
        LOCAL,
        PRIVATE,
        TRANSMIT,
    ),
];

/// Reusable serial endpoint definition shared by every concrete instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialEndpointDefinition {
    /// Stable definition identifier.
    pub id: &'static str,

    /// Reusable transport task contracts.
    pub tasks: SerialEndpointTasks,

    /// Common resource graph owned by an endpoint instance.
    pub resources: &'static [SerialEndpointResource],
}

impl SerialEndpointDefinition {
    /// Creates a concrete endpoint instance bound to board serial hardware.
    pub const fn declare(
        self,
        id: &'static str,
        hardware_id: &'static str,
    ) -> SerialEndpointDeclaration {
        SerialEndpointDeclaration {
            id,
            definition: self,
            hardware_id,
            profile: SerialProfile::disabled(),
            direction: SerialEndpointDirection::ReceiveOnly,
            interrupt_priority: 0,
            worker_priority: None,
            rx_buffer_count: 4,
            rx_queue_depth: 4,
            tx_queue_depth: 16,
        }
    }
}

/// Transfer directions requested by an application endpoint instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialEndpointDirection {
    /// Receive service only.
    ReceiveOnly,
    /// Receive and transmit service.
    Bidirectional,
}

/// One serial endpoint instance declared by an application composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialEndpointDeclaration {
    /// Application-local endpoint identifier used to namespace expanded artifacts.
    pub id: &'static str,

    /// Reusable task and common-resource definition.
    pub definition: SerialEndpointDefinition,

    /// Stable ID of the serial hardware consumed from the selected board.
    pub hardware_id: &'static str,

    /// Electrical and protocol profile used to initialize the UART.
    pub profile: SerialProfile,

    /// Requested receive-only or bidirectional service.
    pub direction: SerialEndpointDirection,

    /// Priority assigned to all endpoint hardware interrupt tasks.
    pub interrupt_priority: u8,

    /// Priority assigned to the transmit worker when TX is enabled.
    pub worker_priority: Option<u8>,

    /// Number of statically allocated receive DMA buffers.
    pub rx_buffer_count: usize,

    /// Capacity of the portable receive queue.
    pub rx_queue_depth: usize,

    /// Capacity of the portable transmit queue.
    pub tx_queue_depth: usize,
}

impl SerialEndpointDeclaration {
    /// Selects the UART electrical and protocol profile.
    pub const fn profile(mut self, profile: SerialProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Enables both receive and transmit endpoint services.
    pub const fn bidirectional(mut self) -> Self {
        self.direction = SerialEndpointDirection::Bidirectional;
        self
    }

    /// Sets the common priority of all endpoint hardware interrupts.
    pub const fn interrupt_priority(mut self, priority: u8) -> Self {
        self.interrupt_priority = priority;
        self
    }

    /// Sets the endpoint-owned transmit worker priority.
    pub const fn worker_priority(mut self, priority: u8) -> Self {
        self.worker_priority = Some(priority);
        self
    }

    /// Sets the number of statically allocated RX DMA buffers.
    pub const fn rx_buffer_count(mut self, count: usize) -> Self {
        self.rx_buffer_count = count;
        self
    }

    /// Sets the portable receive queue depth.
    pub const fn rx_queue_depth(mut self, depth: usize) -> Self {
        self.rx_queue_depth = depth;
        self
    }

    /// Sets the portable transmit queue depth.
    pub const fn tx_queue_depth(mut self, depth: usize) -> Self {
        self.tx_queue_depth = depth;
        self
    }
}

/// STM32F4 DMA serial endpoint used by application compositions.
pub const SERIAL_DMA_ENDPOINT: SerialEndpointDefinition = SerialEndpointDefinition {
    id: "serial_dma",
    tasks: SerialEndpointTasks {
        peripheral_irq: &tasks::serial_rx_idle_irq::CONTRACT,
        rx_dma_irq: &tasks::serial_rx_dma_irq::CONTRACT,
        tx_dma_irq: &tasks::serial_tx_dma_irq::CONTRACT,
        tx_worker: &tasks::serial_tx_worker::CONTRACT,
    },
    resources: SERIAL_ENDPOINT_RESOURCES,
};

pub(crate) fn validate(
    endpoint: SerialEndpointDeclaration,
    board: &BoardDeclaration,
) -> Result<(), String> {
    let Some(hardware) = board.serial(endpoint.hardware_id) else {
        return Err(format!(
            "serial endpoint `{}` consumes undeclared board serial hardware `{}`",
            endpoint.id, endpoint.hardware_id
        ));
    };
    if endpoint.profile.protocol == SerialProtocol::Disabled {
        return Err(format!(
            "serial endpoint `{}` must select an enabled serial profile",
            endpoint.id
        ));
    }
    if endpoint.interrupt_priority == 0 {
        return Err(format!(
            "serial endpoint `{}` must have a nonzero interrupt priority",
            endpoint.id
        ));
    }
    let Some(rx) = hardware.port.rx else {
        return Err(format!(
            "serial endpoint `{}` requires an RX route on board hardware `{}`",
            endpoint.id, endpoint.hardware_id
        ));
    };
    if rx.dma.is_none() {
        return Err(format!(
            "serial endpoint `{}` requires RX DMA on board hardware `{}`",
            endpoint.id, endpoint.hardware_id
        ));
    }
    if endpoint.rx_buffer_count == 0 || endpoint.rx_queue_depth == 0 {
        return Err(format!(
            "serial endpoint `{}` RX buffer count and queue depth must be nonzero",
            endpoint.id
        ));
    }

    match endpoint.direction {
        SerialEndpointDirection::ReceiveOnly => {
            if endpoint.worker_priority.is_some() {
                return Err(format!(
                    "receive-only serial endpoint `{}` cannot declare a TX worker priority",
                    endpoint.id
                ));
            }
        }
        SerialEndpointDirection::Bidirectional => {
            let Some(tx) = hardware.port.tx else {
                return Err(format!(
                    "bidirectional serial endpoint `{}` requires a TX route on board hardware `{}`",
                    endpoint.id, endpoint.hardware_id
                ));
            };
            if tx.dma.is_none() {
                return Err(format!(
                    "bidirectional serial endpoint `{}` requires TX DMA on board hardware `{}`",
                    endpoint.id, endpoint.hardware_id
                ));
            }
            if endpoint
                .worker_priority
                .is_none_or(|priority| priority == 0)
            {
                return Err(format!(
                    "bidirectional serial endpoint `{}` must have a nonzero worker priority",
                    endpoint.id
                ));
            }
            if endpoint.tx_queue_depth == 0 {
                return Err(format!(
                    "bidirectional serial endpoint `{}` TX queue depth must be nonzero",
                    endpoint.id
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_contains_all_four_transport_tasks() {
        assert_eq!(
            SERIAL_DMA_ENDPOINT.tasks.peripheral_irq.id,
            "serial_rx_idle_irq"
        );
        assert_eq!(SERIAL_DMA_ENDPOINT.tasks.rx_dma_irq.id, "serial_rx_dma_irq");
        assert_eq!(SERIAL_DMA_ENDPOINT.tasks.tx_dma_irq.id, "serial_tx_dma_irq");
        assert_eq!(SERIAL_DMA_ENDPOINT.tasks.tx_worker.id, "serial_tx_worker");
    }

    #[test]
    fn writer_is_exposed_but_tx_mechanics_remain_private() {
        let writer = SERIAL_ENDPOINT_RESOURCES
            .iter()
            .find(|resource| resource.role == SerialEndpointResourceRole::TxWriter)
            .unwrap();
        let dma = SERIAL_ENDPOINT_RESOURCES
            .iter()
            .find(|resource| resource.role == SerialEndpointResourceRole::TxDma)
            .unwrap();

        assert_eq!(writer.visibility, SerialEndpointResourceVisibility::Exposed);
        assert_eq!(dma.visibility, SerialEndpointResourceVisibility::Private);
    }
}

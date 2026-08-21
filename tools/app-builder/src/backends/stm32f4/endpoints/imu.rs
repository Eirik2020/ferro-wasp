//! DMA-backed STM32F4 SPI IMU endpoint.

use crate::{
    backends::stm32f4::{board_declaration::BoardDeclaration, tasks},
    rtic::task::TaskContract,
};

/// Reusable task contracts expanded for one SPI IMU instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuEndpointTasks {
    /// Data-ready EXTI handler.
    pub data_ready: &'static TaskContract,
    /// Asynchronous sensor transaction task.
    pub poll: &'static TaskContract,
    /// Mailbox-to-hardware owner service.
    pub owner_service: &'static TaskContract,
    /// Receive-DMA completion handler.
    pub rx_dma_irq: &'static TaskContract,
    /// Transaction deadline and recovery task.
    pub timeout: &'static TaskContract,
    /// Completed-frame parser.
    pub parser: &'static TaskContract,
}

/// Semantic role of one endpoint-owned resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImuEndpointResourceRole {
    /// Static DMA buffer bank.
    Buffers,
    /// Static queue of available receive buffers.
    FreeQueue,
    /// Static queue of completed receive buffers.
    FilledQueue,
    /// DMA hardware owner and bounded request mailbox.
    Owner,
    /// Completed-frame parser side.
    Parser,
    /// Async SPI device used by the polling task.
    Device,
    /// Data-ready EXTI input.
    DataReady,
    /// Active sensor discriminant selected during initialization.
    Kind,
    /// Latest decoded IMU sample.
    Sample,
    /// One-shot unavailable-owner diagnostic state.
    UnavailableLogged,
}

/// RTIC ownership class assigned to an IMU endpoint resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImuEndpointResourceOwnership {
    /// Static storage borrowed during initialization.
    InitLocal,
    /// Resource owned by exactly one task.
    Local,
    /// Resource accessed through RTIC locking.
    Shared,
}

/// Visibility of an endpoint resource to application consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImuEndpointResourceVisibility {
    /// Endpoint implementation detail.
    Private,
    /// Available for a later application-consumer binding.
    Exposed,
}

/// One reusable resource in the SPI IMU endpoint graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuEndpointResource {
    /// Stable semantic role.
    pub role: ImuEndpointResourceRole,
    /// RTIC ownership policy.
    pub ownership: ImuEndpointResourceOwnership,
    /// Consumer visibility.
    pub visibility: ImuEndpointResourceVisibility,
}

impl ImuEndpointResource {
    const fn new(
        role: ImuEndpointResourceRole,
        ownership: ImuEndpointResourceOwnership,
        visibility: ImuEndpointResourceVisibility,
    ) -> Self {
        Self {
            role,
            ownership,
            visibility,
        }
    }
}

use ImuEndpointResourceOwnership::{InitLocal, Local, Shared};
use ImuEndpointResourceRole::{
    Buffers, DataReady, Device, FilledQueue, FreeQueue, Kind, Owner, Parser, Sample,
    UnavailableLogged,
};
use ImuEndpointResourceVisibility::{Exposed, Private};

/// Complete bounded resource graph for one SPI IMU endpoint.
pub const IMU_ENDPOINT_RESOURCES: &[ImuEndpointResource] = &[
    ImuEndpointResource::new(Buffers, InitLocal, Private),
    ImuEndpointResource::new(FreeQueue, InitLocal, Private),
    ImuEndpointResource::new(FilledQueue, InitLocal, Private),
    ImuEndpointResource::new(Owner, Shared, Private),
    ImuEndpointResource::new(Parser, Local, Private),
    ImuEndpointResource::new(Device, Local, Private),
    ImuEndpointResource::new(DataReady, Local, Private),
    ImuEndpointResource::new(Kind, Shared, Exposed),
    ImuEndpointResource::new(Sample, Shared, Exposed),
    ImuEndpointResource::new(UnavailableLogged, Local, Private),
];

/// Reusable definition expanded for every concrete SPI IMU endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuEndpointDefinition {
    /// Stable reusable definition identifier.
    pub id: &'static str,
    /// Endpoint-owned task graph.
    pub tasks: ImuEndpointTasks,
    /// Endpoint-owned resource graph.
    pub resources: &'static [ImuEndpointResource],
}

impl ImuEndpointDefinition {
    /// Creates an endpoint instance bound only to a physical SPI bus.
    pub const fn declare(self, id: &'static str, spi_id: &'static str) -> ImuEndpointDeclaration {
        ImuEndpointDeclaration {
            id,
            definition: self,
            hardware_id: spi_id,
            data_ready_priority: 0,
            owner_priority: 0,
            poll_priority: 0,
            parser_priority: 0,
        }
    }
}

/// One application-selected SPI IMU endpoint instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImuEndpointDeclaration {
    /// Application-local identifier used to namespace generated artifacts.
    pub id: &'static str,
    /// Reusable endpoint definition.
    pub definition: ImuEndpointDefinition,
    /// Board hardware consumed by this transport endpoint.
    pub hardware_id: &'static str,
    /// Priority of the data-ready EXTI task.
    pub data_ready_priority: u8,
    /// Priority shared by owner, DMA, and timeout service tasks.
    pub owner_priority: u8,
    /// Priority of the asynchronous transaction task.
    pub poll_priority: u8,
    /// Priority of the completed-frame parser.
    pub parser_priority: u8,
}

impl ImuEndpointDeclaration {
    /// Sets the data-ready interrupt priority.
    pub const fn data_ready_priority(mut self, priority: u8) -> Self {
        self.data_ready_priority = priority;
        self
    }

    /// Sets the common owner, DMA, and timeout priority.
    pub const fn owner_priority(mut self, priority: u8) -> Self {
        self.owner_priority = priority;
        self
    }

    /// Sets the asynchronous SPI polling priority.
    pub const fn poll_priority(mut self, priority: u8) -> Self {
        self.poll_priority = priority;
        self
    }

    /// Sets the completed-frame parser priority.
    pub const fn parser_priority(mut self, priority: u8) -> Self {
        self.parser_priority = priority;
        self
    }
}

/// Initial STM32F4 SPI IMU endpoint definition.
pub const SPI_IMU_ENDPOINT: ImuEndpointDefinition = ImuEndpointDefinition {
    id: "spi_imu",
    tasks: ImuEndpointTasks {
        data_ready: &tasks::imu_data_ready::CONTRACT,
        poll: &tasks::spi_imu_poll::CONTRACT,
        owner_service: &tasks::spi_imu_owner_service::CONTRACT,
        rx_dma_irq: &tasks::spi_imu_rx_dma_irq::CONTRACT,
        timeout: &tasks::spi_imu_timeout::CONTRACT,
        parser: &tasks::spi_imu_parser::CONTRACT,
    },
    resources: IMU_ENDPOINT_RESOURCES,
};

/// Hardware-neutral SPI endpoint declaration used by compositions.
pub type SpiEndpointDeclaration = ImuEndpointDeclaration;

/// SPI endpoint definition with the currently supported service adapters.
pub const SPI_DMA_ENDPOINT: ImuEndpointDefinition = SPI_IMU_ENDPOINT;

pub(crate) const fn resource_suffix(role: ImuEndpointResourceRole) -> &'static str {
    match role {
        ImuEndpointResourceRole::Buffers => "dma_buffers",
        ImuEndpointResourceRole::FreeQueue => "free_queue",
        ImuEndpointResourceRole::FilledQueue => "filled_queue",
        ImuEndpointResourceRole::Owner => "owner",
        ImuEndpointResourceRole::Parser => "parser",
        ImuEndpointResourceRole::Device => "device",
        ImuEndpointResourceRole::DataReady => "data_ready",
        ImuEndpointResourceRole::Kind => "kind",
        ImuEndpointResourceRole::Sample => "sample",
        ImuEndpointResourceRole::UnavailableLogged => "unavailable_logged",
    }
}

pub(crate) fn validate(
    endpoint: ImuEndpointDeclaration,
    board: &BoardDeclaration,
) -> Result<(), String> {
    let spi_id = endpoint.hardware_id;
    if board.spi(spi_id).is_none() {
        return Err(format!(
            "SPI endpoint `{}` consumes undeclared board SPI hardware `{}`",
            endpoint.id, spi_id
        ));
    }
    if board
        .spi(spi_id)
        .is_some_and(|hardware| hardware.imu.is_empty())
    {
        return Err(format!(
            "SPI endpoint `{}` has no board IMU installation attached to `{}` for the enabled IMU service adapter",
            endpoint.id, spi_id
        ));
    }
    if [
        endpoint.data_ready_priority,
        endpoint.owner_priority,
        endpoint.poll_priority,
        endpoint.parser_priority,
    ]
    .contains(&0)
    {
        return Err(format!(
            "IMU endpoint `{}` priorities must all be nonzero",
            endpoint.id
        ));
    }
    if !(endpoint.data_ready_priority > endpoint.owner_priority
        && endpoint.owner_priority > endpoint.poll_priority
        && endpoint.poll_priority > endpoint.parser_priority)
    {
        return Err(format!(
            "IMU endpoint `{}` priorities must satisfy data-ready > owner > poll > parser",
            endpoint.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_contains_the_bounded_golden_task_graph() {
        assert_eq!(SPI_IMU_ENDPOINT.tasks.data_ready.id, "imu_data_ready");
        assert_eq!(
            SPI_IMU_ENDPOINT.tasks.owner_service.id,
            "spi_imu_owner_service"
        );
        assert_eq!(SPI_IMU_ENDPOINT.tasks.rx_dma_irq.id, "spi_imu_rx_dma_irq");
        assert_eq!(SPI_IMU_ENDPOINT.tasks.timeout.id, "spi_imu_timeout");
        assert_eq!(SPI_IMU_ENDPOINT.tasks.parser.id, "spi_imu_parser");
    }

    #[test]
    fn decoded_sample_is_exposed_while_transport_owner_is_private() {
        let sample = IMU_ENDPOINT_RESOURCES
            .iter()
            .find(|resource| resource.role == Sample)
            .unwrap();
        let owner = IMU_ENDPOINT_RESOURCES
            .iter()
            .find(|resource| resource.role == Owner)
            .unwrap();

        assert_eq!(sample.visibility, Exposed);
        assert_eq!(owner.visibility, Private);
    }
}

use ferrowasp_core::safety::ActuatorPreparationReport;

use crate::backends::stm32f4::task_authoring::dshot::{
    DshotInterruptEvent, DshotMotor, DshotMotorBank,
};

crate::reusable_task! {
    contract {
        local {}
        shared {
            /// Indivisible four-lane DShot containment unit.
            bank: DshotMotorBank,
        }
        config {
            /// One-based physical lane associated with this DMA vector.
            lane: u32,
        }
        spawns {
            /// Reports any terminal or spurious DMA event to safety.
            fault(report: ferrowasp_core::safety::ActuatorPreparationReport),
        }
    }

    /// Services one exact physical-lane completion interrupt.
    pub fn dshot_dma_complete(mut cx: dshot_dma_complete::Context<'_>) {
        let motor = match cx.config.lane {
            1 => DshotMotor::Motor1,
            2 => DshotMotor::Motor2,
            3 => DshotMotor::Motor3,
            4 => DshotMotor::Motor4,
            _ => {
                let _ = fault::spawn(ActuatorPreparationReport::Faulted);
                return;
            }
        };
        let event = cx
            .shared
            .bank
            .lock(|bank| bank.on_dma_interrupt(motor));
        if !matches!(event, DshotInterruptEvent::Completed)
            && fault::spawn(ActuatorPreparationReport::Faulted).is_err()
        {
            defmt::warn!("DShot DMA fault report rejected");
        }
    }
}

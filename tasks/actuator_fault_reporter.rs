use ferrowasp_core::{safety::ActuatorPreparationReport, safety_channel::SafetyProducer};

crate::reusable_task! {
    contract {
        local {
            /// Exclusive fault-report path into the safety master.
            reports: SafetyProducer<'static, ActuatorPreparationReport>,
        }
        shared {}
        config {}
        spawns {}
    }

    /// Publishes one physical-service fault through its sole bounded producer.
    pub async fn actuator_fault_reporter(
        cx: actuator_fault_reporter::Context<'_>,
        report: ferrowasp_core::safety::ActuatorPreparationReport,
    ) {
        if cx.local.reports.try_send(report).is_err() {
            defmt::warn!("actuator fault channel full; output remains fail-closed");
        }
    }
}

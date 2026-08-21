use crate::rtic::task::Mono;
use ferrowasp_core::{safety::MotorCmd, safety_channel::SafetyConsumer};

crate::reusable_task! {
    contract {
        local {
            /// Exclusive command receive handle owned by actuator safety.
            commands: SafetyConsumer<'static, MotorCmd>,
        }
        shared {}
        config {}
        spawns {}
    }

    /// Drains and validates commands while unconditionally inhibiting output.
    pub async fn inhibited_actuator(
        cx: inhibited_actuator::Context<'_>,
        request: ferrowasp_core::safety::ActuatorCmd,
    ) {
        let outcome = ferrowasp_tasks::actuator::handle_inhibited_actuator_wake(
            request,
            Mono::now().duration_since_epoch().to_millis() as u32,
            || cx.local.commands.try_receive(),
        );
        match outcome {
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::Disarmed => {}
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::ArmRequestInhibited => {
                defmt::warn!("actuator arming request refused; output is inhibited")
            }
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::CommandInhibited { seq } => {
                defmt::warn!("motor command {} validated but output remains inhibited", seq)
            }
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::MissingCommand => {
                defmt::warn!("actuator wake refused: motor command queue empty")
            }
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::StaleCommand { seq } => {
                defmt::warn!("stale motor command {} refused", seq)
            }
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::InvalidCommand { seq } => {
                defmt::warn!("invalid motor command {} refused", seq)
            }
            ferrowasp_tasks::actuator::InhibitedActuatorOutcome::DrainLimitExceeded => {
                defmt::warn!("motor command channel exceeded its declared capacity")
            }
        }
    }
}

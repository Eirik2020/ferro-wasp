use crate::backends::stm32f4::periodic_control::{ControlScheduler, acknowledge_control_tick};

crate::reusable_task! {
    contract {
        local {
            /// Timer counter that owns and acknowledges the update interrupt.
            scheduler: ControlScheduler,
            /// Scheduler phase accumulated toward the next control step.
            phase: u32,
        }
        shared {}
        config {
            /// Exact number of scheduler ticks in one control period.
            ticks_per_control: u32,
        }
        spawns {}
    }

    /// Acknowledges every scheduler update and gates work to control cadence.
    pub fn periodic_control_tick(cx: periodic_control_tick::Context<'_>) {
        acknowledge_control_tick(cx.local.scheduler);
        *cx.local.phase = cx.local.phase.wrapping_add(1);
        if *cx.local.phase < cx.config.ticks_per_control {
            return;
        }
        *cx.local.phase = 0;

        // The real bounded control step replaces this deliberate no-op in the
        // dedicated control-task checklist point. This component owns timing,
        // not actuator authority.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tick_is_acknowledged_and_every_second_tick_reaches_control_cadence() {
        let mut scheduler = ControlScheduler::default();
        let mut phase = 0;

        periodic_control_tick(periodic_control_tick::Context::new(
            periodic_control_tick::Local::new(&mut scheduler, &mut phase),
            periodic_control_tick::Shared::new(),
            periodic_control_tick::Config::new(2),
        ));
        assert_eq!(scheduler.acknowledged_ticks(), 1);
        assert_eq!(phase, 1);

        periodic_control_tick(periodic_control_tick::Context::new(
            periodic_control_tick::Local::new(&mut scheduler, &mut phase),
            periodic_control_tick::Shared::new(),
            periodic_control_tick::Config::new(2),
        ));
        assert_eq!(scheduler.acknowledged_ticks(), 2);
        assert_eq!(phase, 0);
    }
}

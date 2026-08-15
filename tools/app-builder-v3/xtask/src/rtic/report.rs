//! Deterministic human-review maps for a fully resolved application graph.

use std::fmt::Write as _;

use super::{
    composition::TaskSafetyClass,
    resolve::{ResolvedApp, ResolvedTaskTrigger},
};

/// Renders the generic task, interrupt, spawn, channel, and resource maps.
///
/// Target-specific parity notes are deliberately appended by the target
/// module: the generic RTIC model must not know about a handwritten reference
/// application.
pub fn render(resolved: &ResolvedApp) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "# Generated output-disabled physical safety spine\n"
    )
    .unwrap();
    writeln!(
        output,
        "> Generated review artifact. Do not edit directly. This graph declares the reviewed motor and ESC-telemetry hardware, but the safety policy and physical component independently disable output. Generation, compilation, and linking are not target, electrical, bench, or flight evidence.\n"
    )
    .unwrap();
    writeln!(output, "Board: `{}`\n", resolved.board.id).unwrap();
    render_safety_authority(&mut output, resolved);
    render_tasks(&mut output, resolved);
    render_interrupts(&mut output, resolved);
    render_spawns(&mut output, resolved);
    render_channels(&mut output, resolved);
    render_resources(&mut output, resolved);
    render_resource_inventory(&mut output, resolved);
    render_state(&mut output, resolved);
    render_components(&mut output, resolved);
    output
}

fn render_safety_authority(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Safety-authority map\n").unwrap();
    writeln!(output, "| Task | Priority | Authority boundary |").unwrap();
    writeln!(output, "| --- | ---: | --- |").unwrap();
    for task in sorted_tasks(resolved)
        .into_iter()
        .filter(|task| task.safety_class != TaskSafetyClass::NonSafetyCritical)
    {
        writeln!(
            output,
            "| `{}` | {} | {} |",
            task.id,
            task.priority,
            safety_class(task.safety_class)
        )
        .unwrap();
    }
    writeln!(
        output,
        "\nOnly `safety_master` owns arming state and the actuator permit. `control_loop` may publish typed requests but cannot grant authority. `actuator_output` owns the sole motor-command consumer and is the only task that may translate a fresh armed request into a bank command. Both selected enable gates are false.\n"
    )
    .unwrap();
}

fn render_tasks(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Task and priority map\n").unwrap();
    writeln!(
        output,
        "| Task | Entry | Priority | Safety class | Component | Init-spawned |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | ---: | --- | --- | --- |").unwrap();
    for task in sorted_tasks(resolved) {
        let trigger = match &task.trigger {
            ResolvedTaskTrigger::Interrupt(interrupt) => format!("interrupt `{interrupt}`"),
            ResolvedTaskTrigger::Software => "software spawn".to_owned(),
        };
        let component = task.owner_component.unwrap_or("standalone");
        let init_spawned = if resolved.init_spawns.contains(&task.id) {
            "yes"
        } else {
            "no"
        };
        writeln!(
            output,
            "| `{}` | {} | {} | {} | `{}` | {} |",
            task.id,
            trigger,
            task.priority,
            safety_class(task.safety_class),
            component,
            init_spawned
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_interrupts(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Interrupt and dispatcher map\n").unwrap();
    writeln!(output, "| Vector | Owner task | Priority |").unwrap();
    writeln!(output, "| --- | --- | ---: |").unwrap();
    let mut interrupts = resolved
        .tasks
        .iter()
        .filter_map(|task| match &task.trigger {
            ResolvedTaskTrigger::Interrupt(interrupt) => {
                Some((interrupt.as_str(), task.id.as_str(), task.priority))
            }
            ResolvedTaskTrigger::Software => None,
        })
        .collect::<Vec<_>>();
    interrupts.sort_unstable();
    for (interrupt, task, priority) in interrupts {
        writeln!(output, "| `{interrupt}` | `{task}` | {priority} |").unwrap();
    }
    writeln!(
        output,
        "\nSoftware dispatchers: {}.\n",
        code_list(&resolved.dispatchers)
    )
    .unwrap();
}

fn render_spawns(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Spawn map\n").unwrap();
    writeln!(output, "| Source | Destination | Arguments |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();
    let mut init_spawns = resolved.init_spawns.iter().collect::<Vec<_>>();
    init_spawns.sort_unstable();
    for target in init_spawns {
        writeln!(output, "| `init` | `{target}` | none |").unwrap();
    }
    let mut edges = resolved
        .tasks
        .iter()
        .flat_map(|task| task.spawns.iter().map(move |spawn| (task, spawn)))
        .collect::<Vec<_>>();
    edges.sort_by(|(left_task, left), (right_task, right)| {
        (&left_task.id, &left.target, left.logical).cmp(&(
            &right_task.id,
            &right.target,
            right.logical,
        ))
    });
    for (task, spawn) in edges {
        let arguments = if spawn.arguments.is_empty() {
            "none".to_owned()
        } else {
            spawn
                .arguments
                .iter()
                .map(|argument| format!("`{}: {}`", argument.id, argument.rust_type))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(
            output,
            "| `{}` | `{}` | {} |",
            task.id, spawn.target, arguments
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_channels(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Authoritative safety-channel map\n").unwrap();
    writeln!(
        output,
        "| Channel | Message | Usable capacity | Producer owner / handle | Consumer owner / handle |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | ---: | --- | --- |").unwrap();
    let mut channels = resolved.safety_channels.iter().collect::<Vec<_>>();
    channels.sort_by_key(|channel| channel.id);
    for channel in channels {
        writeln!(
            output,
            "| `{}` | `{:?}` | {} | `{}` / `{}` | `{}` / `{}` |",
            channel.id,
            channel.message,
            channel.usable_capacity,
            channel.producer.owner_task,
            channel.producer.id,
            channel.consumer.owner_task,
            channel.consumer.id
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_resources(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Task resource map\n").unwrap();
    writeln!(
        output,
        "| Task | Access | Logical name | Resolved resource |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- |").unwrap();
    for task in sorted_tasks(resolved) {
        for binding in &task.local {
            writeln!(
                output,
                "| `{}` | local/exclusive | `{}` | `{}` |",
                task.id, binding.logical, binding.target
            )
            .unwrap();
        }
        for binding in &task.shared {
            writeln!(
                output,
                "| `{}` | shared/locked | `{}` | `{}` |",
                task.id, binding.logical, binding.target
            )
            .unwrap();
        }
    }
    output.push('\n');
}

fn render_resource_inventory(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Resolved resource inventory\n").unwrap();
    writeln!(output, "| Resource | Kind | Ownership / role |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();

    let mut shared = resolved.shared_resources.iter().collect::<Vec<_>>();
    shared.sort_by_key(|resource| resource.id);
    for resource in shared {
        writeln!(
            output,
            "| `{}` | application shared | RTIC locked; initial `{:?}` |",
            resource.id, resource.initial
        )
        .unwrap();
    }

    let mut gpio = resolved.gpio_resources.iter().collect::<Vec<_>>();
    gpio.sort_by_key(|resource| resource.hardware.id);
    for resource in gpio {
        writeln!(
            output,
            "| `{}` | board GPIO | exclusive task `{}` |",
            resource.hardware.id, resource.owner_task
        )
        .unwrap();
    }

    let mut endpoint_resources = resolved
        .serial_endpoints
        .iter()
        .flat_map(|endpoint| {
            endpoint.resources.iter().map(move |resource| {
                (
                    resource.id.as_str(),
                    format!("serial endpoint `{}`", endpoint.declaration.id),
                    format!(
                        "{:?}; {:?}; {:?}",
                        resource.role, resource.ownership, resource.visibility
                    ),
                )
            })
        })
        .chain(resolved.imu_endpoints.iter().flat_map(|endpoint| {
            endpoint.resources.iter().map(move |resource| {
                (
                    resource.id.as_str(),
                    format!("IMU endpoint `{}`", endpoint.declaration.id),
                    format!(
                        "{:?}; {:?}; {:?}",
                        resource.role, resource.ownership, resource.visibility
                    ),
                )
            })
        }))
        .collect::<Vec<_>>();
    endpoint_resources.sort_by(|left, right| left.0.cmp(right.0));
    for (id, kind, ownership) in endpoint_resources {
        writeln!(output, "| `{id}` | {kind} | {ownership} |").unwrap();
    }

    let mut control_resources = resolved
        .periodic_controls
        .iter()
        .flat_map(|control| {
            control
                .resources
                .iter()
                .map(move |resource| (resource.id.as_str(), control.declaration.id, resource.role))
        })
        .collect::<Vec<_>>();
    control_resources.sort_by(|left, right| left.0.cmp(right.0));
    for (id, component, role) in control_resources {
        writeln!(
            output,
            "| `{id}` | periodic control `{component}` | local; `{:?}` |",
            role
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_state(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Persistent task-state ownership\n").unwrap();
    writeln!(output, "| State | Sole owner | Reviewed recipe |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();
    let mut states = resolved.task_state.iter().collect::<Vec<_>>();
    states.sort_by_key(|state| state.id);
    for state in states {
        writeln!(
            output,
            "| `{}` | `{}` | `{:?}` |",
            state.id, state.owner_task, state.recipe
        )
        .unwrap();
    }
    output.push('\n');
}

fn render_components(output: &mut String, resolved: &ResolvedApp) {
    writeln!(output, "## Component and physical-resource map\n").unwrap();
    writeln!(output, "| Instance | Physical resource | Role |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();
    for endpoint in &resolved.serial_endpoints {
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | DMA serial endpoint |",
            endpoint.declaration.id, endpoint.hardware.port.peripheral, endpoint.hardware.id
        )
        .unwrap();
    }
    for endpoint in &resolved.imu_endpoints {
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | SPI IMU endpoint, installation `{:?}` |",
            endpoint.declaration.id,
            endpoint.spi.bus.peripheral,
            endpoint.spi.id,
            endpoint.hardware.id
        )
        .unwrap();
    }
    for control in &resolved.periodic_controls {
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | {} Hz scheduler / {} Hz control |",
            control.declaration.id,
            control.hardware.peripheral,
            control.hardware.id,
            control.declaration.scheduler_hz,
            control.declaration.control_hz
        )
        .unwrap();
    }
    for actuator in &resolved.dshot_actuators {
        for lane in actuator.hardware.lanes {
            writeln!(
                output,
                "| `{}` | `{:?}` / `{:?}` / `{:?}` | physical output {} to logical M{}{} |",
                actuator.declaration.id,
                lane.timer_channel,
                lane.pin,
                lane.dma,
                lane.physical_output,
                lane.logical_motor,
                if lane.timer_channel.is_complementary() {
                    " (complementary)"
                } else {
                    ""
                }
            )
            .unwrap();
        }
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | receive-only ESC telemetry; output enabled = `{}` |",
            actuator.declaration.id,
            actuator.telemetry.port.peripheral,
            actuator.telemetry.id,
            actuator.declaration.output_enabled
        )
        .unwrap();
    }
    for services in &resolved.golden_services {
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) / `{:?}` | ADC voltage/current observation on channels {}/{} |",
            services.declaration.id,
            services.adc.peripheral,
            services.adc.id,
            services.adc.dma,
            services.adc.voltage_channel,
            services.adc.current_channel,
        )
        .unwrap();
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | CPU-serviced SPI NOR at {} Hz |",
            services.declaration.id,
            services.flash.peripheral,
            services.flash.id,
            services.flash.frequency_hz,
        )
        .unwrap();
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | USB CDC `{}` / `{}` |",
            services.declaration.id,
            services.usb.peripheral,
            services.usb.id,
            services.usb.identity.product,
            services.usb.identity.serial_number,
        )
        .unwrap();
        writeln!(
            output,
            "| `{}` | `{:?}` (`{}`) | {} Hz I/O watchdog |",
            services.declaration.id,
            services.watchdog.peripheral,
            services.watchdog.id,
            services.declaration.watchdog_hz,
        )
        .unwrap();
    }
    let monotonic_hardware = resolved.monotonic.hardware_id().unwrap_or("SysTick");
    writeln!(
        output,
        "| `monotonic` | `{}` | {} Hz RTIC monotonic |",
        monotonic_hardware,
        resolved.monotonic.tick_hz()
    )
    .unwrap();
    if let Some(delay) = resolved.init_delay {
        writeln!(
            output,
            "| `init_delay` | `{}` | {} Hz synchronous initialization delay |",
            delay.hardware_id, delay.tick_hz
        )
        .unwrap();
    }
    output.push('\n');
}

fn sorted_tasks(resolved: &ResolvedApp) -> Vec<&super::resolve::ResolvedTask> {
    let mut tasks = resolved.tasks.iter().collect::<Vec<_>>();
    tasks.sort_by(|left, right| left.id.cmp(&right.id));
    tasks
}

fn safety_class(class: TaskSafetyClass) -> &'static str {
    match class {
        TaskSafetyClass::SafetyCritical => "safety-critical",
        TaskSafetyClass::SafetyRelated => "safety-related",
        TaskSafetyClass::NonSafetyCritical => "non-safety-critical",
    }
}

fn code_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rtic::resolve, target::app_composition::APP_COMPOSITION};

    #[test]
    fn selected_report_is_deterministic_and_contains_required_maps() {
        let resolved = resolve::resolve(&APP_COMPOSITION).unwrap();
        let first = render(&resolved);
        let second = render(&resolved);

        assert_eq!(first, second);
        for heading in [
            "## Safety-authority map",
            "## Task and priority map",
            "## Interrupt and dispatcher map",
            "## Spawn map",
            "## Authoritative safety-channel map",
            "## Task resource map",
            "## Resolved resource inventory",
            "## Persistent task-state ownership",
            "## Component and physical-resource map",
        ] {
            assert!(first.contains(heading), "missing {heading}");
        }
        assert!(first.contains("| `safety_master` | 16 | safety-critical |"));
        assert!(first.contains("| `actuator_output` | 15 | safety-critical |"));
        assert!(first.contains("| `control_to_actuator` | `MotorCommand` | 3 |"));
        assert!(first.contains("independently disable output"));
        assert!(first.contains("Tim1Ch3N"));
        assert!(first.contains("output enabled = `false`"));
        assert!(first.contains("ADC voltage/current observation"));
        assert!(first.contains("CPU-serviced SPI NOR"));
        assert!(first.contains("USB CDC `FerroWasp Foxeer Debug`"));
        assert!(first.contains("8000 Hz I/O watchdog"));
        assert!(first.contains("not target, electrical, bench, or flight evidence"));
    }
}

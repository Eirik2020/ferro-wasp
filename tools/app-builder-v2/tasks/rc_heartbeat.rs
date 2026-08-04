use crate::task::{TaskDefinition, duration, rc_input_snapshot};

crate::app_task! {
    pub const RC_HEARTBEAT: TaskDefinition = TaskDefinition::asynchronous("rc_heartbeat")
        .with_parameters(&[duration("report_interval")])
        .with_shared(&[rc_input_snapshot("rc_input")]);

    async fn rc_heartbeat(mut cx: rc_heartbeat::Context) {
        loop {
            Mono::delay(cx.config.report_interval).await;
            let snapshot = cx.shared.rc_input.lock(|snapshot| *snapshot);
            if snapshot.has_valid_frame {
                defmt::info!(
                    "RC channels: [{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}], d1={}, d2={}, frame_lost={}, failsafe={}, valid={}, errors={}",
                    snapshot.channels[0], snapshot.channels[1], snapshot.channels[2],
                    snapshot.channels[3], snapshot.channels[4], snapshot.channels[5],
                    snapshot.channels[6], snapshot.channels[7], snapshot.channels[8],
                    snapshot.channels[9], snapshot.channels[10], snapshot.channels[11],
                    snapshot.channels[12], snapshot.channels[13], snapshot.channels[14],
                    snapshot.channels[15], snapshot.digital_channel_1,
                    snapshot.digital_channel_2, snapshot.frame_lost, snapshot.failsafe,
                    snapshot.valid_frames, snapshot.parse_errors,
                );
            } else {
                defmt::info!("RC heartbeat: no RC frame (errors={})", snapshot.parse_errors);
            }
        }
    }
}

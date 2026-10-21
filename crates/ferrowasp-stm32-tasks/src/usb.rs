//! The USB CDC port: the storage and configuration command line (or, with
//! the MSPv2 configurator, its RPC), bench motor requests passed to the
//! safety master, and the periodic `FWDBG1` status line.

#[cfg(not(feature = "mspv2_configurator"))]
use crate::app::active_usb_debug_imu_kind;
use crate::app::{
    ConfiguratorUsbState, FlashRpcCommandProducer, FlashRpcResponseConsumer, MspCommonInfoFn,
};
#[cfg(feature = "mspv2_configurator")]
use crate::app::{handle_msp_packet, stage_rpc_response};
use crate::prelude::backend::usb_serial::{BufferedUsbCdcSerial, UsbCdcDevice, UsbDeviceState};
use crate::prelude::*;
#[cfg(not(feature = "mspv2_configurator"))]
use crate::snapshots::USB_DEBUG_DUE;
use crate::snapshots::USB_HOST_CONNECTED;
#[cfg(not(feature = "mspv2_configurator"))]
use ferrowasp_cli::usb_debug;

/// Serve the USB port on each OTG interrupt. The header line, whether this
/// firmware can drive motors, the control-loop rate the status line reports,
/// and the board's MSPv2 identity are configuration.
#[allow(clippy::needless_return)]
#[ferroforge::task(
    local = [
        usb_dev: UsbCdcDevice,
        usb_serial: BufferedUsbCdcSerial,
        usb_header_sent: bool,
        flash_command_producer: flash_task::CommandProducer,
        flash_response_consumer: flash_task::ResponseConsumer,
        flash_rpc_command_producer: FlashRpcCommandProducer,
        flash_rpc_response_consumer: FlashRpcResponseConsumer,
        flash_command_parser: flash_task::CommandParser = flash_task::CommandParser::new(),
        flash_pending_response: Option<flash_task::ResponseFrame> = None,
        configurator_usb: ConfiguratorUsbState,
    ],
    shared = [ledger: Ledger],
    spawn = [safety_master(event: safety::SafetyEvent)],
    config = [
        usb_debug_header: &'static [u8],
        actuator_output_enabled: bool,
        control_loop_hz: u32,
        msp_common_info: MspCommonInfoFn,
        rpc_device_info: crate::RpcDeviceInfoFn,
        build_identity: crate::BuildIdentity,
    ],
    monotonic = Mono,
)]
pub fn usb_fs(mut cx: usb_fs::Context) {
    let usb_dev = cx.local.usb_dev;
    let serial = cx.local.usb_serial;

    let _ = usb_dev.poll(&mut [serial]);

    let mut rx_buf = [0u8; 64];
    let read_len = serial.read(&mut rx_buf).unwrap_or(0);

    #[cfg(not(feature = "mspv2_configurator"))]
    for byte in &rx_buf[..read_len] {
        let Some(parsed) = cx.local.flash_command_parser.ingest(*byte) else {
            continue;
        };
        match parsed {
            Ok(flash_task::StorageCommand::Live) => {
                let live = cx.shared.ledger.lock(|ledger| crate::views::live(ledger));
                let line = usb_debug::format_live(live);
                let _ = serial.write(line.as_bytes());
            }
            Ok(flash_task::StorageCommand::Rc) => {
                let channels = cx
                    .shared
                    .ledger
                    .lock(|ledger| crate::views::rc_channels(ledger));
                for line in usb_debug::format_rc(channels) {
                    let _ = serial.write(line.as_bytes());
                }
            }
            Ok(flash_task::StorageCommand::Esc) => {
                let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
                let esc = cx
                    .shared
                    .ledger
                    .lock(|ledger| crate::views::esc(ledger, now_ms));
                for line in usb_debug::format_esc(esc) {
                    let _ = serial.write(line.as_bytes());
                }
            }
            Ok(flash_task::StorageCommand::Build) => {
                for line in usb_debug::format_build(CONFIG::BUILD_IDENTITY) {
                    let _ = serial.write(line.as_bytes());
                }
            }
            // Bench motor requests go straight to the safety master, which
            // outranks this task and decides before `spawn` returns. The
            // check here only gives the host a reason; the safety master
            // and the actuator each re-check before a motor moves.
            Ok(flash_task::StorageCommand::Motor(request)) => {
                let (armed, arm_high) = cx
                    .shared
                    .ledger
                    .lock(|ledger| (ledger.safety().value.armed, ledger.rc().value.arm_high));
                let precheck = safety::validate_bench_motor_request(
                    request,
                    armed,
                    false,
                    arm_high,
                    CONFIG::ACTUATOR_OUTPUT_ENABLED,
                );
                let reply: &[u8] = match precheck {
                    Ok(()) => match cx
                        .spawn
                        .safety_master(safety::SafetyEvent::BenchMotor(request))
                    {
                        Ok(()) => b"OK motor\r\n",
                        Err(_) => b"ERR motor safety master busy\r\n",
                    },
                    Err(safety::BenchMotorRefusal::Armed) => b"ERR motor armed\r\n",
                    Err(safety::BenchMotorRefusal::ArmSwitchHigh) => {
                        b"ERR motor arm switch is on\r\n"
                    }
                    Err(safety::BenchMotorRefusal::OutputDisabled) => {
                        b"ERR motor output disabled in this firmware\r\n"
                    }
                    Err(_) => b"ERR motor refused\r\n",
                };
                let _ = serial.write(reply);
            }
            Ok(command) => {
                if cx.local.flash_command_producer.enqueue(command).is_err() {
                    let _ = serial.write(b"ERR command queue full\r\n");
                }
            }
            Err(_) => {
                let _ = serial.write(b"ERR invalid command; type help\r\n");
            }
        }
    }

    #[cfg(feature = "mspv2_configurator")]
    for byte in &rx_buf[..read_len] {
        if let Ok(Some(packet)) = cx.local.configurator_usb.parser.parse(*byte) {
            handle_msp_packet(
                packet,
                cx.local.configurator_usb,
                cx.local.flash_rpc_command_producer,
                CONFIG::MSP_COMMON_INFO,
                CONFIG::RPC_DEVICE_INFO,
            );
        }
    }

    // Arming is refused while a computer has the board configured.
    USB_HOST_CONNECTED.store(
        usb_dev.state() == UsbDeviceState::Configured,
        Ordering::Release,
    );
    if usb_dev.state() != UsbDeviceState::Configured {
        *cx.local.usb_header_sent = false;
        #[cfg(not(feature = "mspv2_configurator"))]
        cx.local.flash_command_parser.clear();
        #[cfg(feature = "mspv2_configurator")]
        {
            cx.local.configurator_usb.parser.clear();
            cx.local.configurator_usb.pending_tx.clear();
        }
        return;
    }

    if serial.flush().is_err() {
        return;
    }

    #[cfg(feature = "mspv2_configurator")]
    {
        if !cx.local.configurator_usb.pending_tx.is_pending()
            && let Some(response) = cx.local.flash_rpc_response_consumer.dequeue()
        {
            let _ = stage_rpc_response(cx.local.configurator_usb, &response);
        }
        if cx.local.configurator_usb.pending_tx.is_pending()
            && let Ok(written) = serial.write(cx.local.configurator_usb.pending_tx.remaining())
        {
            cx.local.configurator_usb.pending_tx.advance(written);
        }
        return;
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    if !*cx.local.usb_header_sent {
        if matches!(
            serial.write(CONFIG::USB_DEBUG_HEADER),
            Ok(written) if written == CONFIG::USB_DEBUG_HEADER.len()
        ) {
            *cx.local.usb_header_sent = true;
        }
        return;
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    {
        if cx.local.flash_pending_response.is_none() {
            *cx.local.flash_pending_response = cx.local.flash_response_consumer.dequeue();
        }
        if let Some(response) = cx.local.flash_pending_response.as_ref() {
            if matches!(
                serial.write(response.as_bytes()),
                Ok(written) if written == response.as_bytes().len()
            ) {
                *cx.local.flash_pending_response = None;
                cortex_m::peripheral::NVIC::pend(pac::Interrupt::OTG_FS);
            }
            return;
        }
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    if !USB_DEBUG_DUE.swap(false, Ordering::AcqRel) {
        return;
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    let snapshot = {
        let now = Mono::now().duration_since_epoch();
        cx.shared.ledger.lock(|ledger| {
            crate::views::status(
                ledger,
                // The status format carries 32-bit milliseconds, 49.7 days.
                now.to_millis() as u32,
                active_usb_debug_imu_kind(),
                CONFIG::CONTROL_LOOP_HZ,
                now.to_micros(),
            )
        })
    };
    #[cfg(not(feature = "mspv2_configurator"))]
    let Ok(line) = usb_debug::format_status(snapshot) else {
        USB_DEBUG_DUE.store(true, Ordering::Release);
        return;
    };

    #[cfg(not(feature = "mspv2_configurator"))]
    if !matches!(
        serial.write(line.as_bytes()),
        Ok(written) if written == line.len()
    ) {
        USB_DEBUG_DUE.store(true, Ordering::Release);
    }
}

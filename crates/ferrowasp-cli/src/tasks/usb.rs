//! The USB CDC port: the storage and configuration command line (or, with
//! the MSPv2 configurator, its RPC), bench motor requests passed to the
//! safety master, and the periodic `FWDBG1` status line.

use super::prelude::*;
use core::sync::atomic::AtomicBool;
use ferrowasp_border::inbound::USB_HOST_CONNECTED;
#[cfg(feature = "mspv2_configurator")]
use ferrowasp_mspv2 as mspv2;
use ferrowasp_stm32::usb_serial::UsbCdcPort;

#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcCommandProducer = flash_task::RpcCommandProducer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcCommandProducer = ();
#[cfg(feature = "mspv2_configurator")]
pub type FlashRpcResponseConsumer = flash_task::RpcResponseConsumer;
#[cfg(not(feature = "mspv2_configurator"))]
pub type FlashRpcResponseConsumer = ();

/// The board's MSPv2 identity for the common commands, configuration
/// supplied to the USB task; `()` without the MSPv2 configurator.
#[cfg(feature = "mspv2_configurator")]
pub type MspCommonInfoFn = fn(bool, bool) -> mspv2::commands::CommonInfo<'static>;
#[cfg(not(feature = "mspv2_configurator"))]
pub type MspCommonInfoFn = ();

/// The USB configurator's parser and pending reply.
#[cfg(feature = "mspv2_configurator")]
pub struct ConfiguratorUsbState {
    pub parser: mspv2::MspParser,
    pub pending_tx: mspv2::EncodedFrame,
    pub rpc_payload: [u8; mspv2::MAX_PAYLOAD_LEN],
}

#[cfg(feature = "mspv2_configurator")]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self {
            parser: mspv2::MspParser::new(),
            pending_tx: mspv2::EncodedFrame::new(),
            rpc_payload: [0; mspv2::MAX_PAYLOAD_LEN],
        }
    }
}

#[cfg(not(feature = "mspv2_configurator"))]
pub struct ConfiguratorUsbState;

#[cfg(not(feature = "mspv2_configurator"))]
impl ConfiguratorUsbState {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ConfiguratorUsbState {
    fn default() -> Self {
        Self::new()
    }
}

/// `value`, cut or zero-padded to `N` bytes.
#[cfg(feature = "mspv2_configurator")]
pub fn fixed_bytes<const N: usize>(value: &[u8]) -> [u8; N] {
    let mut output = [0u8; N];
    let len = value.len().min(N);
    output[..len].copy_from_slice(&value[..len]);
    output
}

#[cfg(feature = "mspv2_configurator")]
pub fn stage_rpc_response(
    state: &mut ConfiguratorUsbState,
    response: &mspv2::rpc::RpcResponse,
) -> bool {
    let ConfiguratorUsbState {
        pending_tx,
        rpc_payload,
        ..
    } = state;
    let Ok(payload) = mspv2::rpc::encode_response(response, rpc_payload) else {
        return false;
    };
    pending_tx
        .set(
            mspv2::MspDirection::FromFlightController,
            0,
            mspv2::rpc::MSP2_FWSP_RPC,
            payload,
        )
        .is_ok()
}

/// Answer one MSPv2 packet from the USB host: the common commands from the
/// board's identity, `Hello` from its device information, both with the arm
/// state and IMU readiness the ledger shows, and every other RPC request
/// handed to the flash manager.
#[cfg(feature = "mspv2_configurator")]
pub fn handle_msp_packet(
    packet: mspv2::MspPacket,
    state: &mut ConfiguratorUsbState,
    rpc_commands: &mut flash_task::RpcCommandProducer,
    common_info: MspCommonInfoFn,
    device_info: crate::tasks::flash::RpcDeviceInfoFn,
    armed: bool,
    imu_ready: bool,
) {
    if packet.direction != mspv2::MspDirection::ToFlightController || state.pending_tx.is_pending()
    {
        return;
    }

    let mut common_payload = [0u8; 96];
    if let Some(len) = mspv2::commands::encode_common_payload(
        &packet,
        &common_info(armed, imu_ready),
        &mut common_payload,
    ) {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::FromFlightController,
            packet.flags,
            packet.function,
            &common_payload[..len],
        );
        return;
    }

    if packet.function != mspv2::rpc::MSP2_FWSP_RPC {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    }

    let Ok(request) = mspv2::rpc::decode_request(packet.payload()) else {
        let _ = state.pending_tx.set(
            mspv2::MspDirection::Error,
            packet.flags,
            packet.function,
            &[],
        );
        return;
    };
    if request.protocol_version != mspv2::rpc::FWSP_RPC_VERSION {
        let response = mspv2::rpc::error(
            request.request_id,
            mspv2::rpc::DeviceError::UnsupportedProtocolVersion,
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    if request.operation == mspv2::rpc::Request::Hello {
        let response = mspv2::rpc::ok(
            request.request_id,
            mspv2::rpc::Response::Hello(device_info(armed)),
        );
        let _ = stage_rpc_response(state, &response);
        return;
    }
    let request_id = request.request_id;
    if rpc_commands.enqueue(request).is_err() {
        let response = mspv2::rpc::error(request_id, mspv2::rpc::DeviceError::Busy);
        let _ = stage_rpc_response(state, &response);
    }
}

/// The heartbeat sets it every two seconds, and the USB task writes a status
/// line and clears it; both are non-critical.
pub(crate) static STATUS_LINE_DUE: AtomicBool = AtomicBool::new(false);
#[cfg(not(feature = "mspv2_configurator"))]
use crate::usb_debug;

/// Serve the USB port on each OTG interrupt. The header line, whether this
/// firmware can drive motors, the control-loop rate the status line reports,
/// and the board's MSPv2 identity are configuration.
#[allow(clippy::needless_return)]
#[ferroforge::task(
    bounds = [usb_cdc: UsbCdcPort],
    local = [
        usb_cdc,
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
        rpc_device_info: crate::tasks::flash::RpcDeviceInfoFn,
        build_identity: crate::usb_debug::BuildIdentity,
        wake_usb: fn(),
    ],
    monotonic = Mono,
)]
pub fn usb_fs(mut cx: usb_fs::Context) {
    let serial = cx.local.usb_cdc;

    serial.poll();

    let mut rx_buf = [0u8; 64];
    let read_len = serial.read(&mut rx_buf).unwrap_or(0);

    #[cfg(not(feature = "mspv2_configurator"))]
    for byte in &rx_buf[..read_len] {
        let Some(parsed) = cx.local.flash_command_parser.ingest(*byte) else {
            continue;
        };
        match parsed {
            Ok(flash_task::StorageCommand::Live) => {
                let live = cx
                    .shared
                    .ledger
                    .lock(|ledger| crate::tasks::views::live(ledger));
                let line = usb_debug::format_live(live);
                let _ = serial.write(line.as_bytes());
            }
            Ok(flash_task::StorageCommand::Rc) => {
                let channels = cx
                    .shared
                    .ledger
                    .lock(|ledger| crate::tasks::views::rc_channels(ledger));
                for line in usb_debug::format_rc(channels) {
                    let _ = serial.write(line.as_bytes());
                }
            }
            Ok(flash_task::StorageCommand::Esc) => {
                let now_ms = Mono::now().duration_since_epoch().to_millis() as u32;
                let esc = cx
                    .shared
                    .ledger
                    .lock(|ledger| crate::tasks::views::esc(ledger, now_ms));
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
            let (armed, imu_ready) = cx
                .shared
                .ledger
                .lock(|ledger| (ledger.safety().value.armed, ledger.identity().imu_ready));
            handle_msp_packet(
                packet,
                cx.local.configurator_usb,
                cx.local.flash_rpc_command_producer,
                CONFIG::MSP_COMMON_INFO,
                CONFIG::RPC_DEVICE_INFO,
                armed,
                imu_ready,
            );
        }
    }

    // Arming is refused while a computer has the board configured.
    USB_HOST_CONNECTED.store(serial.configured(), Ordering::Release);
    if !serial.configured() {
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
                (CONFIG::WAKE_USB)();
            }
            return;
        }
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    if !STATUS_LINE_DUE.swap(false, Ordering::AcqRel) {
        return;
    }

    #[cfg(not(feature = "mspv2_configurator"))]
    let snapshot = {
        let now = Mono::now().duration_since_epoch();
        cx.shared.ledger.lock(|ledger| {
            crate::tasks::views::status(
                ledger,
                // The status format carries 32-bit milliseconds, 49.7 days.
                now.to_millis() as u32,
                CONFIG::CONTROL_LOOP_HZ,
                now.to_micros(),
            )
        })
    };
    #[cfg(not(feature = "mspv2_configurator"))]
    let Ok(line) = usb_debug::format_status(snapshot) else {
        STATUS_LINE_DUE.store(true, Ordering::Release);
        return;
    };

    #[cfg(not(feature = "mspv2_configurator"))]
    if !matches!(
        serial.write(line.as_bytes()),
        Ok(written) if written == line.len()
    ) {
        STATUS_LINE_DUE.store(true, Ordering::Release);
    }
}

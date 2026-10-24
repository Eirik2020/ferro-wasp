// The boundary between the interface and the controller.
//
// Everything the UI can ask for is declared here once, so the same screens run
// against a real device through Tauri and against a scripted device in a
// browser. `src/mock.ts` is the second implementation, and it is what makes
// the interface developable without a flight controller on the desk.
//
// Field names are snake_case because that is what the Rust side puts on the
// wire, and the CLI's `--format json` output depends on it. Matching it here
// removes a mapping layer that would otherwise have to be kept in step with
// every protocol change.

/** Mirrors `StatusSnapshot` in ferro-configurator-core. */
export interface StatusSnapshot {
  uptime_ms: number;
  imu: string;
  imu_ready: boolean;
  rc_valid: boolean;
  armable: boolean;
  throttle: number;
  arm_switch: boolean;
  armed: boolean;
  battery_decivolts: number;
  /** Absent or null on firmware that does not report it. */
  imu_stale?: boolean | null;
  /** Raw receiver channels; null until the firmware reports them. */
  channels?: number[] | null;
}

/** Mirrors `PrearmCheck` in ferro-configurator-core. */
export interface PrearmCheck {
  id: string;
  label: string;
  state: "pass" | "fail" | "unknown";
  hint: string;
}

/** Mirrors `ConfigFallback` in ferro-configurator-core. */
export type ConfigFallback = "none" | "damaged" | "invalid" | "unreadable";

/** Mirrors `Safety` in ferro-configurator-bridge. */
export interface Safety {
  status: StatusSnapshot;
  /** False whenever the controller reports itself armed. */
  writes_allowed: boolean;
  /** Arming preconditions, in the order the firmware evaluates them. */
  checks: PrearmCheck[];
  /** Whether it booted without its last save; null when it cannot say. */
  config_fallback: ConfigFallback | null;
}

export interface AxisPid {
  p: number;
  i: number;
  d: number;
}

/** Mirrors `FerroConfig`. Optional fields are absent on older firmware. */
export interface FerroConfig {
  schema_version: number;
  roll: AxisPid;
  pitch: AxisPid;
  yaw: AxisPid;
  imu_lpf_hz: number;
  log_rate_divisor: number;
  rc_deadband?: number;
  roll_center_rate?: number;
  roll_max_rate?: number;
  roll_expo?: number;
  pitch_center_rate?: number;
  pitch_max_rate?: number;
  pitch_expo?: number;
  yaw_center_rate?: number;
  yaw_max_rate?: number;
  yaw_expo?: number;
  /** Channels for roll, pitch, throttle and yaw, like 1234 for AETR. */
  rc_map?: number;
  /** One-based arm switch channel, 5-16. */
  rc_arm_channel?: number;
  /** Applies after the controller reboots. */
  rc_protocol?: "sbus" | "crsf";
  /**
   * Motor order: for logical motors 1-4 (rear-right, front-right, rear-left,
   * front-left), which board output each drives. 1234 keeps the wiring.
   */
  motor_map?: number;
}

/** Mirrors `LiveSnapshot`: the fast answer polled for the 3D view. */
export interface LiveSnapshot {
  armed: boolean;
  arm_switch: boolean;
  /** Roll, pitch and yaw in degrees. */
  attitude_deg: [number, number, number];
  /** Bit per physical motor output commanded above zero. */
  active_outputs: number;
}

/** The functions a serial port can serve, as the firmware names them. */
export const SERIAL_FUNCTIONS = ["none", "rc", "osd", "esc_telemetry", "configurator"] as const;

/** Mirrors `SerialPortBinding` in ferro-configurator-core. */
export interface SerialPortBinding {
  /** The UART by its number on the chip, such as `uart3`. */
  port: string;
  function: string;
}

/**
 * Mirrors `SerialBindings`. The staged table: the controller boots with it
 * after the next save and reboot.
 */
export interface SerialBindings {
  /** False while the board's defaults apply. */
  saved: boolean;
  ports: SerialPortBinding[];
}

export interface PortInfo {
  port: string;
  is_ferrowasp: boolean;
}

export interface LogInfo {
  used_pages: number;
  next_flight: number;
  total_pages: number;
  writable: boolean;
}

export interface FlightSpan {
  flight_id: number;
  first_page: number;
  last_page: number;
  pages: number;
  bytes: number;
}

export interface CatalogEntry {
  boot_session: number | null;
  flight: FlightSpan;
}

export interface FlightCatalog {
  storage: LogInfo;
  flights: CatalogEntry[];
}

/**
 * Why a call failed.
 *
 * `refused` is not a malfunction: it is the session declining to write while
 * the controller reports itself armed. The interface presents it as a safety
 * state, never as an error to retry.
 */
export type BridgeErrorKind = "notConnected" | "lost" | "refused" | "device";

export class BridgeError extends Error {
  readonly kind: BridgeErrorKind;

  constructor(kind: BridgeErrorKind, message: string) {
    super(message);
    this.name = "BridgeError";
    this.kind = kind;
  }
}

/** Shapes a rejected Tauri or mock call into a `BridgeError`. */
export function toBridgeError(value: unknown): BridgeError {
  if (value instanceof BridgeError) {
    return value;
  }
  if (typeof value === "object" && value !== null && "kind" in value) {
    const payload = value as { kind: BridgeErrorKind; reason?: string; message?: string };
    const text =
      payload.reason ?? payload.message ?? "the controller rejected the request";
    return new BridgeError(payload.kind, text);
  }
  return new BridgeError("device", String(value));
}

export interface DownloadProgress {
  page: number;
  total: number;
}

export interface SyncProgress {
  flight: number;
  page: number;
  total: number;
}

/** One flight a log sync stored and the controller acknowledged. */
export interface SyncedFlight {
  flight_id: number;
  pages: number;
  output: string;
  /** Already complete from an earlier, interrupted sync. */
  already_stored: boolean;
}

export interface DownloadSummary {
  pages: number;
  bytes: number;
  path: string;
}

/** Where the accelerometer reads level, milli-g per drone axis. */
export interface AccelLevel {
  x_mg: number;
  y_mg: number;
  z_mg: number;
}

/** One step of a flash, as the Rust session reports it. */
export type FlashStage =
  | { stage: "entering_bootloader"; port: string }
  | { stage: "image_validated"; bytes: number; base_address: number }
  | { stage: "dfu_detected"; device: string }
  | { stage: "programming"; bytes: number }
  | { stage: "programmed" }
  | { stage: "restarting" };

export interface FlashReport {
  board: string;
  firmware: string;
  /** The bundled release's version, when the bundled image was flashed. */
  release: string | null;
  bytes_written: number;
  base_address: number;
  /** The port the controller came back on, already reconnected. */
  restarted_port: string | null;
}

export interface Api {
  ports(): Promise<PortInfo[]>;
  /** `null` selects the only FerroWasp port, matching the CLI's auto mode. */
  connect(port: string | null): Promise<string>;
  disconnect(): Promise<void>;
  /** Fresh: waits for the next status line. Use before acting on it. */
  safety(): Promise<Safety>;
  /** The last status line received; cheap enough to poll for display. */
  safetyDisplay(): Promise<Safety>;
  live(): Promise<LiveSnapshot>;
  /** The receiver's sixteen channels in µs, as fresh as the last frame. */
  rcChannels(): Promise<number[]>;
  /**
   * Idles one logical motor for one short firmware lease. Call about every
   * 100 ms while the button is held; stop calling and the motor stops.
   */
  motorSpin(motor: number): Promise<void>;
  motorStop(): Promise<void>;
  /** Sets and saves one ESC's spin direction. Motors must be stopped. */
  motorDirection(motor: number, reversed: boolean): Promise<void>;
  readConfig(): Promise<FerroConfig>;
  /** Refused unless the controller reports itself disarmed. */
  applyConfig(config: FerroConfig): Promise<FerroConfig>;
  serialBindings(): Promise<SerialBindings>;
  /** Refused unless disarmed. Takes effect when the controller reboots. */
  applySerialBinding(port: string, func: string): Promise<SerialBindings>;
  flights(): Promise<FlightCatalog>;
  downloadFlight(
    flightId: number | "latest",
    output: string,
    onProgress: (progress: DownloadProgress) => void,
  ): Promise<DownloadSummary>;
  /**
   * Stores every flight no host has stored into `directory`, one file each,
   * and acknowledges them so the controller may later erase its log.
   */
  syncFlights(directory: string, onProgress: (progress: SyncProgress) => void): Promise<SyncedFlight[]>;
  /**
   * Restarts the controller, or into its ROM bootloader with `dfu`, and
   * closes the connection. Refused while armed or while storage is busy.
   */
  reboot(dfu: boolean): Promise<void>;
  /**
   * Flashes `path`, or the bundled image when `null`. A connected, disarmed
   * controller is restarted into its bootloader first; one already in its
   * bootloader is flashed as it is. Reconnects when the controller returns.
   */
  flash(path: string | null, onStage: (stage: FlashStage) => void): Promise<FlashReport>;
  /** Asks the user for a firmware file; `null` when they cancel. */
  chooseFirmwareFile(): Promise<string | null>;
  /** The stored accelerometer level. */
  accelLevel(): Promise<AccelLevel>;
  /**
   * Calibrates the accelerometer's level and saves it. The craft must be
   * disarmed, still and within 10 degrees of level; the controller refuses
   * otherwise.
   */
  calibrateAccelerometer(): Promise<AccelLevel>;
  /**
   * Acknowledges that the controller booted without its last save, so it
   * may arm. Refused unless disarmed.
   */
  acknowledgeConfigFallback(): Promise<void>;
}

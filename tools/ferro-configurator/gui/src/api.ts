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
}

/** Mirrors `Safety` in ferro-configurator-bridge. */
export interface Safety {
  status: StatusSnapshot;
  /** False whenever the controller reports itself armed. */
  writes_allowed: boolean;
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
}

/** The functions a serial port can serve, as the firmware names them. */
export const SERIAL_FUNCTIONS = ["none", "rc", "osd", "esc_telemetry"] as const;

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
export type BridgeErrorKind = "notConnected" | "refused" | "device";

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

export interface DownloadSummary {
  pages: number;
  bytes: number;
  path: string;
}

export interface Api {
  ports(): Promise<PortInfo[]>;
  /** `null` selects the only FerroWasp port, matching the CLI's auto mode. */
  connect(port: string | null): Promise<string>;
  disconnect(): Promise<void>;
  safety(): Promise<Safety>;
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
}

// A scripted controller, so the interface can be built and demonstrated with
// no hardware attached.
//
// It answers with the documented Foxeer baseline - P-only 2.5/2.5/2.0 with
// every I and D at zero, rates 70/300/0.50 on roll and pitch and 70/200/0.50
// on yaw, deadband 8 - because a mock that returns plausible-but-wrong values
// teaches the wrong thing about what the aircraft is configured to do.

import {
  BridgeError,
  type Api,
  type FerroConfig,
  type FlightCatalog,
  type LiveSnapshot,
  type PrearmCheck,
  type PortInfo,
  type Safety,
  type SerialBindings,
  type DownloadProgress,
  type DownloadSummary,
  type SyncedFlight,
  type SyncProgress,
} from "./api";

/** The reviewed Foxeer baseline. */
function baselineConfig(): FerroConfig {
  return {
    schema_version: 2,
    roll: { p: 2.5, i: 0, d: 0 },
    pitch: { p: 2.5, i: 0, d: 0 },
    yaw: { p: 2.0, i: 0, d: 0 },
    imu_lpf_hz: 50.8,
    log_rate_divisor: 1,
    rc_deadband: 8,
    roll_center_rate: 70,
    roll_max_rate: 300,
    roll_expo: 0.5,
    pitch_center_rate: 70,
    pitch_max_rate: 300,
    pitch_expo: 0.5,
    yaw_center_rate: 70,
    yaw_max_rate: 200,
    yaw_expo: 0.5,
    rc_map: 1234,
    rc_arm_channel: 9,
    rc_protocol: "sbus",
    motor_map: 1234,
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * The scripted device.
 *
 * `armed` is public so the demo can flip it and show the interface refusing a
 * write - the one behaviour worth being able to exercise on demand, since it
 * is the hard one to reproduce safely on a real aircraft.
 */
export class MockApi implements Api {
  armed = false;
  /** Lets the demo show the checklist failing on a lost receiver link. */
  radioOn = true;
  /** Lets the demo show a motor test refused with the arm switch on. */
  armSwitch = false;
  /** The scripted board's wiring: logical motor i is on output BOARD[i]. */
  private static readonly BOARD = [1, 2, 3, 4];
  private spinningOutput = 0;
  private spinUntil = 0;
  /** Directions the scripted ESCs have saved, by physical output. */
  readonly escReversed = [false, false, false, false];
  private aux = 988;
  private connected = false;
  private config = baselineConfig();
  // The Foxeer F405 V2's default wiring.
  private bindings: SerialBindings = {
    saved: false,
    ports: [
      { port: "uart1", function: "esc_telemetry" },
      { port: "uart2", function: "rc" },
      { port: "uart3", function: "configurator" },
      { port: "uart4", function: "osd" },
    ],
  };

  async ports(): Promise<PortInfo[]> {
    await sleep(80);
    return [
      { port: "/dev/ttyACM0", is_ferrowasp: true },
      { port: "/dev/ttyUSB0", is_ferrowasp: false },
    ];
  }

  async connect(port: string | null): Promise<string> {
    await sleep(150);
    this.connected = true;
    return `mock FerroWasp on ${port ?? "/dev/ttyACM0"}`;
  }

  async disconnect(): Promise<void> {
    this.connected = false;
  }

  private requireConnection(): void {
    if (!this.connected) {
      throw new BridgeError("notConnected", "not connected to a controller");
    }
  }

  /** Flips aux channel 6, so the find-a-control helper has something to find. */
  flipAux(): void {
    this.aux = this.aux === 988 ? 2012 : 988;
  }

  async safety(): Promise<Safety> {
    this.requireConnection();
    await sleep(20);
    const channels = this.channels();
    // Scripted, not computed: the real list comes from the Rust library.
    const check = (id: string, label: string, pass: boolean | null, hint: string): PrearmCheck => ({
      id,
      label,
      state: pass === null ? "unknown" : pass ? "pass" : "fail",
      hint,
    });
    return {
      status: {
        uptime_ms: 128_400,
        imu: "icm42688p",
        imu_ready: true,
        rc_valid: this.radioOn,
        armable: this.radioOn && !this.armed,
        throttle: this.armed ? 112 : 0,
        arm_switch: this.armed || this.armSwitch,
        armed: this.armed,
        battery_decivolts: 251,
        imu_stale: false,
        channels,
      },
      writes_allowed: !this.armed,
      checks: [
        check(
          "rc_link",
          "Receiver link",
          this.radioOn,
          "No valid receiver frames. Turn the radio on, check it is bound, and check the receiver wiring.",
        ),
        check(
          "rc_armable",
          "Arm switch reset",
          this.radioOn,
          "After the link connects the arm switch must be seen off once. Flip it off, then on.",
        ),
        check("throttle_low", "Throttle low", true, "Move the throttle stick fully down."),
        check(
          "usb_unplugged",
          "USB unplugged",
          false,
          "The controller never arms while USB is connected to a computer. Unplug it to arm.",
        ),
        check("imu_detected", "Gyro detected", true, ""),
        check("imu_ready", "Gyro running", true, ""),
        check("gyro_calibrated", "Gyro calibrated", null, "Not reported over USB yet."),
        check("imu_fresh", "Gyro data fresh", true, ""),
        check("battery", "Flight battery connected", true, ""),
        check("esc_idle", "ESCs report idle", null, "Not reported over USB yet."),
      ],
    };
  }

  async safetyDisplay(): Promise<Safety> {
    return this.safety();
  }

  private outputFor(motor: number): number {
    const order = String(this.config.motor_map ?? 1234);
    const position = Number(order[motor - 1]);
    return MockApi.BOARD[position - 1] ?? 0;
  }

  private refuseUnlessBenchReady(): void {
    this.requireConnection();
    if (this.armed) {
      throw new BridgeError("refused", "the controller is armed; motor tests run only when disarmed");
    }
    if (this.armSwitch) {
      throw new BridgeError("refused", "turn the arm switch off before testing motors");
    }
  }

  /** Sticks drift gently so the bars visibly track a live radio. */
  private channels(): number[] | null {
    const t = Date.now() / 1000;
    const stick = (phase: number): number => Math.round(1500 + 120 * Math.sin(t + phase));
    // AETR with the arm switch on channel 9, every board's default.
    return this.radioOn
      ? [
          stick(0),
          stick(1.3),
          988,
          stick(2.1),
          this.aux,
          988,
          988,
          1500,
          this.armed ? 2012 : 988,
          988,
          988,
          988,
          988,
          988,
          988,
          988,
        ]
      : null;
  }

  async rcChannels(): Promise<number[]> {
    this.requireConnection();
    await sleep(5);
    return this.channels() ?? new Array<number>(16).fill(0);
  }

  async live(): Promise<LiveSnapshot> {
    this.requireConnection();
    await sleep(5);
    const t = Date.now() / 1000;
    const active = Date.now() < this.spinUntil ? 1 << (this.spinningOutput - 1) : 0;
    return {
      armed: this.armed,
      arm_switch: this.armSwitch || this.armed,
      // A slow hand-held wobble, so the view visibly follows the "board".
      attitude_deg: [18 * Math.sin(t * 0.9), 12 * Math.sin(t * 0.6 + 1), (t * 8) % 360],
      active_outputs: active,
    };
  }

  async motorSpin(motor: number): Promise<void> {
    this.refuseUnlessBenchReady();
    await sleep(4);
    this.spinningOutput = this.outputFor(motor);
    // The firmware's lease: the motor stops unless renewed within 250 ms.
    this.spinUntil = Date.now() + 250;
  }

  async motorStop(): Promise<void> {
    this.requireConnection();
    this.spinUntil = 0;
  }

  async motorDirection(motor: number, reversed: boolean): Promise<void> {
    this.refuseUnlessBenchReady();
    await sleep(60);
    const output = this.outputFor(motor);
    this.escReversed[output - 1] = reversed;
  }

  async readConfig(): Promise<FerroConfig> {
    this.requireConnection();
    await sleep(120);
    return structuredClone(this.config);
  }

  async applyConfig(config: FerroConfig): Promise<FerroConfig> {
    this.requireConnection();
    // The same gate the Rust session applies, so the interface is exercised
    // against a refusal rather than only against success.
    if (this.armed) {
      throw new BridgeError(
        "refused",
        "the controller is armed; disarm it before changing configuration",
      );
    }
    await sleep(300);
    this.config = structuredClone(config);
    return structuredClone(this.config);
  }

  async serialBindings(): Promise<SerialBindings> {
    this.requireConnection();
    await sleep(80);
    return structuredClone(this.bindings);
  }

  async applySerialBinding(port: string, func: string): Promise<SerialBindings> {
    this.requireConnection();
    if (this.armed) {
      throw new BridgeError(
        "refused",
        "the controller is armed; disarm it before changing configuration",
      );
    }
    const binding = this.bindings.ports.find((entry) => entry.port === port);
    if (!binding) {
      throw new BridgeError("device", `this board does not route \`${port}\``);
    }
    await sleep(200);
    binding.function = func;
    this.bindings.saved = true;
    return structuredClone(this.bindings);
  }

  async flights(): Promise<FlightCatalog> {
    this.requireConnection();
    await sleep(200);
    return {
      storage: {
        used_pages: 18_560,
        next_flight: 9,
        total_pages: 65_536,
        writable: true,
      },
      flights: [
        { boot_session: 1, flight: { flight_id: 1, first_page: 0, last_page: 2_642, pages: 2_643, bytes: 676_608 } },
        { boot_session: 4, flight: { flight_id: 8, first_page: 17_182, last_page: 18_560, pages: 1_379, bytes: 353_024 } },
      ],
    };
  }

  async downloadFlight(
    flightId: number | "latest",
    output: string,
    onProgress: (progress: DownloadProgress) => void,
  ): Promise<DownloadSummary> {
    this.requireConnection();
    const total = 1_379;
    for (let page = 0; page <= total; page += 97) {
      onProgress({ page: Math.min(page, total), total });
      await sleep(60);
    }
    onProgress({ page: total, total });
    return {
      pages: total,
      bytes: total * 256,
      path: output || `flight-${String(flightId)}.fwbb`,
    };
  }

  async syncFlights(directory: string, onProgress: (progress: SyncProgress) => void): Promise<SyncedFlight[]> {
    this.requireConnection();
    const flights = [
      { flight_id: 8, pages: 1_379, digest: "5e0a91c2" },
      { flight_id: 1, pages: 2_643, digest: "c4d07b13" },
    ];
    const synced: SyncedFlight[] = [];
    for (const { flight_id, pages, digest } of flights) {
      for (let page = 1; page <= pages; page += 211) {
        onProgress({ flight: flight_id, page, total: pages });
        await sleep(40);
      }
      onProgress({ flight: flight_id, page: pages, total: pages });
      synced.push({
        flight_id,
        pages,
        output: `${directory}/flight-${String(flight_id)}-${digest}.fwbb`,
        already_stored: false,
      });
    }
    return synced;
  }
}

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
  type PortInfo,
  type Safety,
  type DownloadProgress,
  type DownloadSummary,
} from "./api";

/** The reviewed Foxeer baseline. */
function baselineConfig(): FerroConfig {
  return {
    schema_version: 1,
    roll: { p: 2.5, i: 0, d: 0 },
    pitch: { p: 2.5, i: 0, d: 0 },
    yaw: { p: 2.0, i: 0, d: 0 },
    imu_lpf_alpha: 0.2,
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
  private connected = false;
  private config = baselineConfig();

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

  async safety(): Promise<Safety> {
    this.requireConnection();
    await sleep(40);
    return {
      status: {
        uptime_ms: 128_400,
        imu: "icm42688p",
        imu_ready: true,
        rc_valid: true,
        armable: !this.armed,
        throttle: this.armed ? 112 : 0,
        arm_switch: this.armed,
        armed: this.armed,
        battery_decivolts: 251,
      },
      writes_allowed: !this.armed,
    };
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
}

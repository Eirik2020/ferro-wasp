// The real device, reached through the Tauri command layer.
//
// Each method is a thin call onto a Rust command that delegates to
// ferro-configurator-bridge. Nothing here decides anything about safety: the
// disarmed gate lives in the Rust session, so it cannot be bypassed by a
// front-end bug or by anyone calling the commands directly.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  toBridgeError,
  type Api,
  type DownloadProgress,
  type DownloadSummary,
  type SyncedFlight,
  type SyncProgress,
  type FerroConfig,
  type FlightCatalog,
  type LiveSnapshot,
  type PortInfo,
  type Safety,
  type SerialBindings,
} from "./api";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toBridgeError(error);
  }
}

export class TauriApi implements Api {
  ports(): Promise<PortInfo[]> {
    return call<PortInfo[]>("list_ports");
  }

  connect(port: string | null): Promise<string> {
    return call<string>("connect", { port });
  }

  disconnect(): Promise<void> {
    return call<void>("disconnect");
  }

  safety(): Promise<Safety> {
    return call<Safety>("safety");
  }

  safetyDisplay(): Promise<Safety> {
    return call<Safety>("safety_display");
  }

  live(): Promise<LiveSnapshot> {
    return call<LiveSnapshot>("live");
  }

  motorSpin(motor: number): Promise<void> {
    return call<void>("motor_spin", { motor });
  }

  motorStop(): Promise<void> {
    return call<void>("motor_stop");
  }

  motorDirection(motor: number, reversed: boolean): Promise<void> {
    return call<void>("motor_direction", { motor, reversed });
  }

  readConfig(): Promise<FerroConfig> {
    return call<FerroConfig>("read_config");
  }

  applyConfig(config: FerroConfig): Promise<FerroConfig> {
    return call<FerroConfig>("apply_config", { config });
  }

  serialBindings(): Promise<SerialBindings> {
    return call<SerialBindings>("serial_bindings");
  }

  applySerialBinding(port: string, func: string): Promise<SerialBindings> {
    return call<SerialBindings>("apply_serial_binding", { port, function: func });
  }

  flights(): Promise<FlightCatalog> {
    return call<FlightCatalog>("flights");
  }

  async downloadFlight(
    flightId: number | "latest",
    output: string,
    onProgress: (progress: DownloadProgress) => void,
  ): Promise<DownloadSummary> {
    // The download runs for minutes on a full flight, so the Rust side emits
    // progress as events rather than blocking the call until it finishes.
    const stop = await listen<DownloadProgress>("download-progress", (event) => {
      onProgress(event.payload);
    });
    try {
      return await call<DownloadSummary>("download_flight", {
        flight: flightId,
        output,
      });
    } finally {
      stop();
    }
  }

  async syncFlights(directory: string, onProgress: (progress: SyncProgress) => void): Promise<SyncedFlight[]> {
    const stop = await listen<SyncProgress>("sync-progress", (event) => {
      onProgress(event.payload);
    });
    try {
      return await call<SyncedFlight[]>("sync_flights", { directory });
    } finally {
      stop();
    }
  }
}

/** True when running inside the desktop shell rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

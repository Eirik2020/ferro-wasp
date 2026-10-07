// The prototype interface.
//
// Four panels over one connection: the safety banner, the tuning form, the
// serial ports, and the flight log. The banner is not decoration - it is the thing that decides
// whether the form can be submitted at all, and it is deliberately the first
// thing on screen.

import {
  BridgeError,
  SERIAL_FUNCTIONS,
  toBridgeError,
  type Api,
  type FerroConfig,
  type PrearmCheck,
  type Safety,
  type SerialBindings,
} from "./api";
import { MockApi } from "./mock";
import { drawRates, type RateAxis } from "./rates";
import { ControlFinder, channelName, drawChannels } from "./receiver";
import { TauriApi, isTauri } from "./tauri";

const api: Api = isTauri() ? new TauriApi() : new MockApi();
const usingMock = !isTauri();

/** How often the banner and receiver re-read the controller while connected. */
const SAFETY_POLL_MS = 200;

let connected = false;
let safetyState: Safety | null = null;
let config: FerroConfig | null = null;
let bindings: SerialBindings | null = null;
let pollTimer: number | undefined;
let finder: ControlFinder | null = null;
let finderTimeout: number | undefined;

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) {
    throw new Error(`the page is missing #${id}`);
  }
  return found as T;
}

function setStatusLine(text: string, tone: "idle" | "busy" | "bad" = "idle"): void {
  const line = element("status-line");
  line.textContent = text;
  line.dataset["tone"] = tone;
}

/** Presents a failure, distinguishing a refusal from a malfunction. */
function report(error: unknown): void {
  const bridged = toBridgeError(error);
  if (bridged.kind === "refused") {
    setStatusLine(bridged.message, "bad");
    return;
  }
  if (bridged.kind === "notConnected") {
    setStatusLine("Not connected to a controller.", "idle");
    return;
  }
  setStatusLine(bridged.message, "bad");
}

// ---------------------------------------------------------------- safety ---

function renderSafety(): void {
  const banner = element("safety-banner");
  const detail = element("safety-detail");

  if (!connected || !safetyState) {
    banner.textContent = "No controller connected";
    banner.dataset["state"] = "unknown";
    detail.textContent = "";
    element("prearm-checks").replaceChildren();
    renderReceiver();
    updateWriteControls();
    return;
  }

  const { status, writes_allowed } = safetyState;
  banner.textContent = status.armed ? "ARMED — writes refused" : "Disarmed — writes allowed";
  banner.dataset["state"] = status.armed ? "armed" : "disarmed";

  const volts = (status.battery_decivolts / 10).toFixed(1);
  detail.textContent = [
    `IMU ${status.imu}${status.imu_ready ? "" : " (not ready)"}`,
    `RC ${status.rc_valid ? "valid" : "invalid"}`,
    `arm switch ${status.arm_switch ? "high" : "low"}`,
    `${status.armable ? "armable" : "not armable"}`,
    `throttle ${String(status.throttle)}`,
    `${volts} V`,
  ].join(" · ");

  if (!writes_allowed) {
    setStatusLine("Disarm the controller to change configuration.", "bad");
  }
  renderChecks(status.armed ? [] : safetyState.checks);
  renderReceiver();
  updateWriteControls();
}

/**
 * The pre-arm list. Only failures carry their hint, so a healthy quad reads
 * as a short column of ticks and a broken one says what to do next.
 */
function renderChecks(checks: PrearmCheck[]): void {
  const list = element("prearm-checks");
  list.replaceChildren();
  for (const check of checks) {
    const row = document.createElement("li");
    row.dataset["state"] = check.state;
    row.textContent = check.label;
    if (check.state !== "pass") {
      row.title = check.hint;
    }
    if (check.state === "fail") {
      const hint = document.createElement("span");
      hint.className = "hint";
      hint.textContent = check.hint;
      row.append(hint);
    }
    list.append(row);
  }
}

// ------------------------------------------------------------- receiver ---

function renderReceiver(): void {
  const unavailable = element("receiver-unavailable");
  const container = element("channels");
  const findButton = element<HTMLButtonElement>("find-control");
  const channels = connected ? safetyState?.status.channels : null;

  if (!channels) {
    container.replaceChildren();
    if (!connected) {
      unavailable.textContent = "Connect to see live channels.";
    } else if (safetyState?.status.rc_valid === false) {
      unavailable.textContent = "No receiver signal. Turn the radio on and check it is bound.";
    } else {
      unavailable.textContent = "This firmware does not report receiver channels over USB yet.";
    }
    findButton.disabled = true;
    return;
  }
  unavailable.textContent = "";
  findButton.disabled = finder !== null;
  drawChannels(container, channels, config);

  if (finder) {
    const found = finder.observe(channels);
    if (found !== null) {
      stopFinding(
        `That control is on channel ${String(found + 1)} (${channelName(found, config)} in the saved map).`,
      );
      container.children[found]?.classList.add("found");
    }
  }
}

function startFinding(): void {
  finder = new ControlFinder();
  for (const row of Array.from(element("channels").children)) {
    row.classList.remove("found");
  }
  element("find-result").textContent = "Flip the switch or move the stick you want to find…";
  element<HTMLButtonElement>("find-control").disabled = true;
  finderTimeout = window.setTimeout(() => {
    stopFinding("Nothing moved. Is the radio on and bound?");
  }, 10_000);
}

function stopFinding(message: string): void {
  window.clearTimeout(finderTimeout);
  finder = null;
  element("find-result").textContent = message;
  element<HTMLButtonElement>("find-control").disabled = !connected;
}

async function pollSafety(): Promise<void> {
  if (!connected) {
    return;
  }
  try {
    safetyState = await api.safety();
    renderSafety();
  } catch (error) {
    if (toBridgeError(error).kind === "notConnected") {
      connected = false;
    }
    renderSafety();
  }
}

// ------------------------------------------------------------ connection ---

async function refreshPorts(): Promise<void> {
  const select = element<HTMLSelectElement>("port-select");
  try {
    const ports = await api.ports();
    select.replaceChildren();
    const auto = new Option("Auto-detect", "");
    select.add(auto);
    for (const port of ports) {
      const label = port.is_ferrowasp ? `${port.port} (FerroWasp)` : port.port;
      select.add(new Option(label, port.port));
    }
    setStatusLine(`${String(ports.length)} serial port(s) found.`);
  } catch (error) {
    report(error);
  }
}

async function connect(): Promise<void> {
  const select = element<HTMLSelectElement>("port-select");
  const choice = select.value === "" ? null : select.value;
  setStatusLine("Connecting…", "busy");
  try {
    const description = await api.connect(choice);
    connected = true;
    element("device-description").textContent = description;
    setStatusLine(`Connected to ${description}.`);
    await pollSafety();
    await loadConfig();
    await loadPorts();
    pollTimer = window.setInterval(() => void pollSafety(), SAFETY_POLL_MS);
  } catch (error) {
    connected = false;
    report(error);
    renderSafety();
  }
}

async function disconnect(): Promise<void> {
  window.clearInterval(pollTimer);
  pollTimer = undefined;
  await api.disconnect();
  connected = false;
  if (finder) {
    stopFinding("");
  }
  safetyState = null;
  config = null;
  bindings = null;
  element("device-description").textContent = "";
  renderSafety();
  renderConfig();
  renderPorts();
  setStatusLine("Disconnected.");
}

// --------------------------------------------------------------- tuning ---

/** Every editable field, as [input id, reader, writer]. */
const NUMERIC_FIELDS: ReadonlyArray<{
  id: string;
  read: (c: FerroConfig) => number | undefined;
  write: (c: FerroConfig, value: number) => void;
}> = [
  { id: "roll-p", read: (c) => c.roll.p, write: (c, v) => { c.roll.p = v; } },
  { id: "roll-i", read: (c) => c.roll.i, write: (c, v) => { c.roll.i = v; } },
  { id: "roll-d", read: (c) => c.roll.d, write: (c, v) => { c.roll.d = v; } },
  { id: "pitch-p", read: (c) => c.pitch.p, write: (c, v) => { c.pitch.p = v; } },
  { id: "pitch-i", read: (c) => c.pitch.i, write: (c, v) => { c.pitch.i = v; } },
  { id: "pitch-d", read: (c) => c.pitch.d, write: (c, v) => { c.pitch.d = v; } },
  { id: "yaw-p", read: (c) => c.yaw.p, write: (c, v) => { c.yaw.p = v; } },
  { id: "yaw-i", read: (c) => c.yaw.i, write: (c, v) => { c.yaw.i = v; } },
  { id: "yaw-d", read: (c) => c.yaw.d, write: (c, v) => { c.yaw.d = v; } },
  { id: "rc-deadband", read: (c) => c.rc_deadband, write: (c, v) => { c.rc_deadband = v; } },
  { id: "roll-center", read: (c) => c.roll_center_rate, write: (c, v) => { c.roll_center_rate = v; } },
  { id: "roll-max", read: (c) => c.roll_max_rate, write: (c, v) => { c.roll_max_rate = v; } },
  { id: "roll-expo", read: (c) => c.roll_expo, write: (c, v) => { c.roll_expo = v; } },
  { id: "pitch-center", read: (c) => c.pitch_center_rate, write: (c, v) => { c.pitch_center_rate = v; } },
  { id: "pitch-max", read: (c) => c.pitch_max_rate, write: (c, v) => { c.pitch_max_rate = v; } },
  { id: "pitch-expo", read: (c) => c.pitch_expo, write: (c, v) => { c.pitch_expo = v; } },
  { id: "yaw-center", read: (c) => c.yaw_center_rate, write: (c, v) => { c.yaw_center_rate = v; } },
  { id: "yaw-max", read: (c) => c.yaw_max_rate, write: (c, v) => { c.yaw_max_rate = v; } },
  { id: "yaw-expo", read: (c) => c.yaw_expo, write: (c, v) => { c.yaw_expo = v; } },
  { id: "imu-lpf", read: (c) => c.imu_lpf_hz, write: (c, v) => { c.imu_lpf_hz = v; } },
  { id: "log-divisor", read: (c) => c.log_rate_divisor, write: (c, v) => { c.log_rate_divisor = v; } },
  { id: "rc-map", read: (c) => c.rc_map, write: (c, v) => { c.rc_map = v; } },
  { id: "rc-arm-channel", read: (c) => c.rc_arm_channel, write: (c, v) => { c.rc_arm_channel = v; } },
];

function renderConfig(): void {
  for (const field of NUMERIC_FIELDS) {
    const input = element<HTMLInputElement>(field.id);
    const value = config ? field.read(config) : undefined;
    input.value = value === undefined ? "" : String(value);
    input.disabled = config === null;
  }
  const protocol = element<HTMLSelectElement>("rc-protocol");
  protocol.value = config?.rc_protocol ?? "sbus";
  protocol.disabled = config?.rc_protocol === undefined;
  renderRates();
  updateWriteControls();
}

/** Reads the form, not `config`, so the curve follows edits before Apply. */
function renderRates(): void {
  const axis = (name: string): RateAxis | null => {
    const read = (suffix: string): number =>
      Number(element<HTMLInputElement>(`${name}-${suffix}`).value);
    const values = ["center", "max", "expo"].map((suffix) =>
      element<HTMLInputElement>(`${name}-${suffix}`).value.trim(),
    );
    if (values.some((value) => value === "")) {
      return null;
    }
    return { center: read("center"), max: read("max"), expo: read("expo") };
  };
  drawRates(element("rates-curve") as unknown as SVGSVGElement, {
    roll: axis("roll"),
    pitch: axis("pitch"),
    yaw: axis("yaw"),
  });
}

/**
 * The single place that decides whether writing is offered.
 *
 * The Rust session refuses regardless; this only keeps the interface honest
 * about what it will accept, so the operator is not invited to press a button
 * that cannot work.
 */
function updateWriteControls(): void {
  const writable = connected && safetyState?.writes_allowed === true;
  const allowed = writable && config !== null;
  const apply = element<HTMLButtonElement>("apply");
  apply.disabled = !allowed;
  apply.title = allowed
    ? "Stage, save and verify this configuration"
    : "Available when a disarmed controller is connected";

  const portsAllowed = writable && bindings !== null;
  const applyPorts = element<HTMLButtonElement>("apply-ports");
  applyPorts.disabled = !portsAllowed;
  applyPorts.title = portsAllowed
    ? "Save and verify the changed ports; reboot the controller to apply them"
    : "Available when a disarmed controller is connected";
}

async function loadConfig(): Promise<void> {
  setStatusLine("Reading configuration…", "busy");
  try {
    config = await api.readConfig();
    renderConfig();
    setStatusLine("Configuration read from the controller.");
  } catch (error) {
    report(error);
  }
}

function collectConfig(): FerroConfig | null {
  if (!config) {
    return null;
  }
  const draft = structuredClone(config);
  for (const field of NUMERIC_FIELDS) {
    const input = element<HTMLInputElement>(field.id);
    if (input.value.trim() === "") {
      continue;
    }
    const value = Number(input.value);
    if (!Number.isFinite(value)) {
      setStatusLine(`${field.id} is not a number.`, "bad");
      return null;
    }
    field.write(draft, value);
  }
  if (draft.rc_protocol !== undefined) {
    const protocol = element<HTMLSelectElement>("rc-protocol").value;
    draft.rc_protocol = protocol === "crsf" ? "crsf" : "sbus";
  }
  return draft;
}

async function applyConfig(): Promise<void> {
  const draft = collectConfig();
  if (!draft) {
    return;
  }
  setStatusLine("Staging, saving and verifying…", "busy");
  try {
    config = await api.applyConfig(draft);
    renderConfig();
    setStatusLine("Applied and verified by read-back.");
  } catch (error) {
    // A refusal here is the expected outcome of an armed controller, so it is
    // reported the same way the banner reports it rather than as a fault.
    report(error);
    if (error instanceof BridgeError && error.kind === "refused") {
      await pollSafety();
    }
  }
}

// --------------------------------------------------------- serial ports ---

function renderPorts(): void {
  const table = element<HTMLTableElement>("serial-ports");
  table.replaceChildren();
  element("serial-source").textContent = bindings
    ? bindings.saved
      ? "Saved bindings."
      : "Board defaults; nothing saved yet."
    : "";
  for (const binding of bindings?.ports ?? []) {
    const row = table.insertRow();
    row.insertCell().textContent = binding.port.toUpperCase();
    const select = document.createElement("select");
    select.dataset["port"] = binding.port;
    select.setAttribute("aria-label", `${binding.port} function`);
    for (const name of SERIAL_FUNCTIONS) {
      select.add(new Option(name, name, false, name === binding.function));
    }
    row.insertCell().append(select);
  }
  updateWriteControls();
}

async function loadPorts(): Promise<void> {
  try {
    bindings = await api.serialBindings();
    renderPorts();
  } catch (error) {
    report(error);
  }
}

/** Saves each changed port in turn; every one is verified by read-back. */
async function applyPorts(): Promise<void> {
  if (!bindings) {
    return;
  }
  const changes = [...document.querySelectorAll<HTMLSelectElement>("#serial-ports select")]
    .map((select) => ({ port: select.dataset["port"] ?? "", func: select.value }))
    .filter(({ port, func }) => bindings?.ports.find((b) => b.port === port)?.function !== func);
  if (changes.length === 0) {
    setStatusLine("No port changed.");
    return;
  }
  setStatusLine("Saving and verifying ports…", "busy");
  try {
    for (const { port, func } of changes) {
      bindings = await api.applySerialBinding(port, func);
    }
    renderPorts();
    setStatusLine("Ports saved and verified. Reboot the controller to apply them.");
  } catch (error) {
    report(error);
    await loadPorts();
    if (error instanceof BridgeError && error.kind === "refused") {
      await pollSafety();
    }
  }
}

// ------------------------------------------------------------- blackbox ---

async function loadFlights(): Promise<void> {
  setStatusLine("Cataloguing flights…", "busy");
  const list = element("flight-list");
  try {
    const catalog = await api.flights();
    const used = catalog.storage.used_pages;
    const total = catalog.storage.total_pages;
    element("storage-summary").textContent =
      `${String(used)} of ${String(total)} pages used · next flight ${String(catalog.storage.next_flight)}` +
      `${catalog.storage.writable ? "" : " · NOT writable"}`;

    list.replaceChildren();
    for (const entry of catalog.flights) {
      const row = document.createElement("li");
      const boot = entry.boot_session === null ? "boot unknown" : `boot ${String(entry.boot_session)}`;
      row.textContent =
        `Flight ${String(entry.flight.flight_id)} · ${boot} · pages ` +
        `${String(entry.flight.first_page)}..${String(entry.flight.last_page)} · ` +
        `${String(entry.flight.bytes)} bytes`;
      list.append(row);
    }
    setStatusLine(`${String(catalog.flights.length)} flight(s) stored.`);
  } catch (error) {
    report(error);
  }
}

async function downloadLatest(): Promise<void> {
  const progress = element<HTMLProgressElement>("download-progress");
  const output = element<HTMLInputElement>("download-path").value.trim();
  if (output === "") {
    setStatusLine("Choose an output path first.", "bad");
    return;
  }
  progress.hidden = false;
  progress.value = 0;
  setStatusLine("Downloading…", "busy");
  try {
    const summary = await api.downloadFlight("latest", output, ({ page, total }) => {
      progress.max = total;
      progress.value = page;
    });
    setStatusLine(`Downloaded ${String(summary.pages)} pages to ${summary.path}.`);
  } catch (error) {
    report(error);
  } finally {
    progress.hidden = true;
  }
}

// ------------------------------------------------------------------ wire ---

function wire(): void {
  element("refresh-ports").addEventListener("click", () => void refreshPorts());
  element("connect").addEventListener("click", () => void connect());
  element("disconnect").addEventListener("click", () => void disconnect());
  element("reload-config").addEventListener("click", () => void loadConfig());
  element("apply").addEventListener("click", () => void applyConfig());
  element("reload-ports").addEventListener("click", () => void loadPorts());
  element("apply-ports").addEventListener("click", () => void applyPorts());
  element("load-flights").addEventListener("click", () => void loadFlights());
  element("download").addEventListener("click", () => void downloadLatest());
  element("find-control").addEventListener("click", startFinding);
  for (const axis of ["roll", "pitch", "yaw"]) {
    for (const suffix of ["center", "max", "expo"]) {
      element(`${axis}-${suffix}`).addEventListener("input", renderRates);
    }
  }

  if (usingMock) {
    const banner = element("mock-banner");
    banner.hidden = false;
    element("toggle-armed").addEventListener("click", () => {
      const mock = api as MockApi;
      mock.armed = !mock.armed;
      void pollSafety();
    });
    element("toggle-radio").addEventListener("click", () => {
      const mock = api as MockApi;
      mock.radioOn = !mock.radioOn;
      void pollSafety();
    });
    element("flip-switch").addEventListener("click", () => {
      (api as MockApi).flipAux();
      void pollSafety();
    });
  }

  renderSafety();
  renderConfig();
  renderPorts();
  void refreshPorts();
}

wire();

// The Motors tab: hold-to-spin motor test, ESC spin direction, and a motor
// order wizard, behind the same props-off warnings Betaflight shows.
//
// Nothing here can make a motor spin on its own authority. Each spin is one
// short firmware lease that the safety master grants only while disarmed
// with the arm switch off; this module renews it about every 100 ms while a
// button is held. Releasing the button, leaving the window, or losing the
// connection simply stops the renewals, and the firmware stops the motor
// when the lease runs out.

import { toBridgeError, type Api, type FerroConfig } from "./api";

/** How often a held button renews its lease. The firmware lease is 250 ms. */
const RENEW_MS = 100;

/** Betaflight Quad X: motor number, position name, and x/y with nose up. */
const LAYOUT: ReadonlyArray<{ motor: number; name: string; x: number; y: number; cw: boolean }> = [
  { motor: 4, name: "front-left", x: 70, y: 70, cw: true },
  { motor: 2, name: "front-right", x: 210, y: 70, cw: false },
  { motor: 3, name: "rear-left", x: 70, y: 210, cw: false },
  { motor: 1, name: "rear-right", x: 210, y: 210, cw: true },
];

const SVG = "http://www.w3.org/2000/svg";

function svg<K extends keyof SVGElementTagNameMap>(
  name: K,
  attrs: Record<string, string | number>,
): SVGElementTagNameMap[K] {
  const el = document.createElementNS(SVG, name);
  for (const [key, value] of Object.entries(attrs)) {
    el.setAttribute(key, String(value));
  }
  return el;
}

/** The four digits of a motor order, or the board wiring when unset. */
export function motorOrder(config: FerroConfig | null): number[] {
  const digits = String(config?.motor_map ?? 1234).split("").map(Number);
  return digits.length === 4 ? digits : [1, 2, 3, 4];
}

/**
 * The new motor order after the wizard: while logical motor `spun` was
 * spinning under `current`, the pilot clicked `position`. The board output
 * that spun belongs at that position.
 */
export function reorder(current: number[], answers: ReadonlyMap<number, number>): number[] | null {
  const next = [0, 0, 0, 0];
  for (const [spun, position] of answers) {
    next[position - 1] = current[spun - 1] ?? 0;
  }
  const valid = [1, 2, 3, 4].every((digit) => next.filter((d) => d === digit).length === 1);
  return valid ? next : null;
}

/** Keeps one motor's lease renewed while asked to, and stops it after. */
class MotorDriver {
  private motor: number | null = null;
  private timer: number | undefined;
  private inFlight = false;

  constructor(
    private readonly api: Api,
    private readonly onError: (message: string) => void,
  ) {}

  get active(): number | null {
    return this.motor;
  }

  hold(motor: number): void {
    if (this.motor === motor) {
      return;
    }
    this.release();
    this.motor = motor;
    void this.renew();
    this.timer = window.setInterval(() => void this.renew(), RENEW_MS);
  }

  release(): void {
    window.clearInterval(this.timer);
    this.timer = undefined;
    const was = this.motor;
    this.motor = null;
    if (was !== null) {
      // The lease would stop it anyway; this makes it stop now.
      void this.api.motorStop().catch(() => undefined);
    }
  }

  private async renew(): Promise<void> {
    const motor = this.motor;
    if (this.inFlight || motor === null) {
      return;
    }
    this.inFlight = true;
    try {
      await this.api.motorSpin(motor);
    } catch (error) {
      this.release();
      this.onError(toBridgeError(error).message);
    } finally {
      this.inFlight = false;
      // A renewal that landed after release restarted the lease; end it.
      if (this.motor === null) {
        void this.api.motorStop().catch(() => undefined);
      }
    }
  }
}

export interface MotorsTabHost {
  api: Api;
  config(): FerroConfig | null;
  /** Saves a configuration through the normal verified apply path. */
  save(config: FerroConfig): Promise<void>;
  status(text: string, tone?: "idle" | "busy" | "bad"): void;
}

export class MotorsTab {
  private readonly driver: MotorDriver;
  private readonly enable: HTMLInputElement;
  private readonly diagram: SVGSVGElement;
  private readonly message: HTMLElement;
  private readonly controls: HTMLElement;
  private connected = false;
  private wizard: { step: number; answers: Map<number, number>; current: number[] } | null =
    null;

  constructor(
    container: HTMLElement,
    private readonly host: MotorsTabHost,
  ) {
    this.driver = new MotorDriver(host.api, (message) => {
      this.say(`Motor stopped: ${message}`, "bad");
      this.stopWizard();
      this.render();
    });

    container.replaceChildren();

    const notice = document.createElement("div");
    notice.className = "motor-notice";
    notice.innerHTML =
      "<strong>Motor test mode / arming disable notice:</strong> holding a motor button " +
      "will spin that motor. In order to prevent injury " +
      '<strong class="danger">remove ALL propellers</strong> before using this feature. ' +
      "Testing a motor also disables arming for two seconds after the last test, so the " +
      "receiver cannot arm the quad by accident. Connect a flight battery: on USB power " +
      "alone the ESCs cannot spin.";

    const consent = document.createElement("label");
    consent.className = "motor-consent";
    this.enable = document.createElement("input");
    this.enable.type = "checkbox";
    consent.append(
      this.enable,
      document.createTextNode(
        " I understand the risks, propellers are removed - enable motor control and arming disable.",
      ),
    );
    this.enable.addEventListener("change", () => {
      if (!this.enable.checked) {
        this.stopAll();
      }
      this.render();
    });

    this.diagram = document.createElementNS(SVG, "svg");
    this.diagram.classList.add("motor-diagram");
    this.diagram.setAttribute("viewBox", "0 0 280 280");
    this.diagram.setAttribute("role", "group");
    this.diagram.setAttribute("aria-label", "Motor layout, nose up");

    this.message = document.createElement("div");
    this.message.className = "motor-message";

    this.controls = document.createElement("div");
    this.controls.className = "motor-controls";

    const layout = document.createElement("div");
    layout.className = "motor-layout";
    layout.append(this.diagram, this.controls);
    container.append(notice, consent, layout, this.message);

    // Any way of letting go stops the motor.
    window.addEventListener("pointerup", () => this.releaseHeld());
    window.addEventListener("pointercancel", () => this.releaseHeld());
    window.addEventListener("blur", () => this.stopAll());
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        this.stopAll();
      }
    });
    window.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        this.stopAll();
      }
    });

    this.render();
  }

  setConnected(connected: boolean): void {
    this.connected = connected;
    if (!connected) {
      this.stopAll();
      this.enable.checked = false;
    }
    this.render();
  }

  /** Physical outputs the firmware reports as driven, as logical motors. */
  showActive(activeOutputs: number): void {
    const order = motorOrder(this.host.config());
    for (const group of Array.from(this.diagram.querySelectorAll<SVGGElement>("g.motor"))) {
      const motor = Number(group.dataset["motor"]);
      // Board wiring is the identity on the Foxeer; on other boards this
      // label is approximate, which is why the button state is also shown.
      const output = order[motor - 1] ?? 0;
      group.classList.toggle("reported", (activeOutputs & (1 << (output - 1))) !== 0);
    }
  }

  private get enabled(): boolean {
    return this.connected && this.enable.checked;
  }

  private say(text: string, tone: "idle" | "busy" | "bad" = "idle"): void {
    this.message.textContent = text;
    this.message.dataset["tone"] = tone;
    this.host.status(text, tone);
  }

  private releaseHeld(): void {
    if (this.wizard === null && this.driver.active !== null) {
      this.driver.release();
      this.render();
    }
  }

  private stopAll(): void {
    this.driver.release();
    this.stopWizard();
  }

  private stopWizard(): void {
    if (this.wizard !== null) {
      this.wizard = null;
      this.driver.release();
    }
  }

  private render(): void {
    this.drawDiagram();
    this.drawControls();
    this.enable.disabled = !this.connected;
  }

  private drawDiagram(): void {
    const d = this.diagram;
    d.replaceChildren();
    d.append(
      svg("line", { x1: 70, y1: 70, x2: 210, y2: 210, class: "frame" }),
      svg("line", { x1: 210, y1: 70, x2: 70, y2: 210, class: "frame" }),
      svg("rect", { x: 118, y: 112, width: 44, height: 56, rx: 6, class: "frame-body" }),
      svg("path", { d: "M140 96 l-10 14 h20 z", class: "frame-nose" }),
    );
    const front = svg("text", { x: 140, y: 22, class: "frame-label", "text-anchor": "middle" });
    front.textContent = "FRONT";
    d.append(front);

    const wizard = this.wizard;
    for (const { motor, name, x, y, cw } of LAYOUT) {
      const group = svg("g", { class: "motor", tabindex: 0 });
      group.dataset["motor"] = String(motor);
      group.setAttribute("aria-label", `Motor ${String(motor)}, ${name}`);
      const assigned = wizard !== null && [...wizard.answers.values()].includes(motor);
      group.classList.toggle("held", this.driver.active === motor && wizard === null);
      group.classList.toggle("assigned", assigned);
      group.classList.toggle("disabled", !this.enabled || assigned);

      group.append(svg("circle", { cx: x, cy: y, r: 44, class: "prop-disc" }));
      // Default Betaflight direction (props in), as a reference arrow.
      const sweep = cw ? 1 : 0;
      const endX = cw ? x + 30 : x - 30;
      group.append(
        svg("path", {
          d: `M${String(x)} ${String(y - 30)} A30 30 0 0 ${String(sweep)} ${String(endX)} ${String(y)}`,
          class: "prop-arrow",
          "marker-end": "url(#arrow)",
        }),
      );
      const label = svg("text", { x, y: y + 6, class: "motor-label", "text-anchor": "middle" });
      label.textContent = String(motor);
      group.append(label);

      group.addEventListener("pointerdown", (event) => {
        event.preventDefault();
        if (!this.enabled) {
          return;
        }
        if (this.wizard !== null) {
          this.answerWizard(motor);
        } else {
          this.driver.hold(motor);
          this.say(`Motor ${String(motor)} (${name}) idling while held.`, "busy");
          this.render();
        }
      });
      d.append(group);
    }
    const defs = svg("defs", {});
    const marker = svg("marker", {
      id: "arrow",
      viewBox: "0 0 10 10",
      refX: 5,
      refY: 5,
      markerWidth: 5,
      markerHeight: 5,
      orient: "auto-start-reverse",
    });
    marker.append(svg("path", { d: "M0 0 L10 5 L0 10 z", class: "prop-arrow-head" }));
    defs.append(marker);
    d.append(defs);
  }

  private drawControls(): void {
    const c = this.controls;
    c.replaceChildren();

    if (this.wizard !== null) {
      const step = this.wizard.step;
      const heading = document.createElement("p");
      heading.innerHTML =
        `<strong>Reorder motors, step ${String(step)} of 4.</strong> One motor is spinning. ` +
        "Click the position on the diagram where it is.";
      const cancel = button("Stop and cancel", () => {
        this.stopWizard();
        this.say("Motor reorder cancelled; nothing was saved.");
        this.render();
      });
      cancel.classList.add("danger-button");
      c.append(heading, cancel);
      return;
    }

    const hint = document.createElement("p");
    hint.textContent = this.enabled
      ? "Press and hold a motor to idle it. Release to stop."
      : "Tick the box above to enable motor control.";
    c.append(hint);

    const direction = document.createElement("fieldset");
    direction.disabled = !this.enabled;
    const legend = document.createElement("legend");
    legend.textContent = "Motor direction";
    const warning = document.createElement("p");
    warning.className = "small";
    warning.innerHTML =
      '<strong class="danger">Warning:</strong> make sure all propellers are removed. ' +
      "This sends DShot commands that set and save the ESC's direction. " +
      "The ESC cannot report its direction back, so hold the motor afterwards to check it.";
    direction.append(legend, warning);
    for (const motor of [1, 2, 3, 4]) {
      const row = document.createElement("div");
      row.className = "row";
      const name = LAYOUT.find((entry) => entry.motor === motor)?.name ?? "";
      const label = document.createElement("span");
      label.textContent = `Motor ${String(motor)} (${name})`;
      label.className = "direction-label";
      row.append(
        label,
        button("Normal", () => void this.setDirection(motor, false)),
        button("Reversed", () => void this.setDirection(motor, true)),
      );
      direction.append(row);
    }

    const reorderSet = document.createElement("fieldset");
    reorderSet.disabled = !this.enabled;
    const reorderLegend = document.createElement("legend");
    reorderLegend.textContent = "Motor order";
    const reorderText = document.createElement("p");
    reorderText.className = "small";
    reorderText.innerHTML =
      '<strong class="danger">Warning:</strong> make sure all propellers are removed. ' +
      `Current order ${motorOrder(this.host.config()).join("")}. The wizard spins each motor ` +
      "in turn; click where it is, and the new order is saved and verified.";
    reorderSet.append(
      reorderLegend,
      reorderText,
      button("Reorder motors…", () => this.startWizard()),
    );

    c.append(direction, reorderSet);
  }

  private async setDirection(motor: number, reversed: boolean): Promise<void> {
    this.driver.release();
    const word = reversed ? "reversed" : "normal";
    this.say(`Setting motor ${String(motor)} to ${word}…`, "busy");
    try {
      await this.host.api.motorDirection(motor, reversed);
      this.say(
        `Motor ${String(motor)} set to ${word} and saved in the ESC. Hold it to check the direction.`,
      );
    } catch (error) {
      this.say(`Direction not changed: ${toBridgeError(error).message}`, "bad");
    }
  }

  private startWizard(): void {
    if (this.host.config() === null) {
      this.say("Read the configuration from the controller first.", "bad");
      return;
    }
    this.wizard = { step: 1, answers: new Map(), current: motorOrder(this.host.config()) };
    this.driver.hold(1);
    this.say("Reorder: motor 1 of 4 spinning. Click where it is.", "busy");
    this.render();
  }

  private answerWizard(position: number): void {
    const wizard = this.wizard;
    if (wizard === null || [...wizard.answers.values()].includes(position)) {
      return;
    }
    wizard.answers.set(wizard.step, position);
    if (wizard.step < 4) {
      wizard.step += 1;
      this.driver.hold(wizard.step);
      this.say(`Reorder: motor ${String(wizard.step)} of 4 spinning. Click where it is.`, "busy");
      this.render();
      return;
    }
    this.driver.release();
    const answers = wizard.answers;
    const current = wizard.current;
    this.wizard = null;
    this.render();
    void this.saveOrder(reorder(current, answers));
  }

  private async saveOrder(order: number[] | null): Promise<void> {
    const config = this.host.config();
    if (order === null || config === null) {
      this.say("Each position must be chosen once; nothing was saved.", "bad");
      return;
    }
    const motorMap = Number(order.join(""));
    if (motorMap === config.motor_map) {
      this.say("Motor order already matches; nothing to change.");
      return;
    }
    this.say(`Saving motor order ${String(motorMap)}…`, "busy");
    try {
      await this.host.save({ ...structuredClone(config), motor_map: motorMap });
      this.say(`Motor order ${String(motorMap)} saved and verified. Hold each motor to check it.`);
    } catch (error) {
      this.say(`Motor order not saved: ${toBridgeError(error).message}`, "bad");
    }
    this.render();
  }
}

function button(text: string, onClick: () => void): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.textContent = text;
  element.addEventListener("click", onClick);
  return element;
}

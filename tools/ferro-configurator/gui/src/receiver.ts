// Live receiver channels, and a "move the control you mean" helper.
//
// The helper only answers "which channel is that switch on?"; the answer
// goes into the Receiver fields of the tuning form by hand.

import type { FerroConfig } from "./api";

/** Every board's default: AETR sticks, arm switch on channel 9. */
const DEFAULT_STICK_ORDER = 1234;
const DEFAULT_ARM_CHANNEL = 9;
const STICK_NAMES = ["Roll", "Pitch", "Throttle", "Yaw"];

/** What a zero-based channel carries under the saved map, or `Aux n`. */
export function channelName(index: number, config: FerroConfig | null): string {
  const order = String(config?.rc_map ?? DEFAULT_STICK_ORDER);
  const stick = [...order].findIndex((digit) => Number(digit) === index + 1);
  if (stick >= 0) {
    return STICK_NAMES[stick] ?? "Stick";
  }
  if (index + 1 === (config?.rc_arm_channel ?? DEFAULT_ARM_CHANNEL)) {
    return "Arm";
  }
  return `Aux ${String(index - 3)}`;
}

/** Renders one bar per channel, 1000..2000 µs. */
export function drawChannels(
  container: HTMLElement,
  channels: number[],
  config: FerroConfig | null,
): void {
  if (container.childElementCount !== channels.length) {
    container.replaceChildren();
    channels.forEach(() => {
      const row = document.createElement("div");
      row.className = "channel";
      row.innerHTML = `<span class="channel-name"></span><span class="channel-bar"><span></span></span><span class="channel-value"></span>`;
      container.append(row);
    });
  }
  channels.forEach((value, index) => {
    const row = container.children[index] as HTMLElement;
    (row.querySelector(".channel-name") as HTMLElement).textContent =
      `${String(index + 1)} ${channelName(index, config)}`;
    const fill = row.querySelector(".channel-bar > span") as HTMLElement;
    const share = Math.min(Math.max((value - 1000) / 1000, 0), 1);
    fill.style.width = `${(share * 100).toFixed(1)}%`;
    (row.querySelector(".channel-value") as HTMLElement).textContent = String(value);
  });
}

/**
 * Watches channel samples and reports the one that moved furthest from where
 * it started. A switch flip moves ~1000 µs, stick noise a few, so the first
 * channel past `threshold` wins outright rather than by a close margin.
 */
export class ControlFinder {
  private baseline: number[] | null = null;
  private readonly threshold: number;

  constructor(threshold = 300) {
    this.threshold = threshold;
  }

  /** Returns the zero-based channel index once one has clearly moved. */
  observe(channels: number[]): number | null {
    if (this.baseline === null) {
      this.baseline = [...channels];
      return null;
    }
    let best: number | null = null;
    let bestDelta = this.threshold;
    channels.forEach((value, index) => {
      const delta = Math.abs(value - (this.baseline?.[index] ?? value));
      if (delta > bestDelta) {
        best = index;
        bestDelta = delta;
      }
    });
    return best;
  }
}

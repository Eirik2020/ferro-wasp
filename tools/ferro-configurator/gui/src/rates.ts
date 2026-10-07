// The stick-to-rotation curve, drawn from the tuning form as it is edited.
//
// FerroWasp uses Betaflight's "Actual" rates, so a pilot can type in the
// centre sensitivity, max rate and expo they already fly and see the same
// shape. The formula is a copy of `apply_actual_rate` in
// `crates/ferrowasp-tasks/src/drone_toolbox.rs`; if that changes, this must too.

export interface RateAxis {
  center: number;
  max: number;
  expo: number;
}

/** Mirrors `ActualRateAxis::sanitized`. */
function sanitized(rate: RateAxis): RateAxis {
  const center = Number.isFinite(rate.center) ? Math.min(Math.max(rate.center, 0), 1200) : 0;
  const max = Number.isFinite(rate.max) ? Math.min(Math.max(rate.max, center), 1200) : center;
  const expo = Number.isFinite(rate.expo) ? Math.min(Math.max(rate.expo, 0), 1) : 0;
  return { center, max, expo };
}

/** Degrees per second for a stick position in -1..1. */
export function actualRate(stick: number, axis: RateAxis): number {
  const s = Math.min(Math.max(Number.isFinite(stick) ? stick : 0, -1), 1);
  const rate = sanitized(axis);
  const fifth = s * s * s * s * s;
  const transition = Math.abs(s) * (fifth * rate.expo + s * (1 - rate.expo));
  return s * rate.center + (rate.max - rate.center) * transition;
}

const SVG = "http://www.w3.org/2000/svg";
const COLORS: Record<string, string> = { roll: "#e07a5f", pitch: "#81b29a", yaw: "#7aa6e0" };

function node<K extends keyof SVGElementTagNameMap>(
  name: K,
  attrs: Record<string, string | number>,
): SVGElementTagNameMap[K] {
  const el = document.createElementNS(SVG, name);
  for (const [key, value] of Object.entries(attrs)) {
    el.setAttribute(key, String(value));
  }
  return el;
}

/** Redraws the half-stick curve (centre to full deflection) for each axis. */
export function drawRates(svg: SVGSVGElement, axes: Record<string, RateAxis | null>): void {
  const width = 360;
  const height = 200;
  const pad = 32;
  svg.setAttribute("viewBox", `0 0 ${String(width)} ${String(height)}`);
  svg.replaceChildren();

  const present = Object.entries(axes).filter((entry): entry is [string, RateAxis] => entry[1] !== null);
  const top = Math.max(200, ...present.map(([, a]) => sanitized(a).max));
  const yMax = Math.ceil(top / 200) * 200;
  const x = (stick: number): number => pad + stick * (width - pad - 8);
  const y = (rate: number): number => height - pad + 8 - (rate / yMax) * (height - pad);

  for (let rate = 0; rate <= yMax; rate += yMax / 4) {
    svg.append(node("line", { x1: pad, x2: width - 8, y1: y(rate), y2: y(rate), class: "grid-line" }));
    const label = node("text", { x: pad - 4, y: y(rate) + 4, class: "axis-label", "text-anchor": "end" });
    label.textContent = String(rate);
    svg.append(label);
  }
  const caption = node("text", { x: width - 8, y: height - 4, class: "axis-label", "text-anchor": "end" });
  caption.textContent = "stick → full deflection (°/s)";
  svg.append(caption);

  // Axes that share a max rate share one end label, so they do not overprint.
  const labels = new Map<number, string[]>();
  for (const [name, axis] of present) {
    const points: string[] = [];
    for (let i = 0; i <= 50; i += 1) {
      const stick = i / 50;
      points.push(`${x(stick).toFixed(1)},${y(actualRate(stick, axis)).toFixed(1)}`);
    }
    svg.append(
      node("polyline", { points: points.join(" "), fill: "none", stroke: COLORS[name] ?? "#ccc", "stroke-width": 2 }),
    );
    const max = Math.round(sanitized(axis).max);
    labels.set(max, [...(labels.get(max) ?? []), name]);
  }
  for (const [max, names] of labels) {
    const end = node("text", {
      x: width - 10,
      y: y(max) - 4,
      class: "axis-label",
      // Inline so it wins over the class's muted fill.
      style: `fill: ${COLORS[names[0] ?? ""] ?? "#ccc"}`,
      "text-anchor": "end",
    });
    end.textContent = `${names.join(" / ")} ${String(max)}`;
    svg.append(end);
  }
}

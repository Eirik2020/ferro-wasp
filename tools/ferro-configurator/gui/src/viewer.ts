// A 3D quad that tilts with the controller, like Betaflight's Setup tab.
//
// Plain CSS 3D transforms rather than a WebGL library: the model is eight
// flat shapes, and the point is to see at a glance that the board's
// orientation is right - tilt the quad nose up and the model should too.
//
// Angles are the firmware's estimate in degrees. The sign convention used
// here is roll positive right side down, pitch positive nose up, yaw positive
// clockwise seen from above. It has not been checked against a board yet;
// if the model tilts the wrong way on the bench, flip the sign below rather
// than the firmware.

// In the model, x is right, y is towards the tail and z is up; the camera
// tilts it to a view from behind. CSS rotateX(+a) lowers the nose and
// rotateY(+a) lowers the right side, so pitch takes the minus sign.
const ROLL_SIGN = 1;
const PITCH_SIGN = -1;
const YAW_SIGN = 1;

/** Betaflight Quad X positions, as [motor, x, y] with the nose up. */
const MOTOR_POSITIONS: ReadonlyArray<[number, number, number]> = [
  [4, -1, -1],
  [2, 1, -1],
  [3, -1, 1],
  [1, 1, 1],
];

export class QuadViewer {
  private readonly model: HTMLElement;
  private readonly readout: HTMLElement;
  private yawOffset = 0;
  private lastYaw = 0;

  constructor(container: HTMLElement) {
    container.replaceChildren();
    container.classList.add("quad-viewer");

    const scene = document.createElement("div");
    scene.className = "quad-scene";
    const camera = document.createElement("div");
    camera.className = "quad-camera";
    this.model = document.createElement("div");
    this.model.className = "quad-model";

    for (const name of ["arm arm-a", "arm arm-b", "plate", "plate-top", "nose"]) {
      const part = document.createElement("div");
      part.className = name;
      this.model.append(part);
    }
    for (const [motor, x, y] of MOTOR_POSITIONS) {
      const prop = document.createElement("div");
      prop.className = "prop";
      prop.dataset["motor"] = String(motor);
      prop.style.setProperty("--x", String(x));
      prop.style.setProperty("--y", String(y));
      prop.textContent = String(motor);
      this.model.append(prop);
    }

    camera.append(this.model);
    scene.append(camera);

    this.readout = document.createElement("div");
    this.readout.className = "quad-readout";

    const reset = document.createElement("button");
    reset.type = "button";
    reset.textContent = "Reset yaw";
    reset.title = "Point the model's nose away from you, like Betaflight's Reset Z axis";
    reset.addEventListener("click", () => {
      this.yawOffset = this.lastYaw;
      this.render([0, 0, this.lastYaw]);
    });

    const footer = document.createElement("div");
    footer.className = "row";
    footer.append(this.readout, reset);
    container.append(scene, footer);
    this.render(null);
  }

  /** Draws one attitude, or a level grey model when there is none. */
  render(attitude: [number, number, number] | null): void {
    this.model.classList.toggle("no-data", attitude === null);
    if (attitude === null) {
      this.model.style.transform = "";
      this.readout.textContent = "No attitude data";
      return;
    }
    const [roll, pitch, yaw] = attitude;
    this.lastYaw = yaw;
    const shownYaw = yaw - this.yawOffset;
    this.model.style.transform =
      `rotateZ(${String(YAW_SIGN * shownYaw)}deg) ` +
      `rotateX(${String(PITCH_SIGN * pitch)}deg) ` +
      `rotateY(${String(ROLL_SIGN * roll)}deg)`;
    this.readout.textContent =
      `Roll ${roll.toFixed(1)}° · Pitch ${pitch.toFixed(1)}° · Yaw ${yaw.toFixed(0)}°`;
  }

  /** Highlights the props of physical outputs the firmware is driving. */
  showSpinning(motors: ReadonlySet<number>): void {
    for (const prop of Array.from(this.model.querySelectorAll<HTMLElement>(".prop"))) {
      prop.classList.toggle("spinning", motors.has(Number(prop.dataset["motor"])));
    }
  }
}

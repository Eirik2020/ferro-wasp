# USB Session Procedure

What a Foxeer F405 V2 can show over USB alone, with ESC power off and no probe.

## Foxeer USB Session

Verifies: CONFIG-014, CONFIG-025, CONFIG-030, CONFIG-031, CONFIG-033,
CONFIG-034.

`python3 tools/usb_bench.py session --board foxeer-f405-v2` runs these steps with
no probe, from a clean checkout. It builds the candidate and the same commit with
`bench_config_fault`, confirms the bench state with the operator, reports a failed
check and goes on, and records a pass with reqtool for review. An armed board or
an unknown device ends it.

Keep the board on USB power only, with ESC power and the flight battery
disconnected. It needs no receiver. The firmware drives DShot stop frames at its
motor pads, which an unpowered ESC ignores.

1. Flash the candidate over the ROM's DFU, from whatever the board runs. Require
   it back on USB with no power cycle.
2. Require `build` to report this commit and no features. Keep a configuration
   export and the flight list.
3. Watch 30 s of status lines. Require disarmed, IMU ready and fresh, the battery
   reading fresh, uptime rising, the IMU sequence advancing, the control rate
   within 5% of the rate the image reports, and `live` reporting no motor output.
4. Run the flash information, log list and scratch-sector commands. No flight is
   erased.
5. Run the [USB RC configuration gate](foxeer-f405-v2.md#foxeer-usb-rc-configuration-gate), which
   records itself.
6. Set `roll_p` without saving. Require `OK applied`, the new value, and the old
   value after `reboot`.
7. Require `config bind uart3 rc` refused, since SBUS needs an inverter that port
   lacks, and `config bind uart2 none` refused, since it leaves RC unbound. Require
   `config bind uart4 esc_telemetry` to move the function off `uart1`. Restore the
   table and require it to read as it began.
8. Require `reboot` to restart the board, and `reboot dfu` to bring it to its ROM.
   Flash the candidate back.
9. The operator sets the board flat and still; require `imu calibrate` to answer
   with a level, save it, and keep it across a reboot. The operator rocks the board;
   require a refusal naming the movement, the level unchanged, and `config set`
   accepted, which shows the arm inhibit was released. This overwrites the board's
   stored level.
10. Flash the `bench_config_fault` image. For each of `bench invalid-config
    CONFIRM` and `bench damage-config CONFIRM`: save, break the save, and reboot.
    Require `config fallback` to name the fault and the board to fly the previous
    save. Require `config fallback ack` to clear it, one `config save` to repair
    the slot, and `none` after another reboot. Flash the candidate back.
11. Require the configuration export to match step 2.

Stop on an armed state, any actuator activity, an unknown image or feature set,
or a board that does not return.

Limits: USB shows a boot only once its port is up, so BOOT-001 to BOOT-004 still
need the [boot and idle gate](common-boot-idle.md#boot-and-idle-state) and a probe.
Nothing here measures the motor pins. USB blocks arming and refusal reasons go to
RTT, so a held inhibit or an unacknowledged fallback refusing an arm attempt is not
observed; CONFIG-017 to CONFIG-019 and CONFIG-027 keep their host evidence alone.

Required evidence: both images and their hashes, the transcript of every command
and answer, the status lines of step 3, the configuration exports, and operator
observations.

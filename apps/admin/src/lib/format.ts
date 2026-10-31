import type { RegisterValue } from "../schema";

/** `heat_pump_mode` → `heat pump mode`. */
export const human = (value: string) => value.replaceAll("_", " ");

/** A register value for display: On/Off for booleans, an em dash when absent. */
export function display(value?: RegisterValue | null) {
  return value
    ? typeof value.value === "boolean"
      ? value.value
        ? "On"
        : "Off"
      : String(value.value)
    : "—";
}

/** Short age: `<1s`, `42s`, `3m`. */
export function age(ms: number) {
  return ms < 1000
    ? "<1s"
    : ms < 60_000
      ? `${Math.floor(ms / 1000)}s`
      : `${Math.floor(ms / 60_000)}m`;
}

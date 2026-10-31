import { useEffect, useState, useCallback } from "react";
import { api, errorMessage } from "./api";
import type { DeviceSnapshot } from "./schema";

export function useDevice(enabled: boolean) {
  const [device, setDevice] = useState<DeviceSnapshot | null>(null);
  const [error, setError] = useState("");
  const [stream, setStream] = useState("Connecting");
  const [receivedAt, setReceivedAt] = useState(0);
  const [now, setNow] = useState(Date.now());
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    if (!enabled) {
      setDevice(null);
      return;
    }
    let disposed = false;
    let busy = false;
    async function load() {
      if (busy || disposed) return;
      busy = true;
      try {
        const next = await api.device();
        if (!disposed) {
          setDevice(next);
          setReceivedAt(Date.now());
          setError("");
        }
      } catch (err) {
        if (!disposed) setError(errorMessage(err));
      } finally {
        busy = false;
      }
    }
    const events = new EventSource("/api/v1/events", { withCredentials: true });
    events.onopen = () => {
      setStream("Live");
      void load();
    };
    events.onerror = () => {
      setStream("Reconnecting");
    };
    for (const name of ["state", "command", "discovery", "scenario", "resync", "lease"])
      events.addEventListener(name, load);
    void load();
    const interval = window.setInterval(load, 15_000);
    return () => {
      disposed = true;
      events.close();
      window.clearInterval(interval);
    };
  }, [enabled, revision]);
  useEffect(() => {
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, []);
  const elapsed = receivedAt ? Math.max(0, now - receivedAt) : 0;
  const stale =
    !!device &&
    (device.stale ||
      elapsed + (device.now_ms - (device.last_update_ms ?? 0)) >
        device.stale_after_ms);
  return { device, error, stream, stale, elapsed, refresh };
}

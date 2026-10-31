// The update this browser tab started. Installing restarts the gateway and ends the session, so the
// intent survives the login screen and the reload into the new admin UI in sessionStorage.
const KEY = "nibe-gateway.pending-update";

export type PendingUpdate = { version: string; from: string; at: number };

export function pendingUpdate(): PendingUpdate | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(KEY) ?? "null");
    return value && typeof value.version === "string" && typeof value.from === "string" ? value : null;
  } catch {
    return null;
  }
}

export function rememberPendingUpdate(version: string, from: string) {
  try {
    sessionStorage.setItem(KEY, JSON.stringify({ version, from, at: Date.now() }));
  } catch {
    // Private mode without storage: the flow still works, only the login-screen hint is lost.
  }
}

export function clearPendingUpdate() {
  try {
    sessionStorage.removeItem(KEY);
  } catch {
    // Nothing to clear.
  }
}

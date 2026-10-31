import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api, errorMessage } from "../api";
import type { SetupStatus } from "../schema";
import { Button, Field, FieldHelp, Notice, Panel } from "../ui";

/** First-run administrator setup, or sign-in once an administrator exists. */
export function AuthForm({
  setup,
  onSuccess,
}: {
  setup: boolean;
  onSuccess: () => Promise<void>;
}) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [setupToken, setSetupToken] = useState("");
  const [setupInfo, setSetupInfo] = useState<SetupStatus | null>(null);
  useEffect(() => {
    let active = true;
    if (setup)
      void api
        .setupStatus()
        .then((info) => {
          if (active) setSetupInfo(info);
        })
        .catch(() => {});
    return () => {
      active = false;
    };
  }, [setup]);
  // The gateway decides per client: on the open setup AP no token is needed (ADR 0130). Until the
  // status arrives, or if it fails, keep asking for it; the server enforces the rule either way.
  const needsToken = setup && setupInfo?.setup_token_required !== false;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    if (setup && password !== confirm) {
      setError("Passwords do not match.");
      return;
    }
    setBusy(true);
    try {
      await (setup ? api.setup(password, needsToken ? setupToken : "") : api.login(password));
      setPassword("");
      setConfirm("");
      setSetupToken("");
      await onSuccess();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title={setup ? "Set up your gateway" : "Welcome back"}>
      <p>
        {setup
          ? "Create your administrator password to secure this gateway. There is no default password."
          : "Sign in to manage your local gateway."}
      </p>
      {setupInfo && (
        <Notice>
          {setupInfo.simulated ? "Simulated gateway" : "Device setup"}
          {setupInfo.setup_ssid ? ` · Setup network: ${setupInfo.setup_ssid}` : ""}
          . Create an administrator first, then open Wi-Fi setup.
        </Notice>
      )}
      <form onSubmit={submit}>
        {error && <Notice kind="error">{error}</Notice>}
        {needsToken && (
          <Field
            label="Device setup token"
            help="Use the unique token supplied with this gateway. In simulation, read setup-secrets.json in the data directory."
          >
            <input
              type="password"
              autoComplete="off"
              required
              value={setupToken}
              onChange={(e) => setSetupToken(e.target.value)}
            />
          </Field>
        )}
        <Field label="Administrator password">
          <input
            type="password"
            autoComplete={setup ? "new-password" : "current-password"}
            minLength={setup ? 12 : 1}
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        {setup && (
          <>
            <FieldHelp>Use at least 12 characters.</FieldHelp>
            <Field label="Confirm password">
              <input
                type="password"
                autoComplete="new-password"
                required
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
          </>
        )}
        <Button disabled={busy} type="submit">
          {busy ? "Please wait…" : setup ? "Create administrator" : "Sign in"}
        </Button>
      </form>
    </Panel>
  );
}

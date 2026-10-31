import { Notice } from "../primitives/Notice";

/**
 * The outcome of the last action on a page: an error notice and/or a success notice. Every
 * mutating page shows these above its panels.
 */
export function ActionMessages({ error, message }: { error?: string; message?: string }) {
  return (
    <>
      {error && <Notice kind="error">{error}</Notice>}
      {message && <Notice>{message}</Notice>}
    </>
  );
}

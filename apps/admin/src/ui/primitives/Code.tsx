import type { ReactNode } from "react";

/** Preformatted block, e.g. the debug JSON. */
export function CodeBlock({ children }: { children: ReactNode }) {
  return <pre>{children}</pre>;
}

/** A value shown once (API key): selectable as a whole, wraps anywhere. */
export function SecretValue({ children }: { children: ReactNode }) {
  return <code className="secret">{children}</code>;
}

import type { ReactNode } from "react";
import { Button } from "../primitives/Button";
import { SecretValue } from "../primitives/Code";
import { Notice } from "../primitives/Notice";

/** Warning notice that shows a secret once, with Copy and Done actions. */
export function OneTimeSecret({
  title,
  value,
  onCopy,
  onDone,
}: {
  title: ReactNode;
  value: string;
  onCopy: () => void;
  onDone: () => void;
}) {
  return (
    <Notice kind="warning">
      <strong>{title}</strong>
      <SecretValue>{value}</SecretValue>
      <Button onClick={onCopy}>Copy</Button>
      <Button onClick={onDone}>Done</Button>
    </Notice>
  );
}

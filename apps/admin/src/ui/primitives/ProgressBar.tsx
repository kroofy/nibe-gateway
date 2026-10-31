/**
 * A labelled progress bar. With `value` and `max` it shows how far along a task is; without them it is
 * indeterminate (work is running, length unknown). `label` is also the accessible name.
 */
export function ProgressBar({ label, value, max, detail }: { label: string; value?: number; max?: number; detail?: string }) {
  const percent = value !== undefined && max ? Math.min(100, Math.max(0, Math.round((value / max) * 100))) : undefined;
  return (
    <div className="progress">
      <div className="progress-label">
        <span>{label}</span>
        <span>{detail ?? (percent !== undefined ? `${percent} %` : "")}</span>
      </div>
      <div className="progress-track" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
        <div
          className={percent === undefined ? "progress-fill indeterminate" : "progress-fill"}
          style={percent === undefined ? undefined : { width: `${percent}%` }}
        />
      </div>
    </div>
  );
}

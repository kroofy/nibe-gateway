/** The async Clipboard API exists only on secure origins; the appliance UI runs on plain http. */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (window.isSecureContext && navigator.clipboard) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to the legacy path */
  }
  const field = document.createElement("textarea");
  field.value = text;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.appendChild(field);
  field.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } finally {
    field.remove();
  }
  return ok;
}

export function downloadJson(data: unknown, filename: string) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

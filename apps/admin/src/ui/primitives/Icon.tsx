/** Single-path 24 px line icons used by navigation and lists. */
export const ICON_PATHS = {
  overview: "M3 10 12 3l9 7v11h-6v-7H9v7H3z",
  readings: "M3 12h4l3-8 4 16 3-8h4",
  history: "M4 20V11m6 9V5m6 15v-8m5 8H3",
  controls: "M4 7h16M4 17h16M8 4v6m8 4v6",
  registers: "M4 4h16v16H4zM4 9h16M9 9v11",
  wifi: "M2 8a16 16 0 0 1 20 0M5 12a11 11 0 0 1 14 0m-10 4a5 5 0 0 1 6 0m-3 4h.01",
  access: "M7 14a4 4 0 1 1 3.9-5H21v3h-2v3h-3v-3h-5.1A4 4 0 0 1 7 14z",
  integrations: "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5",
  updates: "M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1zM9 12a3 3 0 1 0 6 0 3 3 0 1 0-6 0",
  audit: "M9 3h6v3H9zM7 4.5H5V21h14V4.5h-2M8 11h8m-8 4h8m-8 4h5",
  diagnostics: "m8 5-6 7 6 7m8-14 6 7-6 7m-3-15-2 16",
} as const;
export type IconName = keyof typeof ICON_PATHS;

/** Decorative icon (aria-hidden). Without a name it renders an empty 20 px placeholder. */
export function Icon({ name, size = 20 }: { name?: IconName; size?: number }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={name ? ICON_PATHS[name] : undefined} />
    </svg>
  );
}

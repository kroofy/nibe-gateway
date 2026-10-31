import type { IconName } from "./ui";

export const pages = [
  "Overview",
  "Live readings",
  "History",
  "Controls",
  "Register explorer",
  "Wi-Fi setup",
  "Access",
  "Integrations",
  "Updates",
  "Audit log",
  "Diagnostics",
  "Debug",
] as const;
export type Page = (typeof pages)[number];
export const slugs = [
  "overview",
  "readings",
  "history",
  "controls",
  "registers",
  "wifi",
  "access",
  "integrations",
  "updates",
  "audit",
  "diagnostics",
  "debug",
];
export const descriptions: Record<Page, string> = {
  Overview: "Your heat pump gateway, at a glance.",
  "Live readings": "Current observations directly from the device.",
  History: "Recorded readings: every value for 14 days, 15-minute detail for 13 months, hourly for 10 years.",
  Controls: "Explicit, validated commands for your heat pump.",
  "Register explorer": "Inspect the catalog and observed register values.",
  "Wi-Fi setup": "Connect your gateway to your local network.",
  Access: "API keys for integrations and the admin password.",
  Integrations: "Publish readings over MQTT, with Home Assistant discovery.",
  Updates: "Software releases and appliance recovery.",
  "Audit log": "Every change made to the gateway and the pump, and the leases in force.",
  Diagnostics: "Communication health and a safe report for support.",
  Debug: "An authenticated view of gateway diagnostics.",
};
export const icons: Partial<Record<Page, IconName>> = {
  Overview: "overview",
  "Live readings": "readings",
  History: "history",
  Controls: "controls",
  "Register explorer": "registers",
  "Wi-Fi setup": "wifi",
  Access: "access",
  Integrations: "integrations",
  Updates: "updates",
  "Audit log": "audit",
  Diagnostics: "diagnostics",
};
export function pageFromHash(): Page {
  return pages[slugs.indexOf(location.hash.slice(1))] || "Overview";
}

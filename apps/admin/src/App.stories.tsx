import type { Meta, StoryObj } from "@storybook/react-vite";
import App from "./App";
import { hardwareApi } from "./stories/mockApi";
import * as fx from "./stories/fixtures";

/**
 * The whole application — shell, navigation, polling, SSE — against the in-browser API stand-in.
 * Each story opens a route via the URL hash. Use the toolbar's viewport menu for phone widths.
 */
const meta = {
  title: "Screens/Full app",
  component: App,
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
} satisfies Meta<typeof App>;
export default meta;
type Story = StoryObj<typeof meta>;

const route = (hash: string, hardware = false): Story => ({
  parameters: { hash: `#${hash}`, ...(hardware ? { api: hardwareApi() } : {}) },
});

export const Setup: Story = {
  parameters: { api: { "GET /api/v1/auth/session": { body: { authenticated: false, setup_required: true } } } },
};
export const SignIn: Story = {
  parameters: { api: { "GET /api/v1/auth/session": { body: { authenticated: false, setup_required: false } } } },
};
export const GatewayUnreachable: Story = {
  parameters: { api: { "GET /api/v1/auth/session": { status: 502, body: { error: { code: "http_error", message: "Gateway request failed (502)" } } } } },
};
export const Overview = route("overview");
export const OverviewHardware = route("overview", true);
export const LiveReadings = route("readings", true);
export const History = route("history", true);
export const Controls = route("controls", true);
export const RegisterExplorer = route("registers", true);
export const WifiSetup = route("wifi");
export const Access = route("access", true);
export const Integrations = route("integrations", true);
export const Updates = route("updates", true);
export const Diagnostics = route("diagnostics", true);
export const Debug = route("debug", true);
export const StaleAndReconnecting: Story = {
  parameters: {
    hash: "#readings",
    stream: "reconnecting",
    api: { ...hardwareApi(), "GET /api/v1/device": { body: fx.hardwareDevice({ stale: true, ageMs: 180_000 }) } },
  },
};
export const OutdatedTab: Story = {
  parameters: { hash: "#overview", api: { "GET /health": { body: { version: "0.1.24" } } } },
};
export const PhoneOverview: Story = { ...route("overview", true), globals: { viewport: { value: "phone" } } };
export const PhoneRegisters: Story = { ...route("registers", true), globals: { viewport: { value: "phone" } } };

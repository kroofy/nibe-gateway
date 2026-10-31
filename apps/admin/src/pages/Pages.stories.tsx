import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import * as fx from "../stories/fixtures";
import { hardwareApi } from "../stories/mockApi";
import { Access, ApiKeyTable } from "./Access";
import { AuthForm } from "./AuthForm";
import { Controls } from "./Controls";
import { Debug, Diagnostics } from "./Diagnostics";
import { History, HistoryTable, toLines } from "./History";
import { Integrations } from "./Integrations";
import { OverviewView } from "./Overview";
import { Readings } from "./Readings";
import { NoteBadges, RegisterSummary, RegisterTable, Registers, VerificationBadge } from "./Registers";
import { Updates } from "./Updates";
import { Wifi } from "./Wifi";
import { AuthLayout } from "../ui";

/**
 * Each page component exactly as the app renders it, inside the content column only. Data comes
 * from the in-browser API stand-in (`src/stories/mockApi.ts`): simulated recordings by default,
 * `hardwareApi()` for the owner's F730 appliance.
 */
const meta = {
  title: "Screens/Pages",
  tags: ["autodocs"],
  parameters: { layout: "padded" },
  decorators: [(Story) => <main style={{ maxWidth: 1410, padding: "8px 16px", margin: 0 }}><Story /></main>],
} satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;
const hw = { api: hardwareApi() };

export const SetupForm: Story = {
  name: "Auth / First-run setup",
  decorators: [(Story) => <AuthLayout footnote="Local administration · No cloud account required"><Story /></AuthLayout>],
  render: () => <AuthForm setup onSuccess={async () => {}} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText(/^Device setup token/), "token");
    await userEvent.type(canvas.getByLabelText("Administrator password"), "correct horse battery");
    await userEvent.type(canvas.getByLabelText("Confirm password"), "different");
    await userEvent.click(canvas.getByRole("button", { name: "Create administrator" }));
    await expect(await canvas.findByRole("alert")).toHaveTextContent("Passwords do not match.");
  },
};
export const SignIn: Story = {
  name: "Auth / Sign in",
  decorators: SetupForm.decorators,
  render: () => <AuthForm setup={false} onSuccess={async () => {}} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText("Administrator password"), "wrong");
    await userEvent.click(canvas.getByRole("button", { name: "Sign in" }));
    await expect(await canvas.findByRole("alert")).toHaveTextContent("Incorrect password.");
  },
};

export const OverviewSimulated: Story = {
  name: "Overview / Simulated",
  render: () => (
    <OverviewView device={fx.simulatedDevice()} stale={false} elapsed={0} version="0.1.23"
      status={fx.gatewayStatus()} network={fx.wifiStatus()} networkError="" />
  ),
};
export const OverviewHardware: Story = {
  name: "Overview / Hardware",
  render: () => (
    <OverviewView device={fx.hardwareDevice()} stale={false} elapsed={0} version="0.1.23"
      status={{ ...fx.gatewayStatus(), ip_addresses: ["192.168.1.42", "100.101.102.103"] }}
      network={{ ...fx.wifiStatus(), state: "connected", ssid: "Home WiFi", simulated: false }} networkError="" />
  ),
};
export const OverviewStale: Story = {
  name: "Overview / Stale, network unavailable",
  render: () => (
    <OverviewView device={fx.hardwareDevice({ stale: true, ageMs: 180_000 })} stale elapsed={0} version="0.1.23"
      status={null} network={null} networkError="Gateway request failed (502)" />
  ),
};

export const ReadingsSimulated: Story = { name: "Live readings / Simulated", render: () => <Readings device={fx.simulatedDevice()} elapsed={0} /> };
export const ReadingsHardware: Story = { name: "Live readings / Hardware", render: () => <Readings device={fx.hardwareDevice()} elapsed={0} /> };
export const ReadingsStale: Story = {
  name: "Live readings / Stale",
  render: () => <Readings device={fx.hardwareDevice({ stale: true, ageMs: 180_000 })} elapsed={0} />,
};

export const HistoryPage: Story = { name: "History / Heating temperatures", render: () => <History /> };
export const HistoryEmpty: Story = {
  name: "History / No data yet",
  parameters: { api: { "GET /api/v1/history": { body: { from: fx.NOW - 86_400_000, to: fx.NOW, series: [] } } } },
  render: () => <History />,
};
export const HistoryData: Story = {
  name: "History / Table of values",
  render: () => <HistoryTable lines={toLines(fx.history([40013, 40014], fx.NOW - 3_600_000, fx.NOW, 6).series, false)} />,
};

export const ControlsDisabled: Story = {
  name: "Controls / Disabled until verified",
  render: () => <Controls device={fx.simulatedDevice()} stale={false} refresh={() => {}} />,
};
export const ControlsSimulation: Story = {
  name: "Controls / Simulation enabled",
  render: () => <Controls device={{ ...fx.simulatedDevice(), controls_enabled: true }} stale={false} refresh={() => {}} />,
};
export const ControlsHardware: Story = {
  name: "Controls / Hardware (owner-approved)",
  render: () => <Controls device={fx.hardwareDevice()} stale={false} refresh={() => {}} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const offset = canvas.getByLabelText("Offset");
    await userEvent.clear(offset);
    await userEvent.type(offset, "-2");
    await userEvent.click(canvas.getByRole("button", { name: "Apply offset" }));
    await expect(await canvas.findByText("Heating offset set to -2 and read back.")).toBeVisible();
  },
};

export const RegistersSimulated: Story = {
  name: "Register explorer / Simulated",
  render: () => <Registers device={fx.simulatedDevice()} elapsed={0} refresh={() => {}} />,
};
export const RegistersHardware: Story = {
  name: "Register explorer / Hardware with notes",
  parameters: hw,
  render: () => <Registers device={fx.hardwareDevice()} elapsed={0} refresh={() => {}} />,
};
export const RegistersSearch: Story = {
  name: "Register explorer / Search",
  parameters: hw,
  render: () => <Registers device={fx.hardwareDevice()} elapsed={0} refresh={() => {}} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("button", { name: "Relevant on this pump" });
    await userEvent.click(canvas.getByRole("button", { name: "All registers" }));
    await userEvent.type(canvas.getByLabelText("Search registers"), "hw");
    await expect(canvas.getByText("bt7-hw-top-40013")).toBeVisible();
  },
};
export const RegisterCell: Story = {
  name: "Register explorer / Register summary cell",
  render: () => {
    const device = fx.hardwareDevice();
    const notes = fx.registerNotes();
    const def = device.catalog.find((d) => d.id === 40033);
    return (
      <div style={{ background: "var(--color-surface)", padding: 24, maxWidth: 360 }}>
        <RegisterSummary id={40033} definition={def} reading={device.registers.find((r) => r.id === 40033)} note={notes[40033]} />
        <hr />
        <NoteBadges note={{ ...notes[40004], relevance: "absent" }} /> <VerificationBadge state="hardware_verified" />
      </div>
    );
  },
};
export const RegisterRows: Story = {
  name: "Register explorer / Table",
  render: () => {
    const device = fx.hardwareDevice();
    const notes = fx.registerNotes();
    const rows = [40004, 40013, 40033, 43108, 41256].map((id) => ({
      id,
      definition: device.catalog.find((d) => d.id === id),
      reading: device.registers.find((r) => r.id === id),
      note: notes[id],
    }));
    return <div style={{ background: "var(--color-surface)", padding: 24, overflowX: "auto" }}><RegisterTable rows={rows} overlay={fx.explorer().verification_overlay} ageOf={() => "4s"} /></div>;
  },
};

export const WifiSetup: Story = {
  name: "Wi-Fi setup / Scan and connect",
  render: () => <Wifi />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole("button", { name: "Scan networks" }));
    await userEvent.click(await canvas.findByRole("button", { name: /Home WiFi/ }));
    await expect(canvas.getByLabelText("Network name (SSID)")).toHaveValue("Home WiFi");
  },
};
export const WifiUnavailable: Story = {
  name: "Wi-Fi setup / Unavailable in this build",
  parameters: { api: { "GET /api/v1/network/status": { status: 501, body: { error: { code: "unsupported", message: "Not implemented" } } } } },
  render: () => <Wifi />,
};

export const AccessHelperMissing: Story = { name: "Access / System helper missing", render: () => <Access /> };
export const AccessHardware: Story = { name: "Access / Keys, SSH and Tailscale", parameters: hw, render: () => <Access /> };
export const AccessKeyTable: Story = {
  name: "Access / API key table",
  render: () => <div style={{ background: "var(--color-surface)", padding: 24 }}><ApiKeyTable keys={fx.apiKeys()} onRevoke={() => {}} /></div>,
};

export const IntegrationsOff: Story = { name: "Integrations / MQTT off", render: () => <Integrations /> };
export const IntegrationsConnected: Story = { name: "Integrations / MQTT connected", parameters: hw, render: () => <Integrations /> };

export const UpdatesUnavailable: Story = { name: "Updates / Updater unavailable", render: () => <Updates version="0.1.23" /> };
export const UpdatesAvailable: Story = { name: "Updates / Appliance", parameters: hw, render: () => <Updates version="0.1.23" /> };

export const DiagnosticsPage: Story = { name: "Diagnostics / Counters and report", render: () => <Diagnostics device={fx.hardwareDevice()} /> };
export const DebugPage: Story = { name: "Debug / Authorized", parameters: hw, render: () => <Debug /> };

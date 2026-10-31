import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { Counter, CounterGrid, NetworkList, NetworkOption, QuietNote, SnapshotList, SnapshotRow } from "./Lists";

const meta = {
  title: "Data display/Lists",
  component: SnapshotList,
  tags: ["autodocs"],
  args: { children: null },
  decorators: [(Story) => <div style={{ maxWidth: 520, background: "var(--color-surface)", padding: 24 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component:
          "Repeating rows: `SnapshotList` of `SnapshotRow` (label + Quantity), `CounterGrid` of `Counter`, " +
          "`NetworkList` of selectable `NetworkOption` buttons, and the closing `QuietNote`.",
      },
    },
  },
} satisfies Meta<typeof SnapshotList>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Snapshot: Story = {
  render: () => (
    <SnapshotList>
      <SnapshotRow label="outdoor temperature" value="7.5" unit="°C" />
      <SnapshotRow label="hot water temperature" value="48" unit="°C" />
      <SnapshotRow label="compressor running" value="On" />
      <SnapshotRow label="heating offset" value="0" />
    </SnapshotList>
  ),
};
export const Counters: Story = {
  render: () => (
    <CounterGrid>
      <Counter value={48211} label="polls" />
      <Counter value={48102} label="updates" />
      <Counter value={0} label="transport errors" />
      <Counter value={7} label="timeouts" />
      <Counter value={1} label="malformed frames" />
      <Counter value={14} label="commands accepted" />
    </CounterGrid>
  ),
};
export const Networks: Story = {
  render: function Render() {
    const [ssid, setSsid] = useState("");
    return (
      <NetworkList>
        {[
          { ssid: "Home WiFi", signal: 80, secured: true },
          { ssid: "Guest WiFi", signal: 55, secured: false },
          { ssid: "", signal: 22, secured: true },
        ].map((n, i) => (
          <NetworkOption
            key={i}
            name={n.ssid || "Hidden network"}
            detail={`${n.signal}% · ${n.secured ? "Secured" : "Open"}`}
            selected={ssid === n.ssid && !!n.ssid}
            onSelect={() => setSsid(n.ssid)}
          />
        ))}
      </NetworkList>
    );
  },
  play: async ({ canvasElement }) => {
    const option = within(canvasElement).getByRole("button", { name: /Home WiFi/ });
    await userEvent.click(option);
    await expect(option).toHaveClass("selected");
  },
};
export const Note: Story = {
  render: () => <QuietNote>Register observations do not grant write permission. Controls require physical verification.</QuietNote>,
};

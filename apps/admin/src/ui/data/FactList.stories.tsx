import type { Meta, StoryObj } from "@storybook/react-vite";
import { Fact, FactList } from "./FactList";

const meta = {
  title: "Data display/FactList",
  component: FactList,
  tags: ["autodocs"],
  decorators: [(Story) => <div style={{ maxWidth: 420, background: "var(--color-surface)", padding: 24 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component:
          "Label → value rows (`dl`). Values right-aligned and wrap anywhere (IPs, fingerprints). " +
          "A `null`/`undefined` value reads **Unavailable** — the UI never invents data the API did not report.",
      },
    },
  },
} satisfies Meta<typeof FactList>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Items: Story = {
  args: {
    items: [
      ["Software", "0.1.23"],
      ["Device firmware", undefined],
      ["Identity verification", "hardware verified"],
      ["IP addresses", "192.168.1.42, 100.101.102.103"],
      ["Controls", "Enabled (verified controls only)"],
    ],
  },
};
export const ComposedRows: Story = {
  render: () => (
    <FactList>
      <Fact label="Readings"><code>nibe/register/&lt;id&gt;</code></Fact>
      <Fact label="Availability"><code>nibe/availability</code></Fact>
      <Fact label="Commands">None enabled in the gateway configuration</Fact>
    </FactList>
  ),
};

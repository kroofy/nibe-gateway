import type { Meta, StoryObj } from "@storybook/react-vite";
import { Quantity } from "./Quantity";
import { ReadingCard, ReadingError, ReadingGrid, ReadingHeader, ReadingMeta } from "./ReadingCard";

const meta = {
  title: "Data display/ReadingCard",
  component: ReadingCard,
  tags: ["autodocs"],
  args: {
    name: "bt1-outdoor-temperature-40004",
    value: "4.2",
    unit: "°C",
    stale: false,
    meta: "Register 40004 · 4s ago",
  },
  decorators: [(Story, { parameters }) => <div style={{ maxWidth: parameters.wide ? 900 : 420 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component: [
          "One live measurement. Anatomy:",
          "",
          "```",
          "ReadingCard (div.reading)",
          "├── ReadingHeader   name · FreshnessBadge (Current | Stale)",
          "├── Quantity        value + unit (32 px)",
          "├── ReadingMeta     register id · age",
          "└── ReadingError    optional, red",
          "```",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof ReadingCard>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Current: Story = {};
export const Stale: Story = { args: { stale: true, meta: "Register 40004 · 3m ago" } };
export const Boolean: Story = { args: { name: "compressor running", value: "On", unit: undefined, meta: "Register 3 · <1s ago" } };
export const WithError: Story = {
  args: { stale: true, error: "Read timed out; showing the last known value.", meta: "Register 43136 · 6m ago", name: "compressor-frequency-actual-43136", value: "54.3", unit: "Hz" },
};
export const Composed: Story = {
  render: () => (
    <ReadingCard>
      <ReadingHeader name="bt7-hw-top-40013" stale={false} />
      <Quantity value="49.5" unit="°C" />
      <ReadingMeta>Register 40013 · 4s ago</ReadingMeta>
      <ReadingError>Example error line</ReadingError>
    </ReadingCard>
  ),
};
export const Grid: Story = {
  parameters: { wide: true },
  render: () => (
    <ReadingGrid>
      <ReadingCard name="bt1-outdoor-temperature-40004" value="4.2" unit="°C" meta="Register 40004 · 4s ago" />
      <ReadingCard name="bt2-supply-temp-s1-40008" value="34.6" unit="°C" meta="Register 40008 · 4s ago" />
      <ReadingCard name="compressor-frequency-actual-43136" value="54.3" unit="Hz" meta="Register 43136 · 4s ago" />
      <ReadingCard name="degree-minutes-16-bit-43005" value="-142" stale meta="Register 43005 · 3m ago" />
    </ReadingGrid>
  ),
};

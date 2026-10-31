import type { Meta, StoryObj } from "@storybook/react-vite";
import { MetricCard, MetricDetail, MetricGrid, MetricLabel, MetricValue } from "./MetricCard";

const meta = {
  title: "Data display/MetricCard",
  component: MetricCard,
  tags: ["autodocs"],
  args: { label: "Device communication", value: "connected", detail: "Receiving current observations", tone: "green" },
  argTypes: { tone: { control: "inline-radio", options: ["neutral", "green", "amber"] } },
  decorators: [(Story, { parameters }) => (parameters.wide ? <Story /> : <div style={{ maxWidth: 360 }}><Story /></div>)],
  parameters: {
    docs: {
      description: {
        component: [
          "Headline status. Anatomy:",
          "",
          "```",
          "MetricCard (section.metric, tone)",
          "├── MetricLabel    what is measured",
          "├── MetricValue    the state, coloured by tone",
          "└── MetricDetail   why / since when",
          "```",
          "",
          "On phones under 480 px a card becomes a row: label and value side by side, detail below.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof MetricCard>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Healthy: Story = {};
export const Attention: Story = {
  args: { label: "Data freshness", value: "Stale", detail: "Last observation 3m ago", tone: "amber" },
};
export const Neutral: Story = {
  args: { label: "Network", value: "Setup access point", detail: "Simulated network adapter", tone: "neutral" },
};
/** The same card written with explicit parts. */
export const Composed: Story = {
  render: () => (
    <MetricCard tone="green">
      <MetricLabel>Data freshness</MetricLabel>
      <MetricValue>Current</MetricValue>
      <MetricDetail>Last observation 4s ago</MetricDetail>
    </MetricCard>
  ),
};
export const Grid: Story = {
  parameters: { wide: true },
  render: () => (
    <MetricGrid>
      <MetricCard label="Device communication" value="connected" detail="Receiving current observations" tone="green" />
      <MetricCard label="Network" value="connected" detail="Gateway network adapter" />
      <MetricCard label="Data freshness" value="Stale" detail="Last observation 3m ago" tone="amber" />
    </MetricGrid>
  ),
};
export const GridOnPhone: Story = {
  ...Grid,
  globals: { viewport: { value: "phone" } },
};

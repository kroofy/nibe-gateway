import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge, BadgeRow, FreshnessBadge } from "./Badge";

const meta = {
  title: "Primitives/Badge",
  component: Badge,
  tags: ["autodocs"],
  args: { children: "hardware verified" },
  argTypes: { tone: { control: "inline-radio", options: ["neutral", "green", "amber", "light"] } },
  parameters: {
    docs: {
      description: {
        component:
          "Short status labels. Text is capitalised unless `plain`. Tone carries meaning: " +
          "**green** current / verified, **amber** needs attention, **neutral** counts and states, " +
          "**light** only on the dark hero banner.",
      },
    },
  },
} satisfies Meta<typeof Badge>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Neutral: Story = { args: { children: "24 readings" } };
export const Green: Story = { args: { tone: "green" } };
export const Amber: Story = { args: { tone: "amber", children: "unverified" } };
export const Light: Story = {
  args: { tone: "light", children: "SIMULATED DEVICE" },
  decorators: [(Story) => <div style={{ background: "var(--color-brand-deep)", padding: 24, borderRadius: 10, display: "inline-block" }}><Story /></div>],
};
export const Plain: Story = { args: { tone: "green", plain: true, children: "Checked against the display" } };

/** `FreshnessBadge` is the Current/Stale pair used by reading cards. */
export const Freshness: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 8 }}>
      <FreshnessBadge stale={false} />
      <FreshnessBadge stale />
    </div>
  ),
};

/** `BadgeRow` wraps several badges under a title, e.g. register notes. */
export const Row: Story = {
  render: () => (
    <div style={{ maxWidth: 260 }}>
      <strong>bt50-room-temp-s1-40033</strong>
      <BadgeRow>
        <Badge tone="green" plain>Checked against the display</Badge>
        <Badge tone="amber" plain>Suspicious</Badge>
        <Badge plain>Not used on this pump</Badge>
      </BadgeRow>
    </div>
  ),
};

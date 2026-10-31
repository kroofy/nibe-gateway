import type { Meta, StoryObj } from "@storybook/react-vite";
import { HeroBanner, PumpGlyph } from "./HeroBanner";

const meta = {
  title: "Data display/HeroBanner",
  component: HeroBanner,
  tags: ["autodocs"],
  args: {
    badge: "SIMULATED DEVICE",
    title: "F730 (synthetic)",
    description: "A safe environment for gateway setup and testing.",
  },
  parameters: {
    docs: {
      description: {
        component:
          "Dark brand card at the top of the overview. Anatomy: `Badge tone=light` (mode) · h2 (model) · " +
          "one sentence · `art` slot (defaults to `PumpGlyph`). Use once per page at most.",
      },
    },
  },
} satisfies Meta<typeof HeroBanner>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Simulated: Story = {};
export const Hardware: Story = {
  args: { badge: "HARDWARE", title: "F730 Cu 3x400V", description: "Connected through your local heat pump gateway." },
};
export const UnknownIdentity: Story = { args: { badge: "REPLAY", title: "Device identity unavailable" } };
export const Glyph: Story = {
  render: () => (
    <div style={{ background: "var(--color-brand-deep)", padding: 32, borderRadius: 10, display: "inline-block" }}>
      <PumpGlyph />
    </div>
  ),
};

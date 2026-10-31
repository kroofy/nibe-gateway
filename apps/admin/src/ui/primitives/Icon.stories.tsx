import type { Meta, StoryObj } from "@storybook/react-vite";
import { ICON_PATHS, Icon } from "./Icon";
import type { IconName } from "./Icon";

const meta = {
  title: "Primitives/Icon",
  component: Icon,
  tags: ["autodocs"],
  args: { name: "overview" },
  argTypes: { name: { control: "select", options: Object.keys(ICON_PATHS) } },
  parameters: {
    docs: {
      description: {
        component:
          "Single-path 24 × 24 line icons, 1.6 px stroke, `currentColor`. Decorative (`aria-hidden`); " +
          "the adjacent text names the thing. No icon font or external assets.",
      },
    },
  },
} satisfies Meta<typeof Icon>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Single: Story = {};
export const Gallery: Story = {
  render: () => (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 120px)", gap: 16, color: "var(--color-brand)" }}>
      {(Object.keys(ICON_PATHS) as IconName[]).map((name) => (
        <div key={name} style={{ display: "grid", justifyItems: "center", gap: 6, fontSize: 11 }}>
          <Icon name={name} size={28} />
          {name}
        </div>
      ))}
    </div>
  ),
};

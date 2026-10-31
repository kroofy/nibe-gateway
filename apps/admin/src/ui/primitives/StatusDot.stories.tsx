import type { Meta, StoryObj } from "@storybook/react-vite";
import { StatusDot } from "./StatusDot";

const meta = {
  title: "Primitives/StatusDot",
  component: StatusDot,
  tags: ["autodocs"],
  parameters: {
    docs: { description: { component: "Decorative 7 px health dot. Always paired with text; never the only signal." } },
  },
} satisfies Meta<typeof StatusDot>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Ok: Story = { render: (args) => <span><StatusDot {...args} />Local connection</span> };
export const Warning: Story = { args: { tone: "warning" }, render: (args) => <span><StatusDot {...args} />Reconnecting</span> };

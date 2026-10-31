import type { Meta, StoryObj } from "@storybook/react-vite";
import { CodeBlock, SecretValue } from "./Code";

const meta = {
  title: "Primitives/Code",
  component: CodeBlock,
  tags: ["autodocs"],
  parameters: { docs: { description: { component: "Monospace output: `CodeBlock` for JSON, `SecretValue` for a one-time key." } } },
} satisfies Meta<typeof CodeBlock>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Block: Story = {
  args: { children: JSON.stringify({ polls: 48211, updates: 48102, transport_errors: 0, timeouts: 7 }, null, 2) },
};
export const Secret: Story = {
  args: { children: "" },
  render: () => <SecretValue>ngw_3f9a1c27e4b84d0f9a6e2c51b7d08e46</SecretValue>,
};

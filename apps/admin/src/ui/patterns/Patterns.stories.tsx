import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import { ActionMessages } from "./MessageStack";
import { OneTimeSecret } from "./OneTimeSecret";

const meta = {
  title: "Patterns/Feedback",
  component: OneTimeSecret,
  tags: ["autodocs"],
  args: {
    title: "Copy this key now; it is not shown again.",
    value: "ngw_3f9a1c27e4b84d0f9a6e2c51b7d08e46",
    onCopy: fn(),
    onDone: fn(),
  },
  parameters: {
    docs: {
      description: {
        component:
          "Small compositions of primitives that recur across pages. `OneTimeSecret` = warning Notice + " +
          "title + SecretValue + Copy/Done buttons. `ActionMessages` = the error and success notices every " +
          "mutating page shows above its panels.",
      },
    },
  },
} satisfies Meta<typeof OneTimeSecret>;
export default meta;
type Story = StoryObj<typeof meta>;

export const SecretShownOnce: Story = {};
export const Messages: Story = {
  render: () => <ActionMessages error="The broker refused the connection (not authorised)." message="Saved. The gateway reconnects on its own." />,
};

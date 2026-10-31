import type { Meta, StoryObj } from "@storybook/react-vite";
import { Button } from "./Button";
import { Notice } from "./Notice";

const meta = {
  title: "Primitives/Notice",
  component: Notice,
  tags: ["autodocs"],
  argTypes: { kind: { control: "inline-radio", options: ["info", "warning", "error"] } },
  parameters: {
    docs: {
      description: {
        component:
          "Full-width message above or inside panels. `error` uses `role=alert` (announced immediately); " +
          "`info` and `warning` use `role=status`. Keep the text actionable: what happened, what to do.",
      },
    },
  },
} satisfies Meta<typeof Notice>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Info: Story = {
  args: { children: "Simulation-only controls are enabled. These limits do not authorize physical F730 writes." },
};
export const Warning: Story = {
  args: {
    kind: "warning",
    children: "Readings are stale. Last known values are shown; check device communication before relying on them.",
  },
};
export const Error: Story = { args: { kind: "error", children: "Device: serial read timed out after 3 retries" } };
/** A notice may carry one inline action. */
export const WithAction: Story = {
  args: {
    kind: "error",
    children: (
      <>
        Gateway request failed (502){" "}
        <Button variant="text">Retry connection</Button>
      </>
    ),
  },
};
export const OutdatedPage: Story = {
  args: {
    kind: "warning",
    children: (
      <>
        Gateway version 0.1.24 is installed; this page is still 0.1.23. <Button>Reload</Button>
      </>
    ),
  },
};

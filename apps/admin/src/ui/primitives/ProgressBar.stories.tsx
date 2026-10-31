import type { Meta, StoryObj } from "@storybook/react-vite";
import { ProgressBar } from "./ProgressBar";

const meta = {
  title: "Primitives/ProgressBar",
  component: ProgressBar,
  tags: ["autodocs"],
  args: { label: "Downloading 0.1.28", value: 4.2, max: 10.8 },
  parameters: {
    docs: {
      description: {
        component:
          "Progress of a long-running task, such as downloading a release. With `value`/`max` it shows the share " +
          "done; without them it is indeterminate. The label is the accessible name; `detail` replaces the percentage.",
      },
    },
  },
  decorators: [(Story) => <div style={{ maxWidth: 420 }}><Story /></div>],
} satisfies Meta<typeof ProgressBar>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Determinate: Story = {};
export const WithDetail: Story = { args: { detail: "4.2 of 10.8 MiB" } };
export const Indeterminate: Story = { args: { label: "Verifying signature and staging", value: undefined, max: undefined } };

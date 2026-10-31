import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { FilterGroup, SegmentedControl } from "./SegmentedControl";

const ranges = ["24 h", "7 days", "30 days", "1 year", "2 years"].map((label, value) => ({ value, label }));

const meta = {
  title: "Forms/SegmentedControl",
  component: SegmentedControl,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "One-of-many choice made of buttons (`aria-pressed` marks the active one). Use for a handful of " +
          "short options that change a view; use a select for long lists. `FilterGroup` adds a visible label.",
      },
    },
  },
  args: { options: ranges, value: 0, onChange: () => {}, "aria-label": "Time range" },
} satisfies Meta<typeof SegmentedControl<number>>;
export default meta;
type Story = StoryObj<typeof meta>;

export const TimeRange: Story = {
  render: function Render(args) {
    const [value, setValue] = useState(args.value);
    return <SegmentedControl {...args} value={value} onChange={setValue} />;
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "30 days" }));
    await expect(canvas.getByRole("button", { name: "30 days" })).toHaveAttribute("aria-pressed", "true");
    await expect(canvas.getByRole("button", { name: "24 h" })).toHaveAttribute("aria-pressed", "false");
  },
};

export const LabelledFilter: Story = {
  render: function Render() {
    const [value, setValue] = useState<"relevant" | "all">("relevant");
    return (
      <FilterGroup
        id="story-show"
        label="Show"
        options={[
          { value: "relevant", label: "Relevant on this pump" },
          { value: "all", label: "All registers" },
        ]}
        value={value}
        onChange={setValue}
      />
    );
  },
};

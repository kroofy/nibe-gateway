import type { Meta, StoryObj } from "@storybook/react-vite";
import { Button } from "../primitives/Button";
import { Field } from "../forms/Field";
import { SegmentedControl } from "../forms/SegmentedControl";
import { ActionRow, Columns, FilterBar, Toolbar } from "./Layout";

const meta = {
  title: "Layout/Arrangement",
  component: ActionRow,
  tags: ["autodocs"],
  args: { children: null },
  parameters: {
    docs: {
      description: {
        component:
          "Layout helpers that position components without styling them: `Columns` (two up), `ActionRow` " +
          "(wrapping buttons), `Toolbar` (growing field + actions, stacks under 1100 px) and `FilterBar` " +
          "(chart filters).",
      },
    },
  },
} satisfies Meta<typeof ActionRow>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Actions: Story = {
  render: () => (
    <ActionRow>
      <Button disabled>Upload and verify</Button>
      <Button disabled>Activate staged release</Button>
      <Button variant="secondary">Refresh update status</Button>
    </ActionRow>
  ),
};
export const SearchToolbar: Story = {
  render: () => (
    <Toolbar>
      <Field label="Search registers"><input type="search" placeholder="Search name, ID or verification…" /></Field>
      <Button variant="secondary">Collect passive observations</Button>
    </Toolbar>
  ),
};
export const ChartFilters: Story = {
  render: () => (
    <FilterBar>
      <Field label="Chart">
        <select><option>Heating temperatures</option><option>Hot water</option></select>
      </Field>
      <SegmentedControl aria-label="Time range" options={[{ value: 0, label: "24 h" }, { value: 1, label: "7 days" }]} value={0} onChange={() => {}} />
    </FilterBar>
  ),
};
export const TwoColumns: Story = {
  render: () => (
    <Columns>
      <div style={{ background: "var(--color-surface)", border: "1px dashed var(--color-border-strong)", padding: 24 }}>Left</div>
      <div style={{ background: "var(--color-surface)", border: "1px dashed var(--color-border-strong)", padding: 24 }}>Right</div>
    </Columns>
  ),
};

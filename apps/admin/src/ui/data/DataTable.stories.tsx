import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge, BadgeRow } from "../primitives/Badge";
import { Button } from "../primitives/Button";
import { ActionHeader, Cell, CellLabel, CellMeta, CellTitle, EmptyRow, Pagination, RowActionCell, Table, TableScroll } from "./DataTable";

const meta = {
  title: "Data display/DataTable",
  component: Table,
  tags: ["autodocs"],
  args: { children: null },
  decorators: [(Story) => <div style={{ background: "var(--color-surface)", padding: 24 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component: [
          "Native table markup with design-system parts. A rich first cell stacks:",
          "",
          "```",
          "td",
          "├── CellTitle   name (bold)",
          "├── CellLabel   this installation's label (darker)",
          "├── BadgeRow    note badges",
          "└── CellMeta…   id/access, source/catalog, errors (muted, small)",
          "```",
          "",
          "Wrap wide tables in `TableScroll`; use `Table compact` for simple lists with `RowActionCell`.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof Table>;
export default meta;
type Story = StoryObj<typeof meta>;

export const RichRows: Story = {
  render: () => (
    <TableScroll label="Registers">
      <Table>
        <thead>
          <tr><th>Register</th><th>Raw</th><th>Decoded</th><th>Verification</th><th>Observation</th><th>Age</th></tr>
        </thead>
        <tbody>
          <tr>
            <td>
              <CellTitle>bt1-outdoor-temperature-40004</CellTitle>
              <CellLabel>Outdoor (north wall)</CellLabel>
              <BadgeRow><Badge tone="green" plain>Checked against the display</Badge></BadgeRow>
              <CellMeta>#40004 · read only</CellMeta>
              <CellMeta>nibepi-f730, nibe-modbusmanager-f730-db8310 · catalog 2026.10.22-1</CellMeta>
            </td>
            <Cell mono>42</Cell>
            <Cell mono>4.2 °C</Cell>
            <td><Badge tone="green">hardware verified</Badge></td>
            <td>confirmed readable</td>
            <td>4s</td>
          </tr>
          <tr>
            <td>
              <CellTitle>compressor-frequency-actual-43136</CellTitle>
              <CellMeta>#43136 · read only</CellMeta>
            </td>
            <Cell mono>—</Cell>
            <Cell mono>— Hz</Cell>
            <td><Badge tone="amber">unverified</Badge></td>
            <td>catalog known<CellMeta error>Read timed out</CellMeta></td>
            <td>—</td>
          </tr>
        </tbody>
      </Table>
    </TableScroll>
  ),
};
export const CompactWithActions: Story = {
  render: () => (
    <Table compact>
      <thead><tr><th>Name</th><th>Scope</th><th>Last used</th><ActionHeader /></tr></thead>
      <tbody>
        <tr><td>home-assistant</td><td>read</td><td>40 s ago</td><RowActionCell><Button variant="secondary" size="compact">Revoke</Button></RowActionCell></tr>
        <tr><td>tablet-dashboard</td><td>control</td><td>Not since start</td><RowActionCell><Button variant="secondary" size="compact">Revoke</Button></RowActionCell></tr>
      </tbody>
    </Table>
  ),
};
export const Empty: Story = {
  render: () => (
    <Table compact>
      <thead><tr><th>Key</th><th>Fingerprint</th><ActionHeader /></tr></thead>
      <tbody><EmptyRow colSpan={3}>No keys. SSH is off.</EmptyRow></tbody>
    </Table>
  ),
};
export const Paging: Story = {
  render: function Render() {
    const [page, setPage] = useState(0);
    return <Pagination label="Register pages" page={page} pageCount={12} total={581} onPage={setPage} />;
  },
};

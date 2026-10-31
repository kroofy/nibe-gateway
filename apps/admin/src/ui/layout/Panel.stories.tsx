import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge } from "../primitives/Badge";
import { Button, InlineLink } from "../primitives/Button";
import { FactList } from "../data/FactList";
import { Field } from "../forms/Field";
import { ActionRow, Columns } from "./Layout";
import { Panel, PanelDescription, PanelHeader } from "./Panel";

const meta = {
  title: "Layout/Panel",
  component: Panel,
  tags: ["autodocs"],
  args: { title: "Gateway details" },
  parameters: {
    docs: {
      description: {
        component: [
          "The card every page is built from. Anatomy:",
          "",
          "```",
          "Panel (section.panel)",
          "├── PanelHeader          title (h2) · action slot (Badge | InlineLink | ActionRow compact)",
          "├── PanelDescription     muted paragraph(s)",
          "└── body                 FactList | DataTable | ReadingGrid | SnapshotList | form | …",
          "```",
          "",
          "`<Panel title action>` is shorthand for a PanelHeader as the first child. Inside `Columns`, " +
            "panels sit side by side and lose their bottom margin.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof Panel>;
export default meta;
type Story = StoryObj<typeof meta>;

export const WithFacts: Story = {
  args: {
    children: (
      <FactList items={[["Software", "0.1.23"], ["Device firmware", "9721"], ["Uptime", "93784 s"]]} />
    ),
  },
};
export const WithBadgeAction: Story = {
  args: {
    title: "Register catalog",
    action: <Badge>581 registers</Badge>,
    children: <PanelDescription>Passive discovery only. Observation and catalog metadata never grant write permission.</PanelDescription>,
  },
};
export const WithLinkAction: Story = {
  args: { title: "Live snapshot", action: <InlineLink href="#">All readings ↗</InlineLink>, children: <p>…</p> },
};

/** The explicit parts, for headers with more than one action. */
export const ComposedParts: Story = {
  args: { title: undefined },
  render: () => (
    <Panel>
      <PanelHeader
        title="Heating temperatures"
        action={
          <ActionRow compact>
            <Button variant="secondary" size="compact">Show table</Button>
            <Button variant="secondary" size="compact">Download CSV</Button>
          </ActionRow>
        }
      />
      <PanelDescription>Every reading; lines show the bucket mean.</PanelDescription>
    </Panel>
  ),
};

/** A form panel: description, fields, one primary action. */
export const FormPanel: Story = {
  args: {
    title: "Heating offset",
    children: (
      <>
        <PanelDescription>Heat offset S1 (register 47011), -10 to 10.</PanelDescription>
        <form onSubmit={(e) => e.preventDefault()}>
          <Field label="Offset"><input type="number" defaultValue={-1} /></Field>
          <Button>Apply offset</Button>
        </form>
      </>
    ),
  },
};

/** Two panels side by side; one column under 760 px. */
export const InColumns: Story = {
  parameters: { layout: "fullscreen" },
  render: () => (
    <div style={{ padding: 24 }}>
      <Columns>
        <Panel title="SSH" action={<Badge>On</Badge>}>
          <PanelDescription>Key-only login for <code>nibe-admin</code>. Removing every key turns SSH off.</PanelDescription>
        </Panel>
        <Panel title="Tailscale" action={<Badge>Running</Badge>}>
          <FactList items={[["Device", "nibe-gateway.tail1234.ts.net."], ["Tailnet IP", "100.101.102.103"]]} />
        </Panel>
      </Columns>
    </div>
  ),
};

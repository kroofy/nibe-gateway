import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { Button } from "../primitives/Button";
import { CheckboxField, Disclosure, Field, FieldHelp } from "./Field";

const meta = {
  title: "Forms/Field",
  component: Field,
  tags: ["autodocs"],
  args: { label: "Broker host", children: <input placeholder="192.168.1.10 or broker.lan" /> },
  decorators: [(Story) => <div style={{ maxWidth: 420 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component:
          "A `label` wrapping one native control. Inputs, selects, textareas, checkboxes and file inputs " +
          "are plain HTML elements styled by the base layer (44 px tall, 7 px radius); a Field only adds " +
          "the label and optional help. Wrapping keeps the label and control associated without ids.",
      },
    },
  },
} satisfies Meta<typeof Field>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Text: Story = {};
export const WithHelp: Story = {
  args: {
    label: "Device setup token",
    help: "Use the unique token supplied with this gateway. In simulation, read setup-secrets.json in the data directory.",
    children: <input type="password" autoComplete="off" />,
  },
};
export const Select: Story = {
  args: {
    label: "Mode",
    children: (
      <select defaultValue="three_hours">
        <option value="one_time_increase">One-time increase</option>
        <option value="three_hours">3 hours</option>
        <option value="off">Off</option>
      </select>
    ),
  },
};
export const NumberInput: Story = { args: { label: "Offset", children: <input type="number" min={-10} max={10} defaultValue={-1} /> } };
export const Textarea: Story = {
  args: { label: "Add public key", children: <textarea rows={3} placeholder="ssh-ed25519 AAAA… user@laptop" /> },
};
export const File: Story = { args: { label: "Signed release (.tar)", children: <input type="file" accept=".tar" /> } };
export const Disabled: Story = { args: { label: "Discovery prefix", children: <input value="homeassistant" disabled readOnly /> } };

export const Checkbox: Story = {
  render: function Render() {
    const [checked, setChecked] = useState(true);
    return <CheckboxField label="Home Assistant discovery" checked={checked} onChange={setChecked} />;
  },
  play: async ({ canvasElement }) => {
    const box = within(canvasElement).getByRole("checkbox", { name: "Home Assistant discovery" });
    await expect(box).toBeChecked();
    await userEvent.click(box);
    await expect(box).not.toBeChecked();
  },
};

export const Help: Story = {
  render: () => <FieldHelp>Credentials are sent only to this gateway and are never included in support reports.</FieldHelp>,
};

/** A secondary form kept closed until needed. */
export const DisclosureForm: Story = {
  render: () => (
    <Disclosure summary="Re-join with a new auth key">
      <form onSubmit={(e) => e.preventDefault()}>
        <Field label="Auth key"><input type="password" placeholder="tskey-auth-…" /></Field>
        <Button>Join tailnet</Button>
      </form>
    </Disclosure>
  ),
};

/** How forms are composed: Fields stacked, help between, one primary submit. */
export const FormComposition: Story = {
  render: () => (
    <form onSubmit={(e) => e.preventDefault()}>
      <Field label="Network name (SSID)"><input defaultValue="Home WiFi" /></Field>
      <Field label="Wi-Fi password"><input type="password" /></Field>
      <FieldHelp>Credentials are sent only to this gateway and are never included in support reports.</FieldHelp>
      <Button>Connect and verify</Button>
    </form>
  ),
};

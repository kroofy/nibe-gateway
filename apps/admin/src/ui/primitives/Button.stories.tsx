import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import { Button, ButtonLink, InlineLink } from "./Button";

const meta = {
  title: "Primitives/Button",
  component: Button,
  tags: ["autodocs"],
  args: { children: "Apply offset", onClick: fn() },
  argTypes: {
    variant: { control: "inline-radio", options: ["primary", "secondary", "text"] },
    size: { control: "inline-radio", options: ["default", "compact"] },
  },
  parameters: {
    docs: {
      description: {
        component:
          "Every action in the admin UI. **Primary** commits a change (one per form), **secondary** is " +
          "for safe or reversible actions, **text** for low-emphasis actions in bars and notices. " +
          "`compact` is for table rows and panel headers. Buttons are at least 44 px tall for touch.",
      },
    },
  },
} satisfies Meta<typeof Button>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Primary: Story = {
  play: async ({ canvasElement, args }) => {
    await userEvent.click(within(canvasElement).getByRole("button", { name: "Apply offset" }));
    await expect(args.onClick).toHaveBeenCalledOnce();
  },
};
export const Secondary: Story = { args: { variant: "secondary", children: "Scan networks" } };
export const Text: Story = { args: { variant: "text", children: "Sign out" } };
export const Compact: Story = { args: { variant: "secondary", size: "compact", children: "Revoke" } };
export const Disabled: Story = {
  args: { disabled: true, children: "Set Temporary Lux" },
  play: async ({ canvasElement, args }) => {
    const button = within(canvasElement).getByRole("button");
    await expect(button).toBeDisabled();
    await userEvent.click(button, { pointerEventsCheck: 0 });
    await expect(args.onClick).not.toHaveBeenCalled();
  },
};

/** All variants and states side by side. Adjacent buttons get an 8 px gap. */
export const Matrix: Story = {
  render: () => (
    <div style={{ display: "grid", gap: 16 }}>
      <div>
        <Button>Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="text">Text</Button>
      </div>
      <div>
        <Button disabled>Primary</Button>
        <Button variant="secondary" disabled>Secondary</Button>
        <Button variant="text" disabled>Text</Button>
      </div>
      <div>
        <Button size="compact">Compact</Button>
        <Button variant="secondary" size="compact">Compact secondary</Button>
      </div>
    </div>
  ),
};

/** Links styled as actions: downloads (`ButtonLink`) and quiet header links (`InlineLink`). */
export const Links: Story = {
  render: () => (
    <div style={{ display: "flex", gap: 24, alignItems: "center" }}>
      <ButtonLink href="#">Download CSV</ButtonLink>
      <InlineLink href="#">All readings ↗</InlineLink>
    </div>
  ),
};

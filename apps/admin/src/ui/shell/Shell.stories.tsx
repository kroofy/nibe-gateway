import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge } from "../primitives/Badge";
import { Button } from "../primitives/Button";
import { Panel } from "../layout/Panel";
import { pages, slugs, icons, descriptions } from "../../navigation";
import {
  AppShell, AuthLayout, Brand, LiveStatus, NavItem, PageContent, PageFooter, PageHeading, Sidebar, SidebarCaption,
  SidebarFooter, SidebarNav, Topbar,
} from "./Shell";

const meta = {
  title: "Shell/AppShell",
  component: AppShell,
  tags: ["autodocs"],
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component: [
          "The frame around every signed-in page. Anatomy:",
          "",
          "```",
          "AppShell",
          "├── Sidebar                       fixed 232 px; top strip under 760 px",
          "│   ├── Brand",
          "│   ├── SidebarCaption            ADMINISTRATION",
          "│   ├── SidebarNav → NavItem×11   icon + label, aria-current on the active page",
          "│   └── SidebarFooter             StatusDot · connection · host",
          "└── workspace",
          "    ├── Topbar                    breadcrumb · mode Badge · Sign out (text Button)",
          "    └── PageContent (main)",
          "        ├── Notice…               outdated page, connection errors, stale data",
          "        ├── PageHeading           eyebrow · h1 · description · LiveStatus",
          "        ├── <page>                Panels, MetricGrid, Columns…",
          "        └── PageFooter",
          "```",
        ].join("\n"),
      },
    },
  },
  args: { sidebar: null, children: null },
} satisfies Meta<typeof AppShell>;
export default meta;
type Story = StoryObj<typeof meta>;

const sidebar = (current: string) => (
  <Sidebar>
    <Brand />
    <SidebarCaption>ADMINISTRATION</SidebarCaption>
    <SidebarNav label="Main navigation">
      {pages.map((name, i) => (
        <NavItem key={name} href={`#${slugs[i]}`} icon={icons[name]} current={name === current}>
          {name}
        </NavItem>
      ))}
    </SidebarNav>
    <SidebarFooter label="Local connection" detail="nibe-gateway.local" />
  </Sidebar>
);

/** The empty frame with a placeholder panel. */
export const Frame: Story = {
  render: () => (
    <AppShell sidebar={sidebar("Controls")}>
      <Topbar trail="Gateway" current="Administration">
        <Badge tone="amber">simulated</Badge>
        <Button variant="text">Sign out</Button>
      </Topbar>
      <PageContent>
        <PageHeading
          eyebrow="NIBE GATEWAY"
          title="Controls"
          description={descriptions.Controls}
          aside={<LiveStatus live>Live</LiveStatus>}
        />
        <Panel title="Page content">Panels go here.</Panel>
        <PageFooter meta="Gateway v0.1.23">Local-first. Your gateway stays in your home.</PageFooter>
      </PageContent>
    </AppShell>
  ),
};
export const FrameOnPhone: Story = { ...Frame, globals: { viewport: { value: "phone" } } };

export const BrandMark: Story = { parameters: { layout: "padded" }, render: () => <Brand /> };
export const Navigation: Story = {
  parameters: { layout: "fullscreen" },
  render: () => <div style={{ height: 760, position: "relative", transform: "translateZ(0)" }}>{sidebar("History")}</div>,
};
export const TopBar: Story = {
  render: () => (
    <Topbar trail="Gateway" current="Administration">
      <Badge>hardware</Badge>
      <Button variant="text">Sign out</Button>
    </Topbar>
  ),
};
export const Heading: Story = {
  parameters: { layout: "padded" },
  render: () => (
    <PageHeading
      eyebrow="NIBE GATEWAY"
      title="Live readings"
      description={descriptions["Live readings"]}
      aside={<LiveStatus live={false}>Reconnecting</LiveStatus>}
    />
  ),
};
export const SignedOut: Story = {
  render: () => (
    <AuthLayout footnote="Local administration · No cloud account required">
      <Panel title="Connecting to your gateway">
        <p role="status">Checking setup and session…</p>
      </Panel>
    </AuthLayout>
  ),
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { history } from "../../stories/fixtures";
import { toLines } from "../../pages/History";
import { ChartLegend, ChartTooltip, TimeSeriesChart } from "./TimeSeriesChart";

const to = Date.UTC(2026, 9, 26, 12);
const from = to - 24 * 3600e3;
const heating = toLines(history([40004, 40008, 40012, 43009], from, to).series, false);
const hotWater = toLines(history([40013, 40014], from, to).series, false);
const energy = toLines(history([41850, 41848], to - 7 * 86400e3, to, 8).series, true);

const meta = {
  title: "Charts/TimeSeriesChart",
  component: TimeSeriesChart,
  tags: ["autodocs"],
  args: { lines: heating, from, to },
  decorators: [(Story) => <div style={{ maxWidth: 820, background: "var(--color-surface)", padding: 24 }}><Story /></div>],
  parameters: {
    docs: {
      description: {
        component: [
          "Dependency-free SVG chart for history. One unit per chart (never two scales). Anatomy:",
          "",
          "```",
          "div.chart",
          "├── svg            grid + y ticks, time axis, series (lines or bars), end labels, crosshair",
          "├── ChartTooltip   values at the hovered time (top right)",
          "└── ChartLegend    only with more than one series",
          "```",
          "",
          "Series colours come from `SERIES_COLORS` in fixed order. Lines break across gaps longer than " +
            "three buckets. Without data it renders `ChartEmpty`.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof TimeSeriesChart>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Lines: Story = {};
export const TwoSeries: Story = { args: { lines: hotWater } };
export const Bars: Story = { args: { lines: energy, bars: true, from: to - 7 * 86400e3 } };
export const Hovered: Story = { args: { initialHover: heating[0].points[40].t } };
export const Empty: Story = { args: { lines: [] } };
export const TooltipAlone: Story = {
  render: () => (
    <div style={{ position: "relative", height: 110 }}>
      <ChartTooltip time={heating[0].points[40].t} lines={heating} />
    </div>
  ),
};
export const LegendAlone: Story = { render: () => <ChartLegend lines={heating} /> };

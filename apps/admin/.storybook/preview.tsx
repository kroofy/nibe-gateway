import type { Preview } from "@storybook/react-vite";
import "../src/ui/styles.css";
import "./docs-overrides.css";
import { installMockApi, simulatedApi } from "../src/stories/mockApi";
import type { MockApi, StreamState } from "../src/stories/mockApi";

const preview: Preview = {
  parameters: {
    layout: "padded",
    controls: { expanded: true, matchers: { color: /(background|color)$/i } },
    backgrounds: {
      options: {
        canvas: { name: "Canvas", value: "#f5f7f5" },
        surface: { name: "Surface", value: "#ffffff" },
        brand: { name: "Brand banner", value: "#1c4639" },
      },
    },
    viewport: {
      options: {
        phone: { name: "Service phone (390)", styles: { width: "390px", height: "844px" }, type: "mobile" },
        narrow: { name: "Narrow phone (360)", styles: { width: "360px", height: "740px" }, type: "mobile" },
        tablet: { name: "Tablet (1024)", styles: { width: "1024px", height: "768px" }, type: "tablet" },
        desktop: { name: "Desktop (1440)", styles: { width: "1440px", height: "1000px" }, type: "desktop" },
      },
    },
    // Report accessibility findings in the panel; the shipped palette has known contrast gaps (see docs).
    a11y: { test: "todo" },
    options: {
      storySort: {
        order: [
          "Introduction",
          "Foundations",
          ["Colors", "Typography", "Spacing and shape", "Palette audit"],
          "Primitives",
          "Forms",
          "Layout",
          "Data display",
          "Charts",
          "Shell",
          "Patterns",
          "Screens",
          ["Pages", "Full app"],
        ],
      },
    },
  },
  initialGlobals: { backgrounds: { value: "canvas" } },
  // Every story gets the in-browser API stand-in; page stories override routes via `parameters.api`.
  beforeEach: ({ parameters }) => {
    const routes = { ...simulatedApi(), ...((parameters.api as MockApi | undefined) ?? {}) };
    const restore = installMockApi(routes, (parameters.stream as StreamState | undefined) ?? "live");
    if (typeof parameters.hash === "string") location.hash = parameters.hash;
    return () => {
      restore();
      if (typeof parameters.hash === "string") history.replaceState(null, "", location.pathname + location.search);
    };
  },
};
export default preview;

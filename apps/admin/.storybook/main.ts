import type { StorybookConfig } from "@storybook/react-vite";
import { fileURLToPath } from "node:url";
import remarkGfm from "remark-gfm";

const config: StorybookConfig = {
  framework: "@storybook/react-vite",
  stories: ["../src/**/*.mdx", "../src/**/*.stories.@(ts|tsx)"],
  addons: [
    // GitHub-flavoured Markdown in MDX: pipe tables are not part of plain MDX.
    { name: "@storybook/addon-docs", options: { mdxPluginOptions: { mdxCompileOptions: { remarkPlugins: [remarkGfm] } } } },
    "@storybook/addon-a11y",
  ],
  core: { disableTelemetry: true },
  // Fixtures import the real F730 catalog from the repository's fixtures/ directory.
  viteFinal: (vite) => ({
    ...vite,
    server: {
      ...vite.server,
      fs: { ...vite.server?.fs, allow: [fileURLToPath(new URL("../../../", import.meta.url))] },
    },
  }),
};
export default config;

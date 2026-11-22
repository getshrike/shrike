import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  stories: [
    "../src/**/*.demo.tsx",
    { directory: "../src", files: "**/*.preview.tsx" },
  ],
  framework: "@storybook/react-vite",
};

export default config;

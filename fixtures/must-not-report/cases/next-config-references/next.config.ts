import type { NextConfig } from "next";

// './lib/commented.ts' is documentation, not a loaded module.
const config: NextConfig = {
  images: {
    loader: "custom",
    loaderFile: "./image-loader.ts",
  },
  cacheHandler: "./cache-handler.ts",
  turbopack: {
    rules: {
      "*.svg": {
        loaders: ["./loaders/svg-loader.ts"],
      },
    },
  },
};

export default config;

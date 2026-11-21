import path from "node:path";

export default {
  test: {
    setupFiles: ["./tests/setup.ts"],
  },
  resolve: {
    alias: {
      "server-only": path.resolve(__dirname, "tests/stubs/server-only.ts"),
    },
  },
};

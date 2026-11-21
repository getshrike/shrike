const path = require("path");

module.exports = {
  mode: "production",
  context: path.resolve(__dirname, "src"),
  entry: {
    app: ["./boot.js"],
    admin: { import: "./admin.js" },
  },
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "[name].js",
  },
};

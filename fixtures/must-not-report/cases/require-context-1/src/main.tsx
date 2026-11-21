import { createRoot } from "react-dom/client";

const icons = require.context("./icons", true, /\.tsx$/);

void icons;
createRoot(document.getElementById("root")!).render(<div />);

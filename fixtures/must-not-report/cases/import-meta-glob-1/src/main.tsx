import { createRoot } from "react-dom/client";

const pages = import.meta.glob(["./pages/*.tsx", "!./pages/skip.tsx"], { eager: true });

void pages;
createRoot(document.getElementById("root")!).render(<div />);

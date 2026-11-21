import { createRoot } from "react-dom/client";

import { usedOne } from "./used";

const pages = import.meta.glob(["./pages/*.tsx", "!./pages/skip.tsx"], { eager: true });

void pages;
void usedOne;
createRoot(document.getElementById("root")!).render(<div />);

import { createRoot } from "react-dom/client";
import { value } from "./consumer";
import { loadDynamic } from "./dynamic-consumer";
import { used } from "./lib";
import { NamespaceConsumer } from "./namespace";

void loadDynamic();

createRoot(document.getElementById("root")!).render(
  <div>
    {value}
    {used()}
    <NamespaceConsumer />
  </div>,
);

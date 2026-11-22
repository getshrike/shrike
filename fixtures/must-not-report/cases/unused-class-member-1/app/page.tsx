import { Counter } from "../src/counter";
import { Panel } from "../src/panel";

export default function Page() {
  const counter = new Counter();
  counter.increment();
  return (
    <main>
      <Panel label={`count: ${counter.value}`} />
    </main>
  );
}

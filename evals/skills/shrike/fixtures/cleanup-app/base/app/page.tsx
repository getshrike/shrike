import { formatInvoice } from "../src/format";
import { buildReport } from "../src/report";

export default function Page() {
  return (
    <main>
      {formatInvoice(42)} {String(buildReport())}
    </main>
  );
}

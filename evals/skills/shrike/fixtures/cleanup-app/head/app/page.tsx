import { buildReport } from "../src/report";

export default function Page() {
  return <main>{String(buildReport())}</main>;
}

import { values } from "../src/index";

export default function Page() {
  return <pre>{values.join(" ")}</pre>;
}

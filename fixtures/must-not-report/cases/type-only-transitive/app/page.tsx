import type { Host } from "../src/host";

export default function Page(props: { host: Host }) {
  return <div>{props.host.label}</div>;
}

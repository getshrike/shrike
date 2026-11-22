import { ParamView } from "../components/param-view";

export const unstable_paramMatching = { id: "dynamic" };
export function unstable_generateParamMatching() {
  return { id: "blocking" };
}

export function unusedPageHelper() {
  return 1;
}

export default function Page() {
  return <ParamView />;
}

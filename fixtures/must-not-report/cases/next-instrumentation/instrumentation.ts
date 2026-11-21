import { setup } from "./lib/setup";

export async function register() {
  setup();
}

export function onRequestError() {
  return "reported";
}

export function unusedInstrumentationHelper() {
  return "never used";
}

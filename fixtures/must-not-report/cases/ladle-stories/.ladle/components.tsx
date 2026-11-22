import type { GlobalProvider } from "@ladle/react";
import { LadleShell } from "./shell";

export const Provider: GlobalProvider = ({ children }) => {
  return <LadleShell>{children}</LadleShell>;
};

export function unusedComponentsHelper() {
  return "never used";
}

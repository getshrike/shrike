import { Frame } from "./Frame";

export default function Decorator({ children }) {
  return <Frame>{children}</Frame>;
}

export function unusedDecoratorHelper() {
  return 3;
}

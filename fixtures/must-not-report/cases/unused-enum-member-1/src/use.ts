import {
  Computed,
  Destructured,
  Initializer,
  Numeric,
  OnlyType,
  Status,
  WholeObject,
  sameFile,
} from "./status";
import { Reexported } from "./barrel";

export const current = Status.Active;
export const first = Numeric.One;
export const all = Object.values(WholeObject);
const computedKey = "A" as "A" | "B";
export const computed = Computed[computedKey];
const { First } = Destructured;
export const destructured = First;
export const initializer = Initializer.B;
export type OnlyTypeUsage = OnlyType.X;
export const reexported = Reexported.Red;
export { sameFile };

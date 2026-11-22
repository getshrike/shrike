import {
  Body,
  Color,
  Legacy,
  Merged,
  Outer,
  Reexported,
  Spread,
  Typed,
  Whole,
  make,
} from "./legacy";
import { Api, Dotted, Split } from "./more";

import x = Legacy.aliased;

const { destructured } = Legacy;
const escaped = Api.Escaped;

export const values = [
  Legacy.used,
  x,
  destructured,
  Outer.Inner.deep,
  Outer.Whole,
  Merged.helper,
  new Merged(),
  Color.parse("red"),
  Color.Red,
  make.defaults,
  make(),
  Body.entry(),
  Object.keys(Whole),
  { ...Spread },
  Reexported.a,
  Api.Types.Request,
  Api.viaNamespaceImport,
  typeof Api.viaTypeof,
  Split.second,
  Dotted.Inner.used,
];

export const viaNamespaceImport = values.length;
export const escapedValue = escaped;
export type Shape = Typed.Shape;
export type Request = Api.Types.Request;
export type AmbientValue = typeof import("./ambient").Ambient.a;

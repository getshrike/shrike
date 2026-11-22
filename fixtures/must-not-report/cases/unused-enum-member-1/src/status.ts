export enum Status {
  Active = "active",
  Inactive = "inactive",
  Archived = "archived",
}

export enum Numeric {
  One = 1,
  Two = 2,
  Three = 3,
}

export enum WholeObject {
  A,
  B,
  C,
}

export enum Computed {
  A,
  B,
}

export enum Destructured {
  First,
  Second,
}

export enum Initializer {
  A = 1,
  B = A,
  C = 4,
}

export enum OnlyType {
  X,
  Y,
}

export enum SameFile {
  Used,
  Unused,
}

export const sameFile = SameFile.Used;

export enum Reexported {
  Red,
  Green,
}

export enum DeadEnum {
  A,
  B,
}

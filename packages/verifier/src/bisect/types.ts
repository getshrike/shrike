export interface BisectUnit {
  id: string;
  files: string[];
}

export type BisectStatus =
  | "pass"
  | "failed"
  | "interaction-failure"
  | "inconclusive";

export interface BisectResult<T extends BisectUnit> {
  status: BisectStatus;
  rejected: T[];
  passing: T[];
  attempts: number;
  reason: string;
}

export interface BisectOptions {
  maxAttempts?: number;
}

export interface BisectState {
  attempts: number;
  maxAttempts: number;
}

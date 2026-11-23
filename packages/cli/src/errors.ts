/** Exit codes (see docs/development.md). */
export const ExitCode = {
  OK: 0,
  FINDINGS: 1,
  USAGE: 2,
  VERIFICATION_FAILED: 3,
  COULD_NOT_RUN: 4,
  REVERT_FAILED: 5,
} as const;

export type ExitCodeValue = (typeof ExitCode)[keyof typeof ExitCode];

/** A failure with a stable exit code and a machine-readable code. */
export class CliError extends Error {
  readonly exitCode: ExitCodeValue;
  readonly code: string;
  readonly details: string | undefined;

  constructor(
    exitCode: ExitCodeValue,
    code: string,
    message: string,
    details?: string,
  ) {
    super(message);
    this.name = "CliError";
    this.exitCode = exitCode;
    this.code = code;
    this.details = details;
  }
}

export class UsageError extends CliError {
  constructor(message: string) {
    super(ExitCode.USAGE, "usage", message);
    this.name = "UsageError";
  }
}

export class ConfigError extends CliError {
  constructor(message: string, details?: string) {
    super(ExitCode.USAGE, "config", message, details);
    this.name = "ConfigError";
  }
}

export class EngineUnavailableError extends CliError {
  constructor(message: string) {
    super(ExitCode.COULD_NOT_RUN, "engine-unavailable", message);
    this.name = "EngineUnavailableError";
  }
}

export class EngineCommandError extends CliError {
  readonly engineCode: string;

  constructor(engineCode: string, message: string, details?: string) {
    const usage =
      engineCode.startsWith("config") ||
      engineCode.startsWith("invalid") ||
      engineCode === "unsupported_protocol" ||
      engineCode === "malformed_request";
    super(usage ? ExitCode.USAGE : ExitCode.COULD_NOT_RUN, engineCode, message, details);
    this.name = "EngineCommandError";
    this.engineCode = engineCode;
  }
}

export class VerificationFailedError extends CliError {
  constructor(message: string) {
    super(ExitCode.VERIFICATION_FAILED, "verification-failed", message);
    this.name = "VerificationFailedError";
  }
}

export class RevertFailedError extends CliError {
  constructor(message: string, details?: string) {
    super(ExitCode.REVERT_FAILED, "revert-failed", message, details);
    this.name = "RevertFailedError";
  }
}

export function isCliError(error: unknown): error is CliError {
  return error instanceof CliError;
}

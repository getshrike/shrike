/** Public host API. The command-line entry point is `main.ts`. */
export { PROTOCOL_VERSION, TOOL_NAME, VERSION } from "./version.ts";
export {
  DEFAULT_CONFIG,
  engineConfig,
  findConfigPath,
  loadConfig,
  mergeConfig,
  repoRoot,
  type CheckConfig,
  type ShrikeConfig,
} from "./config.ts";
export {
  CliError,
  ConfigError,
  EngineCommandError,
  EngineUnavailableError,
  ExitCode,
  RevertFailedError,
  UsageError,
  VerificationFailedError,
  isCliError,
  type ExitCodeValue,
} from "./errors.ts";

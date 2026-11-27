import { UsageError } from "./errors.ts";

export interface ParsedArgs {
  positionals: string[];
  values: Map<string, string | boolean>;
  /** Every occurrence of a value-taking flag, in argv order. */
  repeated?: Map<string, string[]>;
}

const DEFAULT_BOOLEANS = new Set([
  "json",
  "help",
  "force",
  "pr",
  "in-place",
  "allow-dirty",
  "verify",
  "receipt",
  "quiet",
  "dry-run",
  "claude",
  "cursor",
  "opencode",
  "all",
]);

/**
 * Small, dependency-free argument parser.
 *
 * Unknown flags that are not in `booleanFlags` consume the next token as a
 * value. `--flag=value` is always supported. `--` ends flag parsing.
 */
export function parseArgs(
  argv: string[],
  booleanFlags: Iterable<string> = DEFAULT_BOOLEANS,
): ParsedArgs {
  const booleans = new Set(booleanFlags);
  const values = new Map<string, string | boolean>();
  const repeated = new Map<string, string[]>();
  const positionals: string[] = [];
  let index = 0;

  const record = (key: string, value: string): void => {
    values.set(key, value);
    repeated.set(key, [...(repeated.get(key) ?? []), value]);
  };

  while (index < argv.length) {
    const token = argv[index];
    if (token === undefined) break;
    if (token === "--") {
      positionals.push(...argv.slice(index + 1));
      break;
    }
    if (token === "-h") {
      values.set("help", true);
      index += 1;
      continue;
    }
    if (!token.startsWith("--")) {
      positionals.push(token);
      index += 1;
      continue;
    }

    const withoutPrefix = token.slice(2);
    const equals = withoutPrefix.indexOf("=");
    if (equals >= 0) {
      const key = withoutPrefix.slice(0, equals);
      record(key, withoutPrefix.slice(equals + 1));
      index += 1;
      continue;
    }

    const key = withoutPrefix;
    if (booleans.has(key)) {
      values.set(key, true);
      index += 1;
      continue;
    }

    const next = argv[index + 1];
    if (next !== undefined && !next.startsWith("--")) {
      record(key, next);
      index += 2;
    } else {
      values.set(key, true);
      index += 1;
    }
  }

  return { positionals, values, repeated };
}

export function hasFlag(args: ParsedArgs, name: string): boolean {
  return args.values.get(name) === true || typeof args.values.get(name) === "string";
}

export function stringFlag(args: ParsedArgs, name: string): string | undefined {
  const value = args.values.get(name);
  return typeof value === "string" ? value : undefined;
}

/** Every value supplied for a repeatable flag, falling back to one value. */
export function stringFlags(args: ParsedArgs, name: string): string[] {
  const repeated = args.repeated?.get(name);
  if (repeated && repeated.length > 0) return [...repeated];
  const value = stringFlag(args, name);
  return value === undefined ? [] : [value];
}

export function booleanFlag(args: ParsedArgs, name: string): boolean {
  const value = args.values.get(name);
  return value === true || value === "true" || value === "1";
}

export function requireStringFlag(args: ParsedArgs, name: string): string {
  const value = stringFlag(args, name);
  if (value === undefined || value.length === 0) {
    throw new UsageError(`--${name} requires a value`);
  }
  return value;
}

export function rejectUnknownFlags(
  args: ParsedArgs,
  allowed: Iterable<string>,
): void {
  const allow = new Set(allowed);
  for (const key of args.values.keys()) {
    if (!allow.has(key)) throw new UsageError(`unknown flag: --${key}`);
  }
}

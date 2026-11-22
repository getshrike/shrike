export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isString(value: unknown, minLength = 1): value is string {
  return typeof value === "string" && value.length >= minLength;
}

export function isInteger(value: unknown, minimum = 0): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= minimum;
}

export function isStringArray(value: unknown, minItems = 0): value is string[] {
  return (
    Array.isArray(value) &&
    value.length >= minItems &&
    value.every((item) => typeof item === "string" && item.length > 0)
  );
}

export function unique(values: string[]): boolean {
  return new Set(values).size === values.length;
}

export function exactKeys(
  value: Record<string, unknown>,
  required: string[],
  optional: string[],
  path: string,
  problems: string[],
): void {
  for (const key of required) {
    if (!(key in value)) problems.push(`${path}.${key} is required`);
  }
  const allowed = new Set([...required, ...optional]);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) problems.push(`${path}.${key} is not allowed`);
  }
}

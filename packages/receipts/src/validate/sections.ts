import {
  BASELINE_STATUSES, BATCH_ID, BUILD_STATUSES, CHECK_STATUSES, CLASSIFICATIONS, ENTRY_POLICIES,
  GRAPH_HASH, HASH, SYMBOL,
} from "./constants.ts";
import {
  exactKeys, isInteger, isRecord, isString, isStringArray, unique,
} from "./primitives.ts";

export function validateTool(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("tool must be an object");
    return;
  }
  exactKeys(value, ["name", "version"], [], "tool", problems);
  if (!isString(value.name)) problems.push("tool.name must be a non-empty string");
  if (!isString(value.version)) problems.push("tool.version must be a non-empty string");
}

export function validateBase(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("base must be an object");
    return;
  }
  exactKeys(value, ["ref", "commit", "graphHash"], [], "base", problems);
  if (!isString(value.ref)) problems.push("base.ref must be a non-empty string");
  if (!isString(value.commit)) problems.push("base.commit must be a non-empty string");
  if (typeof value.graphHash !== "string" || !GRAPH_HASH.test(value.graphHash)) {
    problems.push("base.graphHash must be blake3:hex or sha256:hex");
  }
}

export function validateHead(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("head must be an object");
    return;
  }
  exactKeys(value, ["commit", "graphHash"], ["dirty"], "head", problems);
  if (!isString(value.commit)) problems.push("head.commit must be a non-empty string");
  if (typeof value.graphHash !== "string" || !GRAPH_HASH.test(value.graphHash)) {
    problems.push("head.graphHash must be blake3:hex or sha256:hex");
  }
  if ("dirty" in value && typeof value.dirty !== "boolean") {
    problems.push("head.dirty must be a boolean");
  }
}

export function validatePlan(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("plan must be an object");
    return;
  }
  exactKeys(value, ["id", "hash"], [], "plan", problems);
  if (!isString(value.id)) problems.push("plan.id must be a non-empty string");
  if (typeof value.hash !== "string" || !GRAPH_HASH.test(value.hash)) {
    problems.push("plan.hash must be blake3:hex or sha256:hex");
  }
}

export function validateBuild(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("build must be an object");
    return;
  }
  exactKeys(
    value,
    ["source", "snapshotHash", "version", "status"],
    [],
    "build",
    problems,
  );
  if (!isString(value.source)) problems.push("build.source must be a non-empty string");
  if (typeof value.snapshotHash !== "string" || !HASH.test(value.snapshotHash)) {
    problems.push("build.snapshotHash must be sha256:hex");
  }
  if (!isString(value.version)) problems.push("build.version must be a non-empty string");
  if (typeof value.status !== "string" || !BUILD_STATUSES.has(value.status)) {
    problems.push("build.status is not a known status");
  }
}

export function validateBatch(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("batch must be an object");
    return;
  }
  exactKeys(
    value,
    ["id", "symbols", "files", "linesRemoved", "patches"],
    [],
    "batch",
    problems,
  );
  if (typeof value.id !== "string" || !BATCH_ID.test(value.id)) {
    problems.push("batch.id must match batch_[A-Za-z0-9_-]+");
  }
  if (!isStringArray(value.symbols, 1) || !unique(value.symbols)) {
    problems.push("batch.symbols must be a non-empty unique array of strings");
  } else if (!value.symbols.every((symbol) => SYMBOL.test(symbol))) {
    problems.push("batch.symbols entries must look like path#Name");
  }
  if (!isStringArray(value.files, 1) || !unique(value.files)) {
    problems.push("batch.files must be a non-empty unique array of strings");
  } else if (value.files.some((file) => file.includes("#"))) {
    problems.push("batch.files entries must not contain #");
  }
  if (!isInteger(value.linesRemoved)) {
    problems.push("batch.linesRemoved must be a non-negative integer");
  }
  if (!isStringArray(value.patches, 1) || !unique(value.patches)) {
    problems.push("batch.patches must be a non-empty unique array of strings");
  }
}

export function validateChecks(value: unknown, problems: string[]): void {
  if (!Array.isArray(value)) {
    problems.push("checks must be an array");
    return;
  }
  for (const [index, check] of value.entries()) {
    const path = `checks[${index}]`;
    if (!isRecord(check)) {
      problems.push(`${path} must be an object`);
      continue;
    }
    exactKeys(
      check,
      ["name", "command", "status", "baselineStatus", "durationMs", "reruns"],
      ["required", "classification", "failureSignature"],
      path,
      problems,
    );
    if (!isString(check.name)) problems.push(`${path}.name must be a non-empty string`);
    if (!isString(check.command)) problems.push(`${path}.command must be a non-empty string`);
    if (typeof check.status !== "string" || !CHECK_STATUSES.has(check.status)) {
      problems.push(`${path}.status is not a known check status`);
    }
    if (typeof check.baselineStatus !== "string" || !BASELINE_STATUSES.has(check.baselineStatus)) {
      problems.push(`${path}.baselineStatus is not a known baseline status`);
    }
    if (!isInteger(check.durationMs)) {
      problems.push(`${path}.durationMs must be a non-negative integer`);
    }
    if (!isInteger(check.reruns)) {
      problems.push(`${path}.reruns must be a non-negative integer`);
    }
    if ("required" in check && typeof check.required !== "boolean") {
      problems.push(`${path}.required must be a boolean`);
    }
    if (
      "classification" in check &&
      (typeof check.classification !== "string" || !CLASSIFICATIONS.has(check.classification))
    ) {
      problems.push(`${path}.classification is not a known classification`);
    }
    if ("failureSignature" in check && !isString(check.failureSignature)) {
      problems.push(`${path}.failureSignature must be a non-empty string`);
    }
  }
}

export function validateRejected(value: unknown, problems: string[]): void {
  if (!Array.isArray(value)) {
    problems.push("rejected must be an array");
    return;
  }
  for (const [index, rejection] of value.entries()) {
    const path = `rejected[${index}]`;
    if (!isRecord(rejection)) {
      problems.push(`${path} must be an object`);
      continue;
    }
    exactKeys(
      rejection,
      ["symbols", "check", "signature"],
      ["files", "reason"],
      path,
      problems,
    );
    if (!isStringArray(rejection.symbols, 1) || !unique(rejection.symbols)) {
      problems.push(`${path}.symbols must be a non-empty unique array`);
    } else if (!rejection.symbols.every((symbol) => SYMBOL.test(symbol))) {
      problems.push(`${path}.symbols entries must look like path#Name`);
    }
    if ("files" in rejection && (!isStringArray(rejection.files) || !unique(rejection.files))) {
      problems.push(`${path}.files must be a unique array of strings`);
    }
    if (!isString(rejection.check)) problems.push(`${path}.check must be a non-empty string`);
    if (!isString(rejection.signature)) {
      problems.push(`${path}.signature must be a non-empty string`);
    }
    if ("reason" in rejection && typeof rejection.reason !== "string") {
      problems.push(`${path}.reason must be a string`);
    }
  }
}

export function validatePolicy(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("policy must be an object");
    return;
  }
  exactKeys(
    value,
    ["entryPolicy", "maxSymbols", "maxFiles", "allowOversized"],
    ["maxLiveGraphPercent"],
    "policy",
    problems,
  );
  if (typeof value.entryPolicy !== "string" || !ENTRY_POLICIES.has(value.entryPolicy)) {
    problems.push("policy.entryPolicy must be production, local, or ci");
  }
  if (!isInteger(value.maxSymbols, 1)) {
    problems.push("policy.maxSymbols must be a positive integer");
  }
  if (!isInteger(value.maxFiles, 1)) {
    problems.push("policy.maxFiles must be a positive integer");
  }
  if (typeof value.allowOversized !== "boolean") {
    problems.push("policy.allowOversized must be a boolean");
  }
  if (
    "maxLiveGraphPercent" in value &&
    (typeof value.maxLiveGraphPercent !== "number" ||
      value.maxLiveGraphPercent < 0 ||
      value.maxLiveGraphPercent > 100)
  ) {
    problems.push("policy.maxLiveGraphPercent must be between 0 and 100");
  }
}

export function validateEnvironment(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("environment must be an object");
    return;
  }
  exactKeys(
    value,
    ["node", "packageManager", "platform"],
    ["os", "arch", "shrike"],
    "environment",
    problems,
  );
  for (const key of ["node", "packageManager", "platform"] as const) {
    if (!isString(value[key])) problems.push(`environment.${key} must be a non-empty string`);
  }
  for (const key of ["os", "arch", "shrike"] as const) {
    if (key in value && typeof value[key] !== "string") {
      problems.push(`environment.${key} must be a string`);
    }
  }
}

export function validateRecovery(value: unknown, problems: string[]): void {
  if (!isRecord(value)) {
    problems.push("recovery must be an object");
    return;
  }
  exactKeys(value, ["snapshotPath", "restoreCommand"], [], "recovery", problems);
  if (!isString(value.snapshotPath)) {
    problems.push("recovery.snapshotPath must be a non-empty string");
  }
  if (!isString(value.restoreCommand)) {
    problems.push("recovery.restoreCommand must be a non-empty string");
  }
}

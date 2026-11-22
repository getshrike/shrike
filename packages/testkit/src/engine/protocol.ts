import { EngineProtocolError, PROTOCOL_VERSION, type ProtocolResponse } from "./types.ts";

export function parseResponseLine(line: string): unknown {
  try {
    return JSON.parse(line);
  } catch {
    throw new EngineProtocolError(`engine returned malformed JSON: ${line.slice(0, 200)}`);
  }
}

/** Validate the response envelope and return a typed response. */
export function assertProtocolResponse(value: unknown, expectedId?: string): ProtocolResponse {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new EngineProtocolError("engine response is not an object");
  }
  const record = value as Record<string, unknown>;
  if (record.protocolVersion !== PROTOCOL_VERSION) {
    throw new EngineProtocolError(
      `unsupported protocolVersion: ${String(record.protocolVersion)}`,
    );
  }
  if (typeof record.id !== "string") {
    throw new EngineProtocolError("engine response is missing a string id");
  }
  if (expectedId !== undefined && record.id !== expectedId) {
    throw new EngineProtocolError(
      `engine response id mismatch: expected ${expectedId}, received ${record.id}`,
    );
  }
  if (record.ok === true) {
    if (!("result" in record) || record.result === undefined || record.result === null) {
      throw new EngineProtocolError("ok response is missing a result");
    }
    return {
      protocolVersion: PROTOCOL_VERSION,
      id: record.id,
      ok: true,
      result: record.result,
    };
  }
  if (record.ok === false) {
    const error = record.error;
    if (typeof error !== "object" || error === null) {
      throw new EngineProtocolError("error response is missing an error object");
    }
    const errorRecord = error as Record<string, unknown>;
    if (typeof errorRecord.code !== "string" || typeof errorRecord.message !== "string") {
      throw new EngineProtocolError("error response has an invalid code or message");
    }
    return {
      protocolVersion: PROTOCOL_VERSION,
      id: record.id,
      ok: false,
      error: {
        code: errorRecord.code,
        message: errorRecord.message,
        ...(errorRecord.details !== undefined ? { details: errorRecord.details } : {}),
      },
    };
  }
  throw new EngineProtocolError("engine response has a non-boolean ok field");
}

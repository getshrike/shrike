/** Protocol version locked by the host/engine contract. */
export const PROTOCOL_VERSION = "1.0";

export interface ProtocolRequest {
  protocolVersion: "1.0";
  id: string;
  command: string;
  payload: unknown;
}

export interface ProtocolErrorPayload {
  code: string;
  message: string;
  details?: unknown;
}

export type ProtocolResponse<T = unknown> =
  | {
      protocolVersion: "1.0";
      id: string;
      ok: true;
      result: T;
    }
  | {
      protocolVersion: "1.0";
      id: string;
      ok: false;
      error: ProtocolErrorPayload;
    };

export class EngineProtocolError extends Error {
  readonly details: string | undefined;

  constructor(message: string, details?: string) {
    super(message);
    this.name = "EngineProtocolError";
    this.details = details;
  }
}

export class EngineNotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "EngineNotFoundError";
  }
}

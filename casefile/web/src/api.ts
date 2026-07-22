import {
  type Decoder,
  decodeApplyResponse,
  decodeBoards,
  decodeCurrent,
  decodeDiagnostics,
  decodeHostFailure,
  decodePreview,
  decodeRecords,
  decodeRelationships,
} from "./api-contract";
import {
  type ApplyResponse,
  type Board,
  type ChangeRequest,
  type Diagnostic,
  type Identity,
  type Preview,
  type Record,
  type Relationship,
  type Scope,
} from "./model";

export type ApiResult<T> =
  | Readonly<{ tag: "success"; value: T }>
  | Readonly<{
      tag: "failure";
      message: string;
      code: "stale_revision" | undefined;
    }>;
type Endpoint = "/api/query" | "/api/preview" | "/api/apply";

const json = async (response: Response): Promise<unknown> => await response.json();

const post = async <T>(
  endpoint: Endpoint,
  body: unknown,
  signal: AbortSignal,
  capability: string | undefined,
  decode: Decoder<T>,
): Promise<ApiResult<T>> => {
  try {
    const headers = new Headers({ "Content-Type": "application/json" });
    if (capability !== undefined) headers.set("X-Casefile-Write-Capability", capability);
    const response = await fetch(endpoint, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      signal,
    });
    const payload = await json(response).catch(() => undefined);
    if (!response.ok) {
      const failure = decodeHostFailure(payload, response.status);
      return { tag: "failure", message: failure.message, code: failure.code };
    }
    return { tag: "success", value: decode(payload) };
  } catch (error: unknown) {
    if (error instanceof Error && error.name === "AbortError")
      return { tag: "failure", message: "Request cancelled.", code: undefined };
    return {
      tag: "failure",
      message: error instanceof Error ? error.message : "The host request failed.",
      code: undefined,
    };
  }
};

const query = <T>(body: unknown, signal: AbortSignal, decode: Decoder<T>): Promise<ApiResult<T>> =>
  post("/api/query", body, signal, undefined, (value) => decodeCurrent(value, decode));

export const fetchRecords = (
  search: string | undefined,
  signal: AbortSignal,
): Promise<ApiResult<ReadonlyArray<Record>>> =>
  query({ query: "records", search }, signal, decodeRecords);
export const fetchDiagnostics = (
  signal: AbortSignal,
): Promise<ApiResult<ReadonlyArray<Diagnostic>>> =>
  query({ query: "diagnostics" }, signal, decodeDiagnostics);
export const fetchBoards = (
  scope: Scope,
  signal: AbortSignal,
): Promise<ApiResult<ReadonlyArray<Board>>> =>
  query({ query: "boards", scope }, signal, decodeBoards);
export const fetchRelationships = (
  identity: Identity,
  signal: AbortSignal,
): Promise<ApiResult<ReadonlyArray<Relationship>>> =>
  query({ query: "relationships", identity }, signal, decodeRelationships);
export const preview = (change: ChangeRequest, signal: AbortSignal): Promise<ApiResult<Preview>> =>
  post("/api/preview", change, signal, undefined, decodePreview);
export const apply = (
  value: Preview,
  capability: string,
  signal: AbortSignal,
): Promise<ApiResult<ApplyResponse>> =>
  post("/api/apply", value, signal, capability, decodeApplyResponse);

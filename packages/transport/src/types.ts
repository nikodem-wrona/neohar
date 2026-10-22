// The transport the UI talks to the core through. The UI knows these primitives and
// nothing about Tauri or sockets. The method, projection and stream names and their types come
// from `nh-proto` (`generated.ts`).

import type { RpcError } from './errors';
import type {
  Method,
  Params,
  PatchOp,
  ProjectionData,
  ProjectionName,
  Result,
  StreamParamsMap,
} from './generated';

/** The kinds of stream. */
export type StreamKind = keyof StreamParamsMap;

/** What a subscription calls when the core sends something. */
export interface ProjectionHandlers {
  /** The whole state of the projection. It comes first, and again after a `resync`. */
  onSnapshot(data: ProjectionData, version: number): void;
  /** The changes after the last message. `version` is one above the version before. */
  onPatch(ops: PatchOp[], version: number): void;
  /**
   * The core no longer has the patches that this client needs. The transport subscribes again by
   * itself, and a new snapshot follows. The UI can show that it is waiting.
   */
  onResync?(): void;
}

/** A subscription to a projection. */
export interface Subscription {
  readonly name: ProjectionName;
  close(): void;
}

/** A binary stream from the core. */
export interface BinaryStream {
  close(): void;
}

export interface Transport {
  /** `call(method, params)`: a method of the API. Rejects with an `RpcError`. */
  call<M extends Method>(method: M, params: Params<M>): Promise<Result<M>>;
  /**
   * `subscribe(projection)`: a snapshot, then patches, until `close()`. The transport follows the
   * versions. A patch that does not follow the last one, and a `resync`, make it subscribe again.
   */
  subscribe(name: ProjectionName, handlers: ProjectionHandlers): Subscription;
  /** `stream(kind, params)`: binary frames from the core, in order, until `close()`. */
  stream<K extends StreamKind>(
    kind: K,
    params: StreamParamsMap[K],
    onFrame: (frame: ArrayBuffer) => void,
  ): BinaryStream;
  /** The address of a blob of the store, for an `<img>`. */
  blobUrl(hash: string): string;
  /**
   * The address of the stylesheet of the fonts that the person installed, for a CSS family list:
   * `nh://font/…` in the app, and `/font/…` of the listener on the phone.
   */
  fontsUrl(families: string): string;
  /**
   * Calls `listener` with each failure that no caller of the transport gets: a subscription or a
   * stream that the core refused. Returns the function that ends it. A transport that has no such
   * failure does not have it.
   */
  onFailure?(listener: (failure: RpcError) => void): () => void;
}

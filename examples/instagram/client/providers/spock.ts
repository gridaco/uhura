// Instagram's app-local adapter provider. Uhura owns the deterministic
// machine; this module observes Spock authority and performs requested
// mutations at the two explicitly admitted application ports.

const PAGE_SIZE = 4;
const SUPPORTED_IMAGE_TYPES = new Set([
  "image/jpeg",
  "image/png",
  "image/webp",
]);

// Evidence fixtures use stable logical names while Play serves exact captured
// files. Live Spock storage ids deliberately fall through to signed URLs.
const LOCAL_PLAY_ASSETS: Readonly<Record<string, string>> = {
  "avatar-mira": "avatar-mira.webp",
  "avatar-lena": "avatar-lena.webp",
  "avatar-marco": "avatar-marco.webp",
  "avatar-nils": "avatar-nils.webp",
  "avatar-priya": "avatar-priya.webp",
  "avatar-ayla": "avatar-ayla.webp",
  "avatar-june": "avatar-june.webp",
  "avatar-theo": "avatar-theo.webp",
  "avatar-kenji": "avatar-kenji.webp",
  "media-lena-glaze": "media-lena-glaze.webp",
  "media-marco-baja-1": "media-marco-baja-1.webp",
  "media-marco-baja-2": "media-marco-baja-2.webp",
  "media-marco-baja-3": "media-marco-baja-3.webp",
  "media-nils-aurora-poster": "media-nils-aurora-poster.webp",
  "media-priya-starter": "media-priya-starter.webp",
  "media-ayla-ferry": "media-ayla-ferry.webp",
  "media-june-lookbook": "media-june-lookbook.webp",
  "media-theo-court": "media-theo-court.webp",
  "media-kenji-copper": "media-kenji-copper.webp",
  "thumb-lena-1": "thumb-lena-1.webp",
  "thumb-lena-2": "thumb-lena-2.webp",
  "thumb-lena-3": "thumb-lena-3.webp",
  "thumb-lena-4": "thumb-lena-4.webp",
  "thumb-lena-5": "thumb-lena-5.webp",
  "thumb-lena-6": "thumb-lena-6.webp",
  "thumb-lena-7": "thumb-lena-7.webp",
  "thumb-lena-8": "thumb-lena-8.webp",
  "thumb-lena-9": "thumb-lena-9.webp",
  "thumb-mira-1": "thumb-mira-1.webp",
  "thumb-mira-2": "thumb-mira-2.webp",
  "thumb-mira-3": "thumb-mira-3.webp",
  "thumb-mira-4": "thumb-mira-4.webp",
  "thumb-mira-5": "thumb-mira-5.webp",
  "thumb-mira-6": "thumb-mira-6.webp",
  "video-nils-aurora": "media-nils-aurora.mp4",
  "video-theo-court": "media-theo-court.mp4",
  "video-mira-ferry": "media-mira-ferry.mp4",
};

// The current Spock authority caps one collection read at 200 rows. This demo fits inside
// that ceiling per table; snapshot-consistent pagination is deferred dogfood
// rather than pretending a clamped response is complete.
const SNAPSHOT_QUERY = `
  query UhuraSnapshot {
    users: user(limit: 200) {
      id
      username
      display_name
      avatar { id }
      avatar_alt
      bio
    }
    stories: story(limit: 200) {
      id
      author { id }
      position
      media_file { id }
      media_alt
      caption
      published_at
    }
    storyViews: story_view(limit: 200) {
      viewer { id }
      story { id }
      at
    }
    posts: post(limit: 200) {
      id
      author { id }
      caption
      published_at
      show_in_feed
      media_kind
      media_file { id }
      video_file { id }
      media_alt
    }
    slides: carousel_slide(limit: 200) {
      id
      post { id }
      position
      file { id }
      alt
    }
    comments: comment(limit: 200) {
      id
      post { id }
      author { id }
      body
      created_at
    }
    likes: like(limit: 200) {
      user { id }
      post { id }
      at
    }
    saves: save(limit: 200) {
      user { id }
      post { id }
      at
    }
    follows: follow(limit: 200) {
      follower { id }
      followed { id }
      at
    }
    postTags: post_tag(limit: 200) {
      post { id }
      person { id }
    }
  }
`;

const COMMAND_REFUSALS: Readonly<Record<string, readonly string[]>> = {
  "feed/like-post": ["not-authorized", "not-found"],
  "feed/unlike-post": ["not-authorized"],
  "feed/save-post": ["not-authorized", "not-found"],
  "feed/unsave-post": ["not-authorized"],
  "feed/mark-story-seen": ["not-authorized", "not-found"],
  "comments/add-comment": ["not-authorized", "comment-body-invalid", "not-found"],
  "profile/follow-user": [
    "not-authorized",
    "not-found",
    "cannot-follow-self",
  ],
  "profile/unfollow-user": ["not-authorized"],
  "create/publish-image": [
    "not-authorized",
    "image-not-ready",
    "unsupported-media-type",
  ],
};

export interface SpockProviderConfig {
  /** Standalone fallback for the full Spock `/graphql/v1` endpoint. */
  graphql_url: string;
  /** Standalone fallback for the Spock `/rest/v1/rpc` prefix. */
  rpc_url: string;
  /** Standalone fallback for the Spock `/storage/v1` prefix. */
  storage_url: string;
  /** Seeded user UUID or unique username. */
  actor: string;
}

export interface ProviderHost {
  /** Aborts when the Play route that owns this provider is retired. */
  readonly signal: AbortSignal;
  /**
   * Browser capability supplied by the play shell. The selected File remains
   * entirely outside Uhura Core and its wire envelopes.
   */
  pickFile(options: { accept: string }): Promise<File | null>;
}

interface PortRequirement {
  readonly port: string;
  readonly adapter: "app.provider";
  readonly contractHash: string;
  readonly contractInstanceHash: string;
}

interface PortAdapterContext {
  readonly signal: AbortSignal;
  deliver(value: WireValue): void;
}

interface AdapterProviderHost extends ProviderHost {
  port(name: string): PortRequirement;
}

interface PortAdapter extends PortRequirement {
  start?(context: PortAdapterContext): void | Promise<void>;
  accept(command: WireValue, context: PortAdapterContext): void | Promise<void>;
  dispose?(): void;
}

interface WireValue {
  readonly $: string;
  readonly [field: string]: unknown;
}

export interface RemoteSystemInfo {
  actor: string | null;
  actors: Array<{ id: string; username: string; label: string }>;
}

interface SpockBackend {
  dispose(): void;
  load(): Promise<void>;
  execute(operation: BackendOperation): Promise<BackendSettlement>;
  authorityValue(): WireValue;
  resolveAsset(asset: string): Promise<string>;
  systemInfo(): RemoteSystemInfo;
}

const INSTAGRAM_MODULE = "app.instagram@1";
const INSTAGRAM_MACHINE = `${INSTAGRAM_MODULE}::Instagram`;
const USER_ID_TYPE = `${INSTAGRAM_MODULE}::UserId`;
const POST_ID_TYPE = `${INSTAGRAM_MODULE}::PostId`;
const STORY_ID_TYPE = `${INSTAGRAM_MODULE}::StoryId`;
const REQUEST_ID_TYPE = `${INSTAGRAM_MODULE}::RequestId`;
const AUTHORITY_TYPE = `${INSTAGRAM_MODULE}::Authority`;
const MEDIA_TYPE = `${INSTAGRAM_MODULE}::Media`;
const MUTATION_TYPE = `${INSTAGRAM_MODULE}::Mutation`;
const SETTLEMENT_TYPE = `${INSTAGRAM_MODULE}::Settlement`;
const AUTHORITY_RECEIVE_TYPE =
  `${INSTAGRAM_MACHINE}::port.authority.Receive`;
const MUTATIONS_SEND_TYPE =
  `${INSTAGRAM_MACHINE}::port.mutations.Send`;
const MUTATIONS_RECEIVE_TYPE =
  `${INSTAGRAM_MACHINE}::port.mutations.Receive`;

const wireText = (value: string): WireValue => ({ $: "Text", value });
const wireBool = (value: boolean): WireValue => ({ $: "bool", value });
const wireNat = (value: number): WireValue => ({
  $: "Nat",
  value: String(value),
});
const wireKey = (
  type: string,
  value: WireValue,
): WireValue => ({ $: "key", type, value });
const wireRecord = (
  fields: ReadonlyArray<readonly [string, WireValue]>,
): WireValue => ({
  $: "record",
  fields: fields.map(([name, value]) => ({ name, value })),
});
const wireVariant = (
  type: string,
  caseName: string,
  fields: ReadonlyArray<readonly [string | null, WireValue]> = [],
): WireValue => ({
  $: "variant",
  type,
  case: caseName,
  fields: fields.map(([name, value]) => ({ name, value })),
});
const wireSeq = (items: readonly WireValue[]): WireValue => ({
  $: "seq",
  items,
});
const wireMap = (
  entries: ReadonlyArray<readonly [WireValue, WireValue]>,
): WireValue => ({ $: "map", entries });
const textEncoder = new TextEncoder();
const lengthPrefix = (value: number): number[] => {
  const bytes: number[] = [];
  do {
    let byte = value % 128;
    value = Math.floor(value / 128);
    if (value !== 0) byte += 128;
    bytes.push(byte);
  } while (value !== 0);
  return bytes;
};
const canonicalTextKeyBytes = (value: string): Uint8Array => {
  const body = textEncoder.encode(value);
  return Uint8Array.from([...lengthPrefix(body.length), ...body]);
};
const compareBytes = (left: Uint8Array, right: Uint8Array): number => {
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const order = (left[index] ?? 0) - (right[index] ?? 0);
    if (order !== 0) return order;
  }
  return left.length - right.length;
};
/**
 * Every map currently emitted by this adapter has a nominal Text-backed key.
 * Uhura orders a map by the complete canonical key bytes. The nominal type is
 * constant within one map, so its shared prefix cancels and the order reduces
 * exactly to the length-framed UTF-8 Text body below.
 */
const wireTextKeyMap = (
  type: string,
  entries: ReadonlyArray<readonly [string, WireValue]>,
): WireValue => {
  const ordered = entries
    .map(([key, value]) => ({
      key,
      value,
      canonical: canonicalTextKeyBytes(key),
    }))
    .sort((left, right) => compareBytes(left.canonical, right.canonical));
  for (let index = 1; index < ordered.length; index += 1) {
    if (
      compareBytes(
        ordered[index - 1]?.canonical ?? new Uint8Array(),
        ordered[index]?.canonical ?? new Uint8Array(),
      ) === 0
    ) {
      throw new Error(`duplicate canonical map key \`${ordered[index]?.key}\``);
    }
  }
  return wireMap(ordered.map(({ key, value }) => [
    wireKey(type, wireText(key)),
    value,
  ]));
};
const wireOption = (
  type: string,
  value: WireValue | null,
): WireValue => wireVariant(
  `Option<${type}>`,
  value === null ? "none" : "some",
  value === null ? [] : [["value", value]],
);

/**
 * A picker result is observed immediately so a rejection cannot become
 * unhandled while an earlier provider command finishes.
 */
type PickedFile = Promise<{ file: File | null } | { error: unknown }>;

// A retired backend instance can have a mutation already accepted by Spock.
// A replacement waits for that work before reading its authority snapshot, so a route
// remount cannot strand a just-accepted mutation behind stale boot data.
let authorityTail: Promise<void> = Promise.resolve();

const AUTHORITY_OPERATIONS = new Set<BackendOperation["kind"]>([
  "set_like",
  "set_save",
  "add_comment",
  "mark_story",
  "set_follow",
  "publish_image_request",
]);

const AUTHORITY_REQUEST_TIMEOUT_MS = 15_000;
const HOST_ENVIRONMENT_TIMEOUT_MS = 2_000;
const HOST_ENVIRONMENT_PATH = "/~project/environment";
const HOST_ENVIRONMENT_PROTOCOL = "spock-host-environment/1";

interface AuthorityEndpoints {
  graphqlUrl: string | null;
  rpcUrl: string;
  storageUrl: string;
  whoamiUrl: string;
}

function exactObject(
  value: unknown,
  keys: readonly string[],
): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const actual = Object.keys(value);
  return (
    actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}

function authorityPath(value: unknown): string | null {
  if (
    typeof value !== "string" ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value === "/" ||
    value.endsWith("/")
  ) {
    return null;
  }
  try {
    const parsed = new URL(value, "https://spock.invalid/");
    if (
      parsed.origin !== "https://spock.invalid" ||
      parsed.pathname !== value ||
      parsed.search.length > 0 ||
      parsed.hash.length > 0
    ) {
      return null;
    }
  } catch {
    return null;
  }
  return value;
}

function integratedAuthority(value: unknown): AuthorityEndpoints | null {
  if (
    !exactObject(value, [
      "protocol",
      "mode",
      "project_generation_id",
      "backend_generation_id",
      "authority",
    ]) ||
    value.protocol !== HOST_ENVIRONMENT_PROTOCOL ||
    (value.mode !== "start" && value.mode !== "dev") ||
    !Number.isSafeInteger(value.project_generation_id) ||
    (value.project_generation_id as number) < 1 ||
    !Number.isSafeInteger(value.backend_generation_id) ||
    (value.backend_generation_id as number) < 1 ||
    !exactObject(value.authority, [
      "graphql_path",
      "rpc_path",
      "storage_path",
    ])
  ) {
    return null;
  }

  const graphqlPath = value.authority.graphql_path;
  // `null` is an explicit capability absence in the integrated environment,
  // not invalid metadata and never a reason to contact the standalone host.
  const graphqlUrl = graphqlPath === null ? null : authorityPath(graphqlPath);
  const rpcUrl = authorityPath(value.authority.rpc_path);
  const storageUrl = authorityPath(value.authority.storage_path);
  if (
    (graphqlPath !== null && graphqlUrl === null) ||
    rpcUrl === null ||
    storageUrl === null
  ) {
    return null;
  }

  return {
    graphqlUrl,
    rpcUrl,
    storageUrl,
    whoamiUrl: "/~whoami",
  };
}

function resolveFromEndpoint(reference: string, endpoint: string): string {
  try {
    return new URL(reference, endpoint).toString();
  } catch {
    const sameOrigin = "https://spock.invalid";
    const resolved = new URL(reference, `${sameOrigin}${endpoint}`);
    return resolved.origin === sameOrigin
      ? `${resolved.pathname}${resolved.search}${resolved.hash}`
      : resolved.toString();
  }
}

function enqueueAuthorityWork<T>(work: () => Promise<T>): Promise<T> {
  const queued = authorityTail.then(work, work);
  authorityTail = queued.then(
    () => {},
    () => {},
  );
  return queued;
}

type GraphRef = { id: string };

interface UserRow {
  id: string;
  username: string;
  display_name: string;
  avatar: GraphRef;
  avatar_alt: string;
  bio: string | null;
}

interface GraphStory {
  id: string;
  author: GraphRef;
  position: number;
  media_file: GraphRef;
  media_alt: string;
  caption: string | null;
  published_at: string;
}

interface StoryRow {
  id: string;
  author: string;
  position: number;
  media_file: string;
  media_alt: string;
  caption: string | null;
  published_at: string;
}

interface GraphStoryView {
  viewer: GraphRef;
  story: GraphRef;
  at: string;
}

type MediaKind = "image" | "carousel" | "video";

interface GraphPost {
  id: string;
  author: GraphRef;
  caption: string;
  published_at: string;
  show_in_feed: boolean;
  media_kind: MediaKind;
  media_file: GraphRef | null;
  video_file: GraphRef | null;
  media_alt: string | null;
}

interface PostRow {
  id: string;
  author: string;
  caption: string;
  published_at: string;
  show_in_feed: boolean;
  media_kind: MediaKind;
  media_file: string | null;
  video_file: string | null;
  media_alt: string | null;
}

interface GraphSlide {
  id: string;
  post: GraphRef;
  position: number;
  file: GraphRef;
  alt: string;
}

interface SlideRow {
  id: string;
  post: string;
  position: number;
  file: string;
  alt: string;
}

interface GraphComment {
  id: string;
  post: GraphRef;
  author: GraphRef;
  body: string;
  created_at: string;
}

interface CommentRow {
  id: string;
  post: string;
  author: string;
  body: string;
  created_at: string;
}

interface GraphLike {
  user: GraphRef;
  post: GraphRef;
  at: string;
}

interface GraphSave extends GraphLike {}

interface GraphFollow {
  follower: GraphRef;
  followed: GraphRef;
  at: string;
}

interface GraphPostTag {
  post: GraphRef;
  person: GraphRef;
}

interface SnapshotData {
  users: UserRow[];
  stories: GraphStory[];
  storyViews: GraphStoryView[];
  posts: GraphPost[];
  slides: GraphSlide[];
  comments: GraphComment[];
  likes: GraphLike[];
  saves: GraphSave[];
  follows: GraphFollow[];
  postTags: GraphPostTag[];
}

interface GraphQlError {
  message: string;
  extensions?: Record<string, unknown>;
}

interface GraphQlEnvelope {
  data?: SnapshotData;
  errors?: GraphQlError[];
}

interface WhoAmI {
  actor: unknown;
  anonymous: boolean;
  known: boolean;
}

interface SpockError {
  code?: string;
  kind?: string;
  table?: string | null;
  fields?: string[];
  message?: string;
}

type RpcReply =
  | { ok: true; result: unknown }
  | { ok: false; error: SpockError };

type BackendOperation =
  | { kind: "set_like"; post: string; liked: boolean }
  | { kind: "set_save"; post: string; saved: boolean }
  | { kind: "load_more" }
  | { kind: "reload_feed" }
  | { kind: "set_follow"; user: string; following: boolean }
  | { kind: "add_comment"; post: string; body: string }
  | { kind: "search_people"; query: string }
  | { kind: "choose_image_request" }
  | {
      kind: "publish_image_request";
      object: string;
      caption: string;
      alt: string;
    }
  | { kind: "mark_story"; story: string };

type BackendSettlement =
  | { kind: "accepted" }
  | { kind: "refused"; reason: string }
  | {
      kind: "image_ready";
      object: string;
      preview: string;
      name: string;
    };

interface Database {
  users: Map<string, UserRow>;
  stories: StoryRow[];
  storyViews: Set<string>;
  posts: PostRow[];
  slidesByPost: Map<string, SlideRow[]>;
  commentsByPost: Map<string, CommentRow[]>;
  likeCounts: Map<string, number>;
  liked: Set<string>;
  saved: Set<string>;
  follows: Set<string>;
  followersByUser: Map<string, string[]>;
  followingByUser: Map<string, string[]>;
  taggedPostsByUser: Map<string, string[]>;
}

/**
 * Encode a wire value, rejecting the one JavaScript value JSON cannot encode.
 * @param {unknown} value
 * @returns {string}
 */
function encode(value: unknown): string {
  const encoded = JSON.stringify(value);
  if (encoded === undefined) throw new Error("provider tried to encode `undefined`");
  return encoded;
}

/**
 * @param {unknown} error
 * @returns {string}
 */
function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * @template T
 * @param {T[]} rows
 * @param {(row: T) => string} keyOf
 * @returns {Map<string, T[]>}
 */
function groupBy<T>(rows: T[], keyOf: (row: T) => string): Map<string, T[]> {
  const grouped = new Map<string, T[]>();
  for (const row of rows) {
    const key = keyOf(row);
    const bucket = grouped.get(key) ?? [];
    bucket.push(row);
    grouped.set(key, bucket);
  }
  return grouped;
}

/**
 * Format an authority timestamp for the small social-demo UI. No age label is
 * stored in Spock: it is recomputed whenever a fresh snapshot is projected.
 * @param {string} timestamp
 * @returns {string}
 */
function ageLabel(timestamp: string): string {
  const instant = Date.parse(timestamp);
  if (!Number.isFinite(instant)) {
    throw new Error(`invalid authority timestamp \`${timestamp}\``);
  }
  const elapsed = Math.max(0, Date.now() - instant);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (elapsed < minute) return "now";
  if (elapsed < hour) return `${Math.floor(elapsed / minute)}m`;
  if (elapsed < day) return `${Math.floor(elapsed / hour)}h`;
  if (elapsed < 7 * day) return `${Math.floor(elapsed / day)}d`;
  return new Intl.DateTimeFormat("en", {
    month: "short",
    day: "numeric",
  }).format(new Date(instant));
}

/**
 * @param {string} left
 * @param {string} right
 * @returns {number}
 */
function newestFirst(left: string, right: string): number {
  return Date.parse(right) - Date.parse(left);
}

/**
 * @param {string} left
 * @param {string} right
 * @returns {number}
 */
function oldestFirst(left: string, right: string): number {
  return Date.parse(left) - Date.parse(right);
}

/**
 * @param {string} left
 * @param {string} right
 * @returns {string}
 */
function edgeKey(left: string, right: string): string {
  return `${left}|${right}`;
}

/**
 * Spock errors use snake_case; Uhura refusal names use kebab-case.
 * @param {string} code
 * @returns {string}
 */
function toRefusalName(code: string): string {
  return code.replaceAll("_", "-");
}

/**
 * Create the app-local Spock authority bridge used by the admitted Uhura
 * ports. It exposes domain operations and typed authority values directly;
 * there is no second provider protocol or projection/outcome envelope.
 */
function createSpockBackend(
  { graphql_url, rpc_url, storage_url, actor }: SpockProviderConfig,
  host: ProviderHost,
): SpockBackend {
  const graphqlUrl = graphql_url.replace(/\/+$/, "");
  const rpcUrl = rpc_url.replace(/\/+$/, "");
  const storageUrl = storage_url.replace(/\/+$/, "");
  if (graphqlUrl.length === 0) {
    throw new Error("Spock provider needs `graphql_url`");
  }
  if (rpcUrl.length === 0) throw new Error("Spock provider needs `rpc_url`");
  if (storageUrl.length === 0) {
    throw new Error("Spock provider needs `storage_url`");
  }
  const configuredAuthority: AuthorityEndpoints = {
    graphqlUrl,
    rpcUrl,
    storageUrl,
    whoamiUrl: new URL("/~whoami", graphqlUrl).toString(),
  };

  const signedAssets = new Map<string, { url: string; refreshAt: number }>();
  const signingAssets = new Map<string, Promise<string>>();
  const uploadedFileNames = new Map<string, string>();
  let operationTail: Promise<void> = Promise.resolve();
  const cancellable = new AbortController();
  let disposed = host.signal.aborted;
  let authorityResolution: Promise<AuthorityEndpoints> | undefined;

  function dispose(): void {
    if (disposed) return;
    disposed = true;
    host.signal.removeEventListener("abort", dispose);
    cancellable.abort();
    signedAssets.clear();
    signingAssets.clear();
    uploadedFileNames.clear();
  }

  if (disposed) cancellable.abort();
  else host.signal.addEventListener("abort", dispose, { once: true });

  function assertLive(): void {
    if (!disposed) return;
    throw new DOMException("Uhura Play provider was disposed", "AbortError");
  }

  function authorityEndpoints(): Promise<AuthorityEndpoints> {
    authorityResolution ??= (async () => {
      // Discovery is opportunistic for standalone Uhura sessions. Bound it so
      // a same-origin route that accepts but never answers cannot stall boot.
      const discovery = new AbortController();
      const abortDiscovery = (): void => discovery.abort();
      if (cancellable.signal.aborted) abortDiscovery();
      else {
        cancellable.signal.addEventListener("abort", abortDiscovery, {
          once: true,
        });
      }
      const timeout = setTimeout(abortDiscovery, HOST_ENVIRONMENT_TIMEOUT_MS);
      try {
        const response = await fetch(HOST_ENVIRONMENT_PATH, {
          method: "GET",
          headers: { accept: "application/json" },
          signal: discovery.signal,
        });
        assertLive();
        if (!response.ok) return configuredAuthority;
        const body = await response.text();
        assertLive();
        const environment = integratedAuthority(JSON.parse(body));
        return environment ?? configuredAuthority;
      } catch (error) {
        if (disposed || cancellable.signal.aborted) throw error;
        return configuredAuthority;
      } finally {
        clearTimeout(timeout);
        cancellable.signal.removeEventListener("abort", abortDiscovery);
      }
    })();
    return authorityResolution;
  }

  /**
   * @returns {Promise<SnapshotData>}
   */
  async function fetchSnapshot(): Promise<SnapshotData> {
    const { graphqlUrl } = await authorityEndpoints();
    if (graphqlUrl === null) {
      throw new Error(
        "integrated Spock host does not advertise a GraphQL capability",
      );
    }
    const response = await fetch(graphqlUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: encode({ query: SNAPSHOT_QUERY }),
      signal: cancellable.signal,
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`POST /graphql/v1: ${response.status} ${body}`);
    }

    const envelope = JSON.parse(body) as GraphQlEnvelope;
    if (envelope.errors && envelope.errors.length > 0) {
      const details = envelope.errors.map((error) => error.message).join("; ");
      throw new Error(`GraphQL snapshot failed: ${details}`);
    }
    if (!envelope.data) throw new Error("GraphQL snapshot returned no data");
    return envelope.data;
  }

  let viewerRow: UserRow | null = null;

  /**
   * The RPC identity is always the resolved auth-table UUID, even when the
   * provider was configured with a username.
   * @returns {string}
   */
  function viewerId(): string {
    if (!viewerRow) throw new Error("Spock provider has not assembled boot");
    return viewerRow.id;
  }

  /**
   * Prove that Spock resolves the normalized UUID to the same auth-table row.
   * This catches a wrong server, stale seed, or malformed actor before the
   * session accepts its boot projection.
   * @returns {Promise<void>}
   */
  async function verifyViewer(): Promise<void> {
    const expected = viewerId();
    const { whoamiUrl } = await authorityEndpoints();
    const response = await fetch(whoamiUrl, {
      headers: { "x-spock-actor": expected },
      signal: cancellable.signal,
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`GET /~whoami: ${response.status} ${body}`);
    }
    const identity = JSON.parse(body) as WhoAmI;
    if (identity.anonymous || !identity.known || identity.actor !== expected) {
      throw new Error(
        `Spock did not recognize resolved actor \`${expected}\`: ${body}`,
      );
    }
  }

  /**
   * @param {string} fn
   * @param {Record<string, unknown>} payload
   * @returns {Promise<RpcReply>}
   */
  async function rpc(
    fn: string,
    payload: Record<string, unknown>,
  ): Promise<RpcReply> {
    const { rpcUrl } = await authorityEndpoints();
    const timeout = new AbortController();
    const timeoutId = setTimeout(
      () => timeout.abort(),
      AUTHORITY_REQUEST_TIMEOUT_MS,
    );
    let response: Response;
    try {
      // Once sent, a domain mutation may already be accepted by Spock. Do not
      // abort it merely because its route retired; the module-level authority
      // barrier makes the replacement backend wait for settlement. The finite timeout
      // prevents a broken connection from blocking every future boot forever.
      response = await fetch(`${rpcUrl}/${fn}`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-spock-actor": viewerId(),
        },
        body: encode(payload),
        signal: timeout.signal,
      });
    } finally {
      clearTimeout(timeoutId);
    }
    const body = await response.text();
    if (response.ok) {
      return { ok: true, result: body ? JSON.parse(body) : null };
    }

    let error: SpockError = { message: body };
    try {
      const envelope = JSON.parse(body) as { error?: SpockError };
      error = envelope.error ?? error;
    } catch {
      // Preserve a non-JSON body as the provider-facing reason.
    }
    return { ok: false, error };
  }

  /**
   * Mint a short-lived signed download URL only when the shell is about to
   * render an asset. The provider owns expiry because the asset value carried
   * through Uhura remains the stable storage-object id.
   * @param {string} asset
   * @returns {Promise<string>}
   */
  async function resolveAsset(asset: string): Promise<string> {
    if (/^(?:[a-z][a-z0-9+.-]*:|\/)/iu.test(asset)) return asset;
    const local = LOCAL_PLAY_ASSETS[asset];
    if (local) {
      return `/api/play/assets/${encodeURIComponent(local)}`;
    }
    assertLive();
    const cached = signedAssets.get(asset);
    if (cached && Date.now() < cached.refreshAt) return cached.url;
    const current = signingAssets.get(asset);
    if (current) return current;

    const signing = (async () => {
      const { storageUrl } = await authorityEndpoints();
      const response = await fetch(
        `${storageUrl}/object/sign/${encodeURIComponent(asset)}`,
        {
          method: "POST",
          headers: { "x-spock-actor": viewerId() },
          signal: cancellable.signal,
        },
      );
      const body = await response.text();
      if (!response.ok) {
        throw new Error(
          `POST /storage/v1/object/sign/${asset}: ${response.status} ${body}`,
        );
      }
      const envelope = JSON.parse(body) as { url?: string };
      if (typeof envelope.url !== "string") {
        throw new Error("Spock storage signing returned no URL");
      }
      const absolute = resolveFromEndpoint(envelope.url, storageUrl);
      const expiry = Number(
        new URL(absolute, "https://spock.invalid/").searchParams.get("exp"),
      );
      const refreshAt = Number.isFinite(expiry)
        ? Math.max(Date.now(), expiry * 1000 - 30_000)
        : Date.now();
      signedAssets.set(asset, { url: absolute, refreshAt });
      return absolute;
    })();
    signingAssets.set(asset, signing);
    try {
      return await signing;
    } finally {
      if (signingAssets.get(asset) === signing) signingAssets.delete(asset);
    }
  }

  /**
   * Upload the browser-owned File directly to Spock's byte plane. Projections
   * carry only the resulting id and serializable display metadata; commands
   * never carry a File or its bytes.
   * @param {File} file
   * @returns {Promise<string>}
   */
  async function uploadFile(file: File): Promise<string> {
    const contentType = file.type.trim().toLowerCase();
    if (!SUPPORTED_IMAGE_TYPES.has(contentType)) {
      throw new Error("Choose an image file (JPEG, PNG, or WebP)");
    }

    const { storageUrl } = await authorityEndpoints();
    const mintResponse = await fetch(`${storageUrl}/object/upload/sign`, {
      method: "POST",
      headers: { "x-spock-actor": viewerId() },
      signal: cancellable.signal,
    });
    const mintBody = await mintResponse.text();
    if (!mintResponse.ok) {
      throw new Error(
        `POST /storage/v1/object/upload/sign: ${mintResponse.status} ${mintBody}`,
      );
    }
    const mint = JSON.parse(mintBody) as { id?: string; url?: string };
    if (typeof mint.id !== "string" || typeof mint.url !== "string") {
      throw new Error("Spock storage upload signing returned no object id or URL");
    }

    const putUrl = resolveFromEndpoint(mint.url, storageUrl);
    const putResponse = await fetch(putUrl, {
      method: "PUT",
      headers: { "content-type": contentType },
      body: file,
      signal: cancellable.signal,
    });
    const putBody = await putResponse.text();
    if (!putResponse.ok) {
      throw new Error(
        `PUT signed storage object: ${putResponse.status} ${putBody}`,
      );
    }
    signedAssets.delete(mint.id);
    return mint.id;
  }

  const db: Database = {
    users: new Map(),
    stories: [],
    storyViews: new Set(),
    posts: [],
    slidesByPost: new Map(),
    commentsByPost: new Map(),
    likeCounts: new Map(),
    liked: new Set(),
    saved: new Set(),
    follows: new Set(),
    followersByUser: new Map(),
    followingByUser: new Map(),
    taggedPostsByUser: new Map(),
  };
  let feedCount = PAGE_SIZE;
  let searchQuery = "";

  /**
   * Refresh every raw table cache from one GraphQL response. Relationship
   * objects are normalized back to their scalar foreign keys so the port
   * assembly below stays independent of GraphQL's representation.
   * @returns {Promise<void>}
   */
  async function loadAll(): Promise<void> {
    const data = await fetchSnapshot();

    db.users = new Map(data.users.map((user) => [user.id, user]));
    db.stories = data.stories
      .map((story) => ({
        id: story.id,
        author: story.author.id,
        position: story.position,
        media_file: story.media_file.id,
        media_alt: story.media_alt,
        caption: story.caption,
        published_at: story.published_at,
      }))
      .sort((left, right) => {
        const byTime = newestFirst(left.published_at, right.published_at);
        return byTime || left.position - right.position || left.id.localeCompare(right.id);
      });
    db.storyViews = new Set(
      data.storyViews.map((view) => edgeKey(view.viewer.id, view.story.id)),
    );
    db.posts = data.posts
      .map((post) => ({
        id: post.id,
        author: post.author.id,
        caption: post.caption,
        published_at: post.published_at,
        show_in_feed: post.show_in_feed,
        media_kind: post.media_kind,
        media_file: post.media_file?.id ?? null,
        video_file: post.video_file?.id ?? null,
        media_alt: post.media_alt,
      }))
      .sort((left, right) => {
        const byTime = newestFirst(left.published_at, right.published_at);
        return byTime || left.id.localeCompare(right.id);
      });

    const slides: SlideRow[] = data.slides.map((slide) => ({
      id: slide.id,
      post: slide.post.id,
      position: slide.position,
      file: slide.file.id,
      alt: slide.alt,
    }));
    db.slidesByPost = groupBy(slides, (slide) => slide.post);
    for (const rows of db.slidesByPost.values()) {
      rows.sort((left, right) => left.position - right.position);
    }

    const comments: CommentRow[] = data.comments.map((comment) => ({
      id: comment.id,
      post: comment.post.id,
      author: comment.author.id,
      body: comment.body,
      created_at: comment.created_at,
    }));
    db.commentsByPost = groupBy(comments, (comment) => comment.post);
    for (const rows of db.commentsByPost.values()) {
      rows.sort((left, right) => {
        const byTime = oldestFirst(left.created_at, right.created_at);
        return byTime || left.id.localeCompare(right.id);
      });
    }

    db.likeCounts = new Map();
    for (const like of data.likes) {
      const post = like.post.id;
      db.likeCounts.set(post, (db.likeCounts.get(post) ?? 0) + 1);
    }

    db.follows = new Set();
    db.followersByUser = new Map();
    db.followingByUser = new Map();
    for (const follow of data.follows) {
      const follower = follow.follower.id;
      const followed = follow.followed.id;
      db.follows.add(edgeKey(follower, followed));
      const followers = db.followersByUser.get(followed) ?? [];
      followers.push(follower);
      db.followersByUser.set(followed, followers);
      const following = db.followingByUser.get(follower) ?? [];
      following.push(followed);
      db.followingByUser.set(follower, following);
    }

    db.taggedPostsByUser = new Map();
    for (const tag of data.postTags) {
      const posts = db.taggedPostsByUser.get(tag.person.id) ?? [];
      posts.push(tag.post.id);
      db.taggedPostsByUser.set(tag.person.id, posts);
    }

    const resolved = data.users.find(
      (user) => user.id === actor || user.username === actor,
    );
    // Keep the authority-owned user directory available even when a stale
    // tab-local actor selection cannot resolve. `load` still refuses
    // that identity, but the system chrome can offer a valid actor and recover
    // by replacing the stored selection.
    viewerRow = resolved ?? null;
    db.liked = new Set(
      data.likes
        .filter((like) => like.user.id === resolved?.id)
        .map((like) => like.post.id),
    );
    db.saved = new Set(
      data.saves
        .filter((save) => save.user.id === resolved?.id)
        .map((save) => save.post.id),
    );
  }

  /**
   * @param {string} id
   * @returns {UserRow}
   */
  function requireUser(id: string): UserRow {
    const user = db.users.get(id);
    if (!user) throw new Error(`Spock snapshot references missing user \`${id}\``);
    return user;
  }

  /**
   * @param {PostRow} post
   * @returns {{ id: string, src: string, alt: string }}
   */
  function postThumb(post: PostRow): { id: string; src: string; alt: string } {
    if (post.media_kind === "carousel") {
      const first = (db.slidesByPost.get(post.id) ?? [])[0];
      if (!first) throw new Error(`carousel post \`${post.id}\` has no slides`);
      return { id: post.id, src: first.file, alt: first.alt };
    }
    if (post.media_file === null || post.media_alt === null) {
      throw new Error(`post \`${post.id}\` has no thumbnail media`);
    }
    return { id: post.id, src: post.media_file, alt: post.media_alt };
  }

  /**
   * Home is a relationship projection, not a global dump: the actor sees
   * their own publications and publications by accounts they currently
   * follow. Explore and reels deliberately use their own broader policies.
   * @param {string} author
   * @returns {boolean}
   */
  function isHomeAuthor(author: string): boolean {
    return author === viewerId() || db.follows.has(edgeKey(viewerId(), author));
  }

  /** @returns {PostRow[]} */
  function feedPosts(): PostRow[] {
    return db.posts.filter((post) => post.show_in_feed && isHomeAuthor(post.author));
  }

  function userWire(id: string): WireValue {
    const user = requireUser(id);
    return wireRecord([
      ["id", wireKey(USER_ID_TYPE, wireText(user.id))],
      ["username", wireText(user.username)],
      ["display_name", wireText(user.display_name)],
      [
        "avatar",
        wireRecord([
          ["src", wireText(user.avatar.id)],
          ["alt", wireText(user.avatar_alt)],
        ]),
      ],
    ]);
  }

  function imageWire(src: string, alt: string): WireValue {
    return wireRecord([
      ["src", wireText(src)],
      ["alt", wireText(alt)],
    ]);
  }

  function mediaWire(post: PostRow): WireValue {
    if (post.media_kind === "carousel") {
      const slides = db.slidesByPost.get(post.id) ?? [];
      return wireVariant(MEDIA_TYPE, "Carousel", [[
        "images",
        wireSeq(slides.map((slide) => imageWire(slide.file, slide.alt))),
      ]]);
    }
    if (post.media_file === null || post.media_alt === null) {
      throw new Error(
        `post \`${post.id}\` has incomplete ${post.media_kind} media`,
      );
    }
    const poster = imageWire(post.media_file, post.media_alt);
    if (post.media_kind === "video") {
      if (post.video_file === null) {
        throw new Error(`video post \`${post.id}\` has no playable video_file`);
      }
      return wireVariant(MEDIA_TYPE, "Video", [
        ["src", wireText(post.video_file)],
        ["poster", poster],
      ]);
    }
    return wireVariant(MEDIA_TYPE, "Image", [["image", poster]]);
  }

  function postWire(post: PostRow): WireValue {
    return wireRecord([
      ["id", wireKey(POST_ID_TYPE, wireText(post.id))],
      ["author", userWire(post.author)],
      ["caption", wireText(post.caption)],
      ["media", mediaWire(post)],
      ["like_count", wireNat(db.likeCounts.get(post.id) ?? 0)],
      [
        "comment_count",
        wireNat((db.commentsByPost.get(post.id) ?? []).length),
      ],
      ["viewer_liked", wireBool(db.liked.has(post.id))],
      ["viewer_saved", wireBool(db.saved.has(post.id))],
      ["posted_label", wireText(ageLabel(post.published_at))],
    ]);
  }

  function tileWire(post: PostRow): WireValue {
    const thumb = postThumb(post);
    return wireRecord([
      ["post", wireKey(POST_ID_TYPE, wireText(post.id))],
      ["image", imageWire(thumb.src, thumb.alt)],
    ]);
  }

  function connectionWire(id: string): WireValue {
    return wireRecord([
      ["user", userWire(id)],
      ["follows_viewer", wireBool(db.follows.has(edgeKey(id, viewerId())))],
      [
        "viewer_follows",
        wireBool(db.follows.has(edgeKey(viewerId(), id))),
      ],
    ]);
  }

  function connectionSequence(ids: readonly string[]): WireValue {
    const unique = [...new Set(ids)];
    unique.sort((left, right) =>
      requireUser(left).username.localeCompare(requireUser(right).username)
    );
    return wireSeq(unique.map(connectionWire));
  }

  function commentWire(comment: CommentRow): WireValue {
    return wireRecord([
      ["id", wireText(comment.id)],
      ["author", userWire(comment.author)],
      ["body", wireText(comment.body)],
      ["posted_label", wireText(ageLabel(comment.created_at))],
    ]);
  }

  function storyDetailWire(story: StoryRow): WireValue {
    const sequence = db.stories
      .filter((candidate) => candidate.author === story.author)
      .sort(
        (left, right) =>
          left.position - right.position || left.id.localeCompare(right.id),
      );
    const index = sequence.findIndex((candidate) => candidate.id === story.id);
    if (index < 0) {
      throw new Error(`story sequence lost frame \`${story.id}\``);
    }
    const previous = index > 0 ? sequence[index - 1]?.id ?? null : null;
    const next = index + 1 < sequence.length
      ? sequence[index + 1]?.id ?? null
      : null;
    return wireRecord([
      ["id", wireKey(STORY_ID_TYPE, wireText(story.id))],
      ["author", userWire(story.author)],
      ["image", imageWire(story.media_file, story.media_alt)],
      ["caption", wireText(story.caption ?? "")],
      ["posted_label", wireText(ageLabel(story.published_at))],
      [
        "viewed",
        wireBool(db.storyViews.has(edgeKey(viewerId(), story.id))),
      ],
      [
        "previous",
        wireOption(
          STORY_ID_TYPE,
          previous === null
            ? null
            : wireKey(STORY_ID_TYPE, wireText(previous)),
        ),
      ],
      [
        "next",
        wireOption(
          STORY_ID_TYPE,
          next === null ? null : wireKey(STORY_ID_TYPE, wireText(next)),
        ),
      ],
      [
        "progress",
        wireSeq(sequence.map((frame) =>
          wireRecord([
            ["id", wireKey(STORY_ID_TYPE, wireText(frame.id))],
            ["current", wireBool(frame.id === story.id)],
            [
              "viewed",
              wireBool(db.storyViews.has(edgeKey(viewerId(), frame.id))),
            ],
          ])
        )),
      ],
    ]);
  }

  function storyRingWires(): WireValue[] {
    const grouped = groupBy(
      db.stories.filter((story) => isHomeAuthor(story.author)),
      (story) => story.author,
    );
    const rings = [...grouped.entries()].map(([author, stories]) => {
      stories.sort(
        (left, right) =>
          left.position - right.position || left.id.localeCompare(right.id),
      );
      const unseen = stories.filter(
        (story) => !db.storyViews.has(edgeKey(viewerId(), story.id)),
      );
      const self = author === viewerId();
      const selected = self ? stories[0] : unseen[0] ?? stories[0];
      if (!selected) throw new Error(`story author \`${author}\` has no frames`);
      const newest = stories.reduce((latest, story) =>
        newestFirst(latest.published_at, story.published_at) <= 0
          ? latest
          : story
      );
      return {
        author,
        selected,
        newest,
        unseen: !self && unseen.length > 0,
        self,
      };
    });
    rings.sort((left, right) =>
      Number(right.self) - Number(left.self)
      || newestFirst(left.newest.published_at, right.newest.published_at)
      || requireUser(left.author).username.localeCompare(
        requireUser(right.author).username,
      )
    );
    return rings.map((ring) =>
      wireRecord([
        ["id", wireKey(STORY_ID_TYPE, wireText(ring.selected.id))],
        ["user", userWire(ring.author)],
        ["unseen", wireBool(ring.unseen)],
        ["is_self", wireBool(ring.self)],
      ])
    );
  }

  function profileWire(id: string): WireValue {
    const user = requireUser(id);
    const posts = db.posts.filter((post) => post.author === id);
    const tagged = new Set(db.taggedPostsByUser.get(id) ?? []);
    return wireRecord([
      ["user", userWire(id)],
      ["bio", wireText(user.bio ?? "")],
      ["post_count", wireNat(posts.length)],
      [
        "follower_count",
        wireNat((db.followersByUser.get(id) ?? []).length),
      ],
      [
        "following_count",
        wireNat((db.followingByUser.get(id) ?? []).length),
      ],
      [
        "viewer_follows",
        wireBool(db.follows.has(edgeKey(viewerId(), id))),
      ],
      ["posts", wireSeq(posts.map(tileWire))],
      [
        "reels",
        wireSeq(posts.filter((post) => post.media_kind === "video").map(tileWire)),
      ],
      [
        "tagged",
        wireSeq(db.posts.filter((post) => tagged.has(post.id)).map(tileWire)),
      ],
      [
        "saved",
        wireSeq(
          id === viewerId()
            ? db.posts.filter((post) => db.saved.has(post.id)).map(tileWire)
            : [],
        ),
      ],
    ]);
  }

  function authorityValue(): WireValue {
    const home = feedPosts();
    const visible = home.slice(0, feedCount);
    const needle = searchQuery.trim().toLocaleLowerCase();
    const searchPeople = [...db.users.values()]
      .filter((user) => user.id !== viewerId())
      .filter((user) =>
        needle.length === 0
        || user.username.toLocaleLowerCase().includes(needle)
        || user.display_name.toLocaleLowerCase().includes(needle)
      )
      .map((user) => user.id);
    const users = [...db.users.keys()];
    return wireVariant(AUTHORITY_TYPE, "Ready", [[
      "data",
      wireRecord([
        ["viewer", userWire(viewerId())],
        [
          "posts",
          wireTextKeyMap(POST_ID_TYPE, db.posts.map((post) => [
            post.id,
            postWire(post),
          ])),
        ],
        ["feed_posts", wireSeq(visible.map(postWire))],
        ["feed_has_more", wireBool(feedCount < home.length)],
        [
          "reels",
          wireSeq(
            db.posts.filter((post) => post.media_kind === "video").map(postWire),
          ),
        ],
        ["stories", wireSeq(storyRingWires())],
        [
          "story_details",
          wireTextKeyMap(STORY_ID_TYPE, db.stories.map((story) => [
            story.id,
            storyDetailWire(story),
          ])),
        ],
        [
          "profiles",
          wireTextKeyMap(USER_ID_TYPE, users.map((id) => [
            id,
            profileWire(id),
          ])),
        ],
        [
          "followers",
          wireTextKeyMap(USER_ID_TYPE, users.map((id) => [
            id,
            connectionSequence(db.followersByUser.get(id) ?? []),
          ])),
        ],
        [
          "following",
          wireTextKeyMap(USER_ID_TYPE, users.map((id) => [
            id,
            connectionSequence(db.followingByUser.get(id) ?? []),
          ])),
        ],
        [
          "comments",
          wireTextKeyMap(POST_ID_TYPE, db.posts.map((post) => [
            post.id,
            wireSeq((db.commentsByPost.get(post.id) ?? []).map(commentWire)),
          ])),
        ],
        ["search_people", connectionSequence(searchPeople)],
        ["explore_tiles", wireSeq(db.posts.map(tileWire))],
      ]),
    ]]);
  }

  /**
   * The provider cannot inspect pixels, so an omitted author description gets
   * provenance-only text rather than invented visual content.
   * @param {string} image
   * @returns {string}
   */
  function fallbackUploadAlt(image: string): string {
    const author = requireUser(viewerId());
    const fileName = uploadedFileNames.get(image)?.trim();
    return fileName
      ? `Uploaded image “${fileName}” by ${author.display_name}`
      : `Image uploaded by ${author.display_name}`;
  }

  function refusal(
    route: string,
    error: SpockError,
  ): BackendSettlement {
    const reason = toRefusalName(error.code ?? "");
    if ((COMMAND_REFUSALS[route] ?? []).includes(reason)) {
      return { kind: "refused", reason };
    }
    return {
      kind: "refused",
      reason: error.message ?? error.code ?? "provider-error",
    };
  }

  async function handle(
    operation: BackendOperation,
    pickedFile: PickedFile | undefined,
  ): Promise<BackendSettlement> {
    try {
      switch (operation.kind) {
        case "set_like": {
          const route = operation.liked
            ? "feed/like-post"
            : "feed/unlike-post";
          const reply = await rpc(
            operation.liked ? "like_post" : "unlike_post",
            { post: operation.post },
          );
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          await loadAll();
          return { kind: "accepted" };
        }
        case "set_save": {
          const route = operation.saved
            ? "feed/save-post"
            : "feed/unsave-post";
          const reply = await rpc(
            operation.saved ? "save_post" : "unsave_post",
            { post: operation.post },
          );
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          await loadAll();
          return { kind: "accepted" };
        }
        case "add_comment": {
          const route = "comments/add-comment";
          const reply = await rpc("add_comment", {
            post: operation.post,
            body: operation.body,
          });
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          await loadAll();
          return { kind: "accepted" };
        }
        case "load_more": {
          await loadAll();
          feedCount = Math.min(feedCount + PAGE_SIZE, feedPosts().length);
          return { kind: "accepted" };
        }
        case "reload_feed": {
          feedCount = PAGE_SIZE;
          await loadAll();
          return { kind: "accepted" };
        }
        case "mark_story": {
          const route = "feed/mark-story-seen";
          const reply = await rpc("mark_story_viewed", {
            story: operation.story,
          });
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          await loadAll();
          return { kind: "accepted" };
        }
        case "set_follow": {
          const route = operation.following
            ? "profile/follow-user"
            : "profile/unfollow-user";
          const reply = await rpc(
            operation.following ? "follow_user" : "unfollow_user",
            { target: operation.user },
          );
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          await loadAll();
          return { kind: "accepted" };
        }
        case "search_people": {
          searchQuery = operation.query;
          await loadAll();
          return { kind: "accepted" };
        }
        case "choose_image_request": {
          if (!pickedFile) {
            throw new Error("this play host cannot choose local files");
          }
          const picked = await pickedFile;
          if ("error" in picked) throw picked.error;
          if (picked.file === null) {
            return { kind: "refused", reason: "selection-cancelled" };
          }
          if (!SUPPORTED_IMAGE_TYPES.has(picked.file.type.trim().toLowerCase())) {
            return { kind: "refused", reason: "unsupported-media-type" };
          }
          const object = await uploadFile(picked.file);
          uploadedFileNames.set(object, picked.file.name);
          return {
            kind: "image_ready",
            object,
            preview: object,
            name: picked.file.name,
          };
        }
        case "publish_image_request": {
          const route = "create/publish-image";
          const alt = operation.alt.trim().length > 0
            ? operation.alt
            : fallbackUploadAlt(operation.object);
          const reply = await rpc("create_image_post", {
            image: operation.object,
            caption: operation.caption,
            alt,
          });
          if (reply.ok === false) {
            return refusal(route, reply.error);
          }
          if (
            typeof reply.result !== "object" ||
            reply.result === null ||
            !("id" in reply.result) ||
            typeof reply.result.id !== "string"
          ) {
            throw new Error("create_image_post returned no post id");
          }
          await loadAll();
          uploadedFileNames.delete(operation.object);
          return { kind: "accepted" };
        }
      }
    } catch (error) {
      return { kind: "refused", reason: errorMessage(error) };
    }
  }

  return {
    dispose,

    systemInfo() {
      return {
        actor: viewerRow?.id ?? actor,
        actors: [...db.users.values()]
          .sort((left, right) => left.username.localeCompare(right.username))
          .map((user) => ({
            id: user.id,
            username: user.username,
            label: user.display_name,
          })),
      };
    },

    async load() {
      await authorityTail;
      assertLive();
      await loadAll();
      assertLive();
      const viewer = viewerRow;
      if (!viewer) throw new Error(`actor \`${actor}\` is not a seeded user`);
      await verifyViewer();
    },

    execute(operation: BackendOperation): Promise<BackendSettlement> {
      assertLive();
      let pickedFile: PickedFile | undefined;
      if (operation.kind === "choose_image_request") {
        try {
          // This must happen in the click's synchronous call stack. Deferring
          // it behind the operation queue would lose browser user activation.
          pickedFile = host.pickFile({ accept: "image/jpeg,image/png,image/webp" })
            .then(
              (file) => ({ file }),
              (error) => ({ error }),
            );
        } catch (error) {
          pickedFile = Promise.resolve({ error });
        }
      }
      const work = operationTail.then(() => {
        assertLive();
        const run = () => handle(operation, pickedFile);
        return AUTHORITY_OPERATIONS.has(operation.kind)
          ? enqueueAuthorityWork(run)
          : run();
      });
      operationTail = work.then(
        () => {},
        () => {},
      );
      return work;
    },

    authorityValue,
    resolveAsset,
  };
}

function wireObject(value: unknown, context: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Record<string, unknown>;
}

function variantFields(
  value: WireValue,
  type: string,
  caseName?: string,
): Map<string | null, WireValue> {
  if (value.$ !== "variant" || value.type !== type) {
    throw new TypeError(`expected Uhura variant ${type}`);
  }
  if (caseName !== undefined && value.case !== caseName) {
    throw new TypeError(`expected Uhura variant ${type}.${caseName}`);
  }
  if (!Array.isArray(value.fields)) {
    throw new TypeError(`Uhura variant ${type} has no fields`);
  }
  const fields = new Map<string | null, WireValue>();
  for (const raw of value.fields) {
    const field = wireObject(raw, `${type} field`);
    const name = field.name;
    if (name !== null && typeof name !== "string") {
      throw new TypeError(`${type} field name must be text or null`);
    }
    const child = wireObject(field.value, `${type} field value`) as WireValue;
    if (fields.has(name)) throw new TypeError(`${type} repeats field ${String(name)}`);
    fields.set(name, child);
  }
  return fields;
}

function requiredField(
  fields: ReadonlyMap<string | null, WireValue>,
  name: string,
): WireValue {
  const value = fields.get(name);
  if (!value) throw new TypeError(`Uhura value has no field \`${name}\``);
  return value;
}

function keyText(value: WireValue, type: string): string {
  if (value.$ !== "key" || value.type !== type) {
    throw new TypeError(`expected Uhura key ${type}`);
  }
  const body = wireObject(value.value, `${type} body`);
  if (body.$ !== "Text" || typeof body.value !== "string") {
    throw new TypeError(`${type} must wrap Text`);
  }
  return body.value;
}

function requestText(value: WireValue): string {
  if (value.$ !== "key" || value.type !== REQUEST_ID_TYPE) {
    throw new TypeError(`expected Uhura key ${REQUEST_ID_TYPE}`);
  }
  const body = wireObject(value.value, `${REQUEST_ID_TYPE} body`);
  if (
    body.$ !== "PositiveInt"
    || typeof body.value !== "string"
    || !/^[1-9]\d*$/u.test(body.value)
  ) {
    throw new TypeError(`${REQUEST_ID_TYPE} must wrap PositiveInt`);
  }
  return body.value;
}

function textValue(value: WireValue): string {
  if (value.$ !== "Text" || typeof value.value !== "string") {
    throw new TypeError("expected Uhura Text");
  }
  return value.value;
}

function boolValue(value: WireValue): boolean {
  if (value.$ !== "bool" || typeof value.value !== "boolean") {
    throw new TypeError("expected Uhura Bool");
  }
  return value.value;
}

interface AdaptedRequest {
  readonly request: WireValue;
  readonly operation: BackendOperation;
}

function adaptRequest(command: WireValue): AdaptedRequest {
  const requestFields = variantFields(
    command,
    MUTATIONS_SEND_TYPE,
    "request",
  );
  const request = requiredField(requestFields, "id");
  requestText(request);
  const payload = requiredField(requestFields, "payload");
  const fields = variantFields(payload, MUTATION_TYPE);
  const mutation = String(payload.case);

  switch (mutation) {
    case "SetLike":
      return {
        request,
        operation: {
          kind: "set_like",
          post: keyText(requiredField(fields, "post"), POST_ID_TYPE),
          liked: boolValue(requiredField(fields, "liked")),
        },
      };
    case "SetSave":
      return {
        request,
        operation: {
          kind: "set_save",
          post: keyText(requiredField(fields, "post"), POST_ID_TYPE),
          saved: boolValue(requiredField(fields, "saved")),
        },
      };
    case "LoadMore":
      return { request, operation: { kind: "load_more" } };
    case "ReloadFeed":
      return { request, operation: { kind: "reload_feed" } };
    case "SetFollow":
      return {
        request,
        operation: {
          kind: "set_follow",
          user: keyText(requiredField(fields, "user"), USER_ID_TYPE),
          following: boolValue(requiredField(fields, "following")),
        },
      };
    case "AddComment":
      return {
        request,
        operation: {
          kind: "add_comment",
          post: keyText(requiredField(fields, "post"), POST_ID_TYPE),
          body: textValue(requiredField(fields, "body")),
        },
      };
    case "SearchPeople":
      return {
        request,
        operation: {
          kind: "search_people",
          query: textValue(requiredField(fields, "query")),
        },
      };
    case "ChooseImage":
      return { request, operation: { kind: "choose_image_request" } };
    case "PublishImage":
      return {
        request,
        operation: {
          kind: "publish_image_request",
          object: textValue(requiredField(fields, "object")),
          caption: textValue(requiredField(fields, "caption")),
          alt: textValue(requiredField(fields, "alt")),
        },
      };
    case "MarkStory":
      return {
        request,
        operation: {
          kind: "mark_story",
          story: keyText(requiredField(fields, "story"), STORY_ID_TYPE),
        },
      };
    default:
      throw new TypeError(`unsupported Instagram mutation \`${mutation}\``);
  }
}

function observed(value: WireValue): WireValue {
  return wireVariant(
    AUTHORITY_RECEIVE_TYPE,
    "authority.observed",
    [["value", value]],
  );
}

function refused(reason: string): WireValue {
  return wireVariant(SETTLEMENT_TYPE, "Refused", [[
    "reason",
    wireText(reason),
  ]]);
}

function settlementValue(result: BackendSettlement): WireValue {
  switch (result.kind) {
    case "accepted":
      return wireVariant(SETTLEMENT_TYPE, "Accepted");
    case "refused":
      return refused(result.reason);
    case "image_ready":
      return wireVariant(SETTLEMENT_TYPE, "ImageReady", [
        ["object", wireText(result.object)],
        ["preview", wireText(result.preview)],
        ["name", wireText(result.name)],
      ]);
  }
}

function settled(request: WireValue, result: WireValue): WireValue {
  return wireVariant(
    MUTATIONS_RECEIVE_TYPE,
    "mutations.settled",
    [
      ["id", request],
      ["result", result],
    ],
  );
}

function providerConfig(
  config: Readonly<Record<string, unknown>>,
): SpockProviderConfig {
  const value = (name: keyof SpockProviderConfig): string => {
    const entry = config[name];
    if (typeof entry !== "string" || entry.trim().length === 0) {
      throw new TypeError(`Instagram provider needs nonempty \`${name}\``);
    }
    return entry;
  };
  return {
    graphql_url: value("graphql_url"),
    rpc_url: value("rpc_url"),
    storage_url: value("storage_url"),
    actor: value("actor"),
  };
}

/**
 * Current Uhura adapter entry point. Contract identities come from the
 * admitted Play deployment; the app provider never calculates or hardcodes
 * compiler-owned hashes.
 */
export function createUhuraAdapters(
  config: Readonly<Record<string, unknown>>,
  host: AdapterProviderHost,
): {
  readonly adapters: readonly PortAdapter[];
  resolveAsset(asset: string): Promise<string>;
  systemInfo(): RemoteSystemInfo;
  dispose(): void;
} {
  const backend = createSpockBackend(providerConfig(config), host);
  const authorityRequirement = host.port("authority");
  const mutationsRequirement = host.port("mutations");
  let authorityContext: PortAdapterContext | null = null;

  const authority: PortAdapter = {
    ...authorityRequirement,
    async start(context): Promise<void> {
      authorityContext = context;
      try {
        await backend.load();
        context.deliver(observed(backend.authorityValue()));
      } catch (error) {
        context.deliver(
          observed(
            wireVariant(AUTHORITY_TYPE, "Failed", [[
              "reason",
              wireText(errorMessage(error)),
            ]]),
          ),
        );
      }
    },
    accept(): never {
      throw new Error("Observation<Authority> does not accept commands");
    },
  };

  const mutations: PortAdapter = {
    ...mutationsRequirement,
    accept(command, context): Promise<void> {
      const adapted = adaptRequest(command);
      const work = backend.execute(adapted.operation).then((settlement) => {
        const result = settlementValue(settlement);
        if (
          result.case === "Accepted"
          && adapted.operation.kind !== "choose_image_request"
        ) {
          authorityContext?.deliver(observed(backend.authorityValue()));
        }
        context.deliver(settled(adapted.request, result));
      });
      return work;
    },
  };

  return {
    adapters: [authority, mutations],
    resolveAsset: (asset) => backend.resolveAsset(asset),
    systemInfo: () => backend.systemInfo(),
    dispose: () => backend.dispose(),
  };
}

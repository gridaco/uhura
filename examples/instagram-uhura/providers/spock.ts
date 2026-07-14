// Instagram's app-local Spock provider. It speaks the same
// `uhura-provider/0` envelopes as FixtureDriver, but reads one coherent
// authority snapshot through Spock GraphQL and sends commands through the
// deliberate REST RPC surface.

const PAGE_SIZE = 4;
const SUPPORTED_IMAGE_TYPES = new Set([
  "image/jpeg",
  "image/png",
  "image/webp",
]);

// Spock v0 caps one collection read at 200 rows. The current demo fits inside
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

export interface SpockDriverConfig {
  /** Full Spock `/graphql/v1` endpoint. */
  graphql_url: string;
  /** Spock `/rest/v1/rpc` prefix. */
  rpc_url: string;
  /** Spock `/storage/v1` prefix. */
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

export interface RemoteSystemInfo {
  actor: string | null;
  actors: Array<{ id: string; username: string; label: string }>;
}

export interface SpockDriver {
  dispose(): void;
  assembleBoot(): Promise<string>;
  deliver(commandJson: string): void;
  tick(): string[];
  idle(): boolean;
  resolveAsset(asset: string): Promise<string>;
  systemInfo(): RemoteSystemInfo;
}

/**
 * A picker result is observed immediately so a rejection cannot become
 * unhandled while an earlier provider command finishes.
 */
type PickedFile = Promise<{ file: File | null } | { error: unknown }>;

// A retired driver can have a mutation already accepted by Spock. New driver
// boot waits for that work before reading its authority snapshot, so a route
// remount cannot strand a just-accepted mutation behind stale boot data.
let authorityTail: Promise<void> = Promise.resolve();

const AUTHORITY_COMMANDS = new Set([
  "feed/like-post",
  "feed/unlike-post",
  "feed/save-post",
  "feed/unsave-post",
  "comments/add-comment",
  "feed/mark-story-seen",
  "profile/follow-user",
  "profile/unfollow-user",
  "create/publish-image",
]);

const AUTHORITY_REQUEST_TIMEOUT_MS = 15_000;

function enqueueAuthorityWork(work: () => Promise<void>): Promise<void> {
  const queued = authorityTail.then(work, work);
  authorityTail = queued.catch(() => {});
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

interface ProviderCommand {
  kind: "command";
  port: string;
  command: string;
  correlation: string;
  payload: Record<string, unknown>;
}

interface ProjectionUpdate {
  port: string;
  projection: string;
  key: unknown;
  revision: number;
  value: unknown;
}

type CommandOutcome =
  | { ok: Record<string, never> }
  | { refused: { refusal: string } }
  | { unavailable: { reason: string } };

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
 * Create the Instagram demo's live Spock-backed provider.
 *
 * Delivery is eager: boot queues every keyed post, comment thread, story,
 * profile, and relationship list plus the feed, reels, people search, and
 * create draft. Commands settle by re-reading one authority snapshot and
 * carrying whole-slice updates in their outcome envelope.
 *
 * @param {SpockDriverConfig} config
 * @param {ProviderHost} host
 * @returns {SpockDriver}
 */
export function createDriver(
  { graphql_url, rpc_url, storage_url, actor }: SpockDriverConfig,
  host: ProviderHost,
): SpockDriver {
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
  const whoamiUrl = new URL("/~whoami", graphqlUrl).toString();

  const outbox: string[] = [];
  let inflight = 0;
  let commandTail: Promise<void> = Promise.resolve();

  const signedAssets = new Map<string, { url: string; refreshAt: number }>();
  const signingAssets = new Map<string, Promise<string>>();
  const uploadedFileNames = new Map<string, string>();
  const cancellable = new AbortController();
  let disposed = host.signal.aborted;

  function dispose(): void {
    if (disposed) return;
    disposed = true;
    host.signal.removeEventListener("abort", dispose);
    cancellable.abort();
    outbox.length = 0;
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

  const revisions = new Map<string, number>();

  /**
   * @param {string} port
   * @param {string} projection
   * @param {unknown} key
   * @returns {number}
   */
  function mintRevision(
    port: string,
    projection: string,
    key: unknown,
  ): number {
    const slot = `${port}|${projection}|${encode(key ?? null)}`;
    const next = (revisions.get(slot) ?? 1) + 1;
    revisions.set(slot, next);
    return next;
  }

  /**
   * @param {string} port
   * @param {string} projection
   * @param {unknown} key
   * @param {unknown} value
   * @returns {string}
   */
  function projectionMsg(
    port: string,
    projection: string,
    key: unknown,
    value: unknown,
  ): string {
    return encode({
      kind: "projection",
      port,
      projection,
      key: key ?? null,
      revision: mintRevision(port, projection, key),
      value,
    });
  }

  /**
   * @param {string} port
   * @param {string} projection
   * @param {unknown} key
   * @param {unknown} value
   * @returns {ProjectionUpdate}
   */
  function projectionUpdate(
    port: string,
    projection: string,
    key: unknown,
    value: unknown,
  ): ProjectionUpdate {
    return {
      port,
      projection,
      key: key ?? null,
      revision: mintRevision(port, projection, key),
      value,
    };
  }

  /**
   * @returns {Promise<SnapshotData>}
   */
  async function fetchSnapshot(): Promise<SnapshotData> {
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
    const timeout = new AbortController();
    const timeoutId = setTimeout(
      () => timeout.abort(),
      AUTHORITY_REQUEST_TIMEOUT_MS,
    );
    let response: Response;
    try {
      // Once sent, a domain mutation may already be accepted by Spock. Do not
      // abort it merely because its route retired; the module-level authority
      // barrier makes the next driver wait for settlement. The finite timeout
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
    assertLive();
    const cached = signedAssets.get(asset);
    if (cached && Date.now() < cached.refreshAt) return cached.url;
    const current = signingAssets.get(asset);
    if (current) return current;

    const signing = (async () => {
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
      const absolute = new URL(envelope.url, storageUrl).toString();
      const expiry = Number(new URL(absolute).searchParams.get("exp"));
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

    const putUrl = new URL(mint.url, storageUrl).toString();
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
    // Keep the authority-owned user catalog available even when a stale
    // tab-local actor selection cannot resolve. assembleBoot still refuses
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
   * @param {UserRow} user
   * @returns {Record<string, unknown>}
   */
  function userRef(user: UserRow) {
    return {
      id: user.id,
      username: user.username,
      "display-name": user.display_name,
      avatar: { src: user.avatar.id, alt: user.avatar_alt },
    };
  }

  /**
   * @param {PostRow} post
   * @returns {Record<string, unknown>}
   */
  function media(post: PostRow) {
    if (post.media_kind === "carousel") {
      const slides = db.slidesByPost.get(post.id) ?? [];
      return {
        carousel: {
          slides: slides.map((slide) => ({
            id: slide.id,
            src: slide.file,
            alt: slide.alt,
          })),
        },
      };
    }
    if (post.media_file === null || post.media_alt === null) {
      throw new Error(`post \`${post.id}\` has incomplete ${post.media_kind} media`);
    }
    const ref = { src: post.media_file, alt: post.media_alt };
    if (post.media_kind === "video") {
      if (post.video_file === null) {
        throw new Error(`video post \`${post.id}\` has no playable video_file`);
      }
      return { video: { src: post.video_file, poster: ref } };
    }
    return { image: { image: ref } };
  }

  /**
   * @param {PostRow} post
   * @returns {Record<string, unknown>}
   */
  function postSummary(post: PostRow) {
    return {
      id: post.id,
      author: userRef(requireUser(post.author)),
      media: media(post),
      caption: post.caption,
      "like-count": db.likeCounts.get(post.id) ?? 0,
      "comment-count": (db.commentsByPost.get(post.id) ?? []).length,
      "viewer-has-liked": db.liked.has(post.id),
      "viewer-has-saved": db.saved.has(post.id),
      "posted-label": ageLabel(post.published_at),
    };
  }

  /**
   * @param {string} id
   * @returns {PostRow}
   */
  function requirePost(id: string): PostRow {
    const post = db.posts.find((candidate) => candidate.id === id);
    if (!post) throw new Error(`Spock snapshot has no post \`${id}\``);
    return post;
  }

  /**
   * @param {string} id
   * @returns {StoryRow}
   */
  function requireStory(id: string): StoryRow {
    const story = db.stories.find((candidate) => candidate.id === id);
    if (!story) throw new Error(`Spock snapshot has no story \`${id}\``);
    return story;
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

  /**
   * One tray entry represents one author's current story sequence. Its id is
   * the next unseen frame (or the first frame after the sequence is exhausted),
   * so opening a ring always addresses a real keyed story projection.
   * @returns {Record<string, unknown>[]}
   */
  function storyRingsValue() {
    const grouped = groupBy(
      db.stories.filter((story) => isHomeAuthor(story.author)),
      (story) => story.author,
    );
    const rings = [...grouped.entries()].map(([author, stories]) => {
      stories.sort(
        (left, right) => left.position - right.position || left.id.localeCompare(right.id),
      );
      const unseen = stories.filter(
        (story) => !db.storyViews.has(edgeKey(viewerId(), story.id)),
      );
      const isSelf = author === viewerId();
      const selected = isSelf ? stories[0] : unseen[0] ?? stories[0];
      if (!selected) throw new Error(`story author \`${author}\` has no frames`);
      const newest = stories.reduce((latest, story) =>
        newestFirst(latest.published_at, story.published_at) <= 0 ? latest : story,
      );
      return {
        author,
        selected,
        newest,
        hasUnseen: !isSelf && unseen.length > 0,
      };
    });
    rings.sort((left, right) => {
      const leftSelf = left.author === viewerId() ? 1 : 0;
      const rightSelf = right.author === viewerId() ? 1 : 0;
      return (
        rightSelf - leftSelf ||
        newestFirst(left.newest.published_at, right.newest.published_at) ||
        requireUser(left.author).username.localeCompare(requireUser(right.author).username)
      );
    });
    return rings.map((ring) => ({
      id: ring.selected.id,
      user: userRef(requireUser(ring.author)),
      "has-unseen": ring.hasUnseen,
      "is-self": ring.author === viewerId(),
    }));
  }

  /** @returns {Record<string, unknown>} */
  function feedPageValue() {
    const posts = feedPosts();
    const shown = posts.slice(0, feedCount);
    const hasMore = feedCount < posts.length;
    return {
      stories: storyRingsValue(),
      posts: shown.map((post) => postSummary(post)),
      cursor: hasMore ? `offset:${feedCount}` : null,
      "has-more": hasMore,
    };
  }

  /**
   * @param {string} postId
   * @returns {Record<string, unknown>}
   */
  function threadValue(postId: string) {
    const rows = db.commentsByPost.get(postId) ?? [];
    return {
      comments: rows.map((comment) => ({
        id: comment.id,
        author: userRef(requireUser(comment.author)),
        body: comment.body,
        "posted-label": ageLabel(comment.created_at),
      })),
    };
  }

  /**
   * @param {string} storyId
   * @returns {Record<string, unknown>}
   */
  function storyValue(storyId: string) {
    const story = requireStory(storyId);
    const sequence = db.stories
      .filter((candidate) => candidate.author === story.author)
      .sort(
        (left, right) =>
          left.position - right.position || left.id.localeCompare(right.id),
      );
    const index = sequence.findIndex((candidate) => candidate.id === story.id);
    if (index < 0) throw new Error(`story sequence lost frame \`${story.id}\``);
    return {
      id: story.id,
      author: userRef(requireUser(story.author)),
      image: { src: story.media_file, alt: story.media_alt },
      caption: story.caption ?? "",
      "posted-label": ageLabel(story.published_at),
      "viewer-has-viewed": db.storyViews.has(edgeKey(viewerId(), story.id)),
      previous: index > 0 ? sequence[index - 1]?.id ?? null : null,
      next:
        index + 1 < sequence.length ? sequence[index + 1]?.id ?? null : null,
      progress: sequence.map((frame) => ({
        id: frame.id,
        "is-current": frame.id === story.id,
        "is-viewed": db.storyViews.has(edgeKey(viewerId(), frame.id)),
      })),
    };
  }

  /** @returns {Record<string, unknown>} */
  function reelsValue() {
    return {
      posts: db.posts
        .filter((post) => post.media_kind === "video")
        .map((post) => postSummary(post)),
    };
  }

  /**
   * @param {string} userId
   * @returns {Record<string, unknown>}
   */
  function profileValue(userId: string) {
    const user = requireUser(userId);
    const posts = db.posts.filter((post) => post.author === userId);
    const reels = posts.filter((post) => post.media_kind === "video");
    const saved =
      userId === viewerId()
        ? db.posts.filter((post) => db.saved.has(post.id))
        : [];
    const taggedIds = new Set(db.taggedPostsByUser.get(userId) ?? []);
    const tagged = db.posts.filter((post) => taggedIds.has(post.id));
    return {
      user: userRef(user),
      bio: user.bio ?? "",
      "is-self": userId === viewerId(),
      "viewer-follows": db.follows.has(edgeKey(viewerId(), userId)),
      "post-count": posts.length,
      "follower-count": (db.followersByUser.get(userId) ?? []).length,
      "following-count": (db.followingByUser.get(userId) ?? []).length,
      posts: posts.map((post) => postThumb(post)),
      reels: reels.map((post) => postThumb(post)),
      saved: saved.map((post) => postThumb(post)),
      tagged: tagged.map((post) => postThumb(post)),
    };
  }

  /**
   * @param {string[]} userIds
   * @returns {Record<string, unknown>}
   */
  function connectionsValue(userIds: string[]) {
    return {
      people: userIds
        .map((id) => requireUser(id))
        .sort((left, right) => left.username.localeCompare(right.username))
        .map((user) => ({
          user: userRef(user),
          "viewer-follows": db.follows.has(edgeKey(viewerId(), user.id)),
        })),
    };
  }

  /**
   * @param {string} userId
   * @returns {Record<string, unknown>}
   */
  function followersValue(userId: string) {
    requireUser(userId);
    return connectionsValue(db.followersByUser.get(userId) ?? []);
  }

  /**
   * @param {string} userId
   * @returns {Record<string, unknown>}
   */
  function followingValue(userId: string) {
    requireUser(userId);
    return connectionsValue(db.followingByUser.get(userId) ?? []);
  }

  /**
   * @param {string} query
   * @returns {Record<string, unknown>}
   */
  function searchValue(query: string) {
    const needle = query.trim().toLocaleLowerCase();
    const people = [...db.users.values()]
      .filter((user) => user.id !== viewerId())
      .filter(
        (user) =>
          needle.length === 0 ||
          user.username.toLocaleLowerCase().includes(needle) ||
          user.display_name.toLocaleLowerCase().includes(needle),
      )
      .map((user) => user.id);
    const posts = db.posts.filter((post) => {
      if (needle.length === 0) return true;
      const author = requireUser(post.author);
      return (
        post.caption.toLocaleLowerCase().includes(needle) ||
        author.username.toLocaleLowerCase().includes(needle) ||
        author.display_name.toLocaleLowerCase().includes(needle)
      );
    });
    return {
      people: connectionsValue(people).people,
      posts: posts.map((post) => postThumb(post)),
    };
  }

  /**
   * @param {string} postId
   * @param {boolean} includeThread
   * @returns {ProjectionUpdate[]}
   */
  function postSettlementUpdates(
    postId: string,
    includeThread: boolean,
  ): ProjectionUpdate[] {
    const updates = [
      projectionUpdate("feed", "feed-page", null, feedPageValue()),
      projectionUpdate(
        "feed",
        "post-by-id",
        postId,
        postSummary(requirePost(postId)),
      ),
      projectionUpdate("feed", "reels", null, reelsValue()),
    ];
    if (includeThread) {
      updates.push(
        projectionUpdate("comments", "for-post", postId, threadValue(postId)),
      );
    }
    return updates;
  }

  /**
   * Saving changes every viewer-specific rendering of a post plus the private
   * Saved grid on the actor's own profile.
   * @param {string} postId
   * @returns {ProjectionUpdate[]}
   */
  function saveSettlementUpdates(postId: string): ProjectionUpdate[] {
    return [
      ...postSettlementUpdates(postId, false),
      projectionUpdate(
        "profile",
        "profile",
        viewerId(),
        profileValue(viewerId()),
      ),
    ];
  }

  /**
   * A viewed edge changes the ring and every frame's progress strip in that
   * author's sequence, so settle them as one authority snapshot.
   * @param {string} storyId
   * @returns {ProjectionUpdate[]}
   */
  function storySettlementUpdates(storyId: string): ProjectionUpdate[] {
    const author = requireStory(storyId).author;
    return [
      projectionUpdate("feed", "feed-page", null, feedPageValue()),
      ...db.stories
        .filter((story) => story.author === author)
        .map((story) =>
          projectionUpdate(
            "feed",
            "story-by-id",
            story.id,
            storyValue(story.id),
          ),
        ),
    ];
  }

  /** @returns {ProjectionUpdate[]} */
  function allSocialUpdates(): ProjectionUpdate[] {
    const updates: ProjectionUpdate[] = [
      projectionUpdate("feed", "feed-page", null, feedPageValue()),
    ];
    for (const userId of db.users.keys()) {
      updates.push(
        projectionUpdate("profile", "profile", userId, profileValue(userId)),
        projectionUpdate("profile", "followers", userId, followersValue(userId)),
        projectionUpdate("profile", "following", userId, followingValue(userId)),
      );
    }
    updates.push(
      projectionUpdate("profile", "search-results", null, searchValue(searchQuery)),
    );
    return updates;
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

  /**
   * @param {ProviderCommand} command
   * @param {string} field
   * @returns {string}
   */
  function payloadString(command: ProviderCommand, field: string): string {
    const value = command.payload[field];
    if (typeof value !== "string") {
      throw new Error(`command \`${command.port}/${command.command}\` needs string \`${field}\``);
    }
    return value;
  }

  /**
   * @param {string} route
   * @param {SpockError} error
   * @returns {CommandOutcome}
   */
  function refuseOrUnavailable(
    route: string,
    error: SpockError,
  ): CommandOutcome {
    const refusal = toRefusalName(error.code ?? "");
    if ((COMMAND_REFUSALS[route] ?? []).includes(refusal)) {
      return { refused: { refusal } };
    }
    return {
      unavailable: { reason: error.message ?? error.code ?? "provider error" },
    };
  }

  /**
   * @param {ProviderCommand} command
   * @param {CommandOutcome} result
   * @param {ProjectionUpdate[]} [updates]
   * @returns {void}
   */
  function outcome(
    command: ProviderCommand,
    result: CommandOutcome,
    updates: ProjectionUpdate[] = [],
  ): void {
    if (disposed) return;
    outbox.push(
      encode({
        kind: "outcome",
        correlation: command.correlation,
        outcome: result,
        updates,
      }),
    );
  }

  /**
   * @param {ProviderCommand} command
   * @param {PickedFile | undefined} pickedFile
   * @returns {Promise<void>}
   */
  async function handle(
    command: ProviderCommand,
    pickedFile: PickedFile | undefined,
  ): Promise<void> {
    const route = `${command.port}/${command.command}`;
    try {
      switch (route) {
        case "feed/like-post":
        case "feed/unlike-post": {
          const post = payloadString(command, "post");
          const fn = command.command === "like-post" ? "like_post" : "unlike_post";
          const reply = await rpc(fn, { post });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          await loadAll();
          outcome(command, { ok: {} }, postSettlementUpdates(post, false));
          return;
        }
        case "feed/save-post":
        case "feed/unsave-post": {
          const post = payloadString(command, "post");
          const fn = command.command === "save-post" ? "save_post" : "unsave_post";
          const reply = await rpc(fn, { post });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          await loadAll();
          outcome(command, { ok: {} }, saveSettlementUpdates(post));
          return;
        }
        case "comments/add-comment": {
          const post = payloadString(command, "post");
          const body = payloadString(command, "body");
          const reply = await rpc("add_comment", { post, body });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          await loadAll();
          outcome(command, { ok: {} }, postSettlementUpdates(post, true));
          return;
        }
        case "feed/load-next-page": {
          await loadAll();
          feedCount = Math.min(feedCount + PAGE_SIZE, feedPosts().length);
          outcome(command, { ok: {} }, [
            projectionUpdate("feed", "feed-page", null, feedPageValue()),
          ]);
          return;
        }
        case "feed/reload": {
          feedCount = PAGE_SIZE;
          await loadAll();
          outcome(command, { ok: {} }, [
            projectionUpdate("feed", "feed-page", null, feedPageValue()),
          ]);
          return;
        }
        case "feed/mark-story-seen": {
          const story = payloadString(command, "story");
          const reply = await rpc("mark_story_viewed", { story });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          await loadAll();
          outcome(command, { ok: {} }, storySettlementUpdates(story));
          return;
        }
        case "profile/follow-user":
        case "profile/unfollow-user": {
          const user = payloadString(command, "user");
          const fn = command.command === "follow-user" ? "follow_user" : "unfollow_user";
          const reply = await rpc(fn, { target: user });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          await loadAll();
          outcome(command, { ok: {} }, allSocialUpdates());
          return;
        }
        case "profile/search-people": {
          searchQuery = payloadString(command, "query");
          await loadAll();
          outcome(command, { ok: {} }, [
            projectionUpdate(
              "profile",
              "search-results",
              null,
              searchValue(searchQuery),
            ),
          ]);
          return;
        }
        case "create/choose-image": {
          if (!pickedFile) {
            throw new Error("this play host cannot choose local files");
          }
          const picked = await pickedFile;
          if ("error" in picked) throw picked.error;
          if (picked.file === null) {
            outcome(command, { ok: {} });
            return;
          }
          if (!SUPPORTED_IMAGE_TYPES.has(picked.file.type.trim().toLowerCase())) {
            outcome(command, {
              refused: { refusal: "unsupported-media-type" },
            });
            return;
          }
          const object = await uploadFile(picked.file);
          uploadedFileNames.set(object, picked.file.name);
          outcome(command, { ok: {} }, [
            projectionUpdate("create", "draft", null, {
              uploaded: {
                object,
                preview: object,
                name: picked.file.name,
              },
            }),
          ]);
          return;
        }
        case "create/publish-image": {
          const image = payloadString(command, "image");
          const caption = payloadString(command, "caption");
          const requestedAlt = payloadString(command, "alt");
          const alt = requestedAlt.trim().length > 0
            ? requestedAlt
            : fallbackUploadAlt(image);
          const reply = await rpc("create_image_post", { image, caption, alt });
          if (reply.ok === false) {
            outcome(command, refuseOrUnavailable(route, reply.error));
            return;
          }
          if (
            typeof reply.result !== "object" ||
            reply.result === null ||
            !("id" in reply.result) ||
            typeof reply.result.id !== "string"
          ) {
            throw new Error("create_image_post returned no post id");
          }
          const post = reply.result.id;
          await loadAll();
          uploadedFileNames.delete(image);
          outcome(command, { ok: {} }, [
            projectionUpdate("feed", "feed-page", null, feedPageValue()),
            projectionUpdate(
              "feed",
              "post-by-id",
              post,
              postSummary(requirePost(post)),
            ),
            projectionUpdate("comments", "for-post", post, threadValue(post)),
            projectionUpdate("profile", "profile", viewerId(), profileValue(viewerId())),
            projectionUpdate(
              "profile",
              "search-results",
              null,
              searchValue(searchQuery),
            ),
            projectionUpdate("create", "draft", null, { empty: {} }),
          ]);
          return;
        }
        default:
          outcome(command, {
            unavailable: { reason: `no binding for command \`${route}\`` },
          });
      }
    } catch (error) {
      outcome(command, { unavailable: { reason: errorMessage(error) } });
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

    async assembleBoot() {
      await authorityTail;
      assertLive();
      await loadAll();
      assertLive();
      const viewer = viewerRow;
      if (!viewer) throw new Error(`actor \`${actor}\` is not a seeded user`);
      await verifyViewer();

      outbox.push(projectionMsg("feed", "feed-page", null, feedPageValue()));
      for (const post of db.posts) {
        outbox.push(
          projectionMsg("comments", "for-post", post.id, threadValue(post.id)),
          projectionMsg("feed", "post-by-id", post.id, postSummary(post)),
        );
      }
      for (const story of db.stories) {
        outbox.push(
          projectionMsg(
            "feed",
            "story-by-id",
            story.id,
            storyValue(story.id),
          ),
        );
      }
      outbox.push(projectionMsg("feed", "reels", null, reelsValue()));
      for (const userId of db.users.keys()) {
        outbox.push(
          projectionMsg("profile", "profile", userId, profileValue(userId)),
          projectionMsg("profile", "followers", userId, followersValue(userId)),
          projectionMsg("profile", "following", userId, followingValue(userId)),
        );
      }
      outbox.push(
        projectionMsg(
          "profile",
          "search-results",
          null,
          searchValue(searchQuery),
        ),
      );
      outbox.push(projectionMsg("create", "draft", null, { empty: {} }));

      return encode({
        updates: [
          {
            port: "feed",
            projection: "viewer",
            key: null,
            revision: 1,
            value: userRef(viewer),
          },
        ],
      });
    },

    deliver(commandJson: string) {
      if (disposed) return;
      const command = JSON.parse(commandJson) as ProviderCommand;
      let pickedFile: PickedFile | undefined;
      if (`${command.port}/${command.command}` === "create/choose-image") {
        try {
          // This must happen in the click's synchronous call stack. Deferring
          // it behind commandTail would lose browser user activation.
          pickedFile = host.pickFile({ accept: "image/jpeg,image/png,image/webp" })
            .then(
              (file) => ({ file }),
              (error) => ({ error }),
            );
        } catch (error) {
          pickedFile = Promise.resolve({ error });
        }
      }
      inflight += 1;
      // Preserve delivery order inside this driver. Only domain mutations
      // enter the cross-driver authority barrier: a picker, upload draft, or
      // ordinary read must never strand a later Play boot.
      const route = `${command.port}/${command.command}`;
      const predecessor = commandTail;
      commandTail = predecessor
        .then(() => {
          if (disposed) return;
          const work = () => handle(command, pickedFile);
          return AUTHORITY_COMMANDS.has(route)
            ? enqueueAuthorityWork(work)
            : work();
        })
        .finally(() => {
          inflight -= 1;
        });
    },

    tick() {
      if (disposed) return [];
      return outbox.splice(0, outbox.length);
    },

    idle() {
      return inflight === 0 && outbox.length === 0;
    },

    resolveAsset,
  };
}

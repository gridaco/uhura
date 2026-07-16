# Uhura spike design — Instagram vertical slice (v3, model-driven Editor)

- **Status:** Working-group design — non-normative, implementation-guiding
- **Owner:** [Uhura Working Group](README.md)
- **Foundation:** [RFC 0001](../rfcs/0001-project-foundation.md), [Uhura specification](../spec/README.md)
- **Requirements evidence:** [Application-scale stress-test requirements](application-scale-stress-test.md)
- **Provenance:** v1 was synthesized from seven parallel subsystem designs +
  an examples-system design, reconciled against ~90 adversarial critique
  findings. **v2 reframes the view layer** after a direction challenge
  ("successor of HTML/CSS + widgets; a minimal Svelte that does not support
  JS") and folds in a final verification pass. **v3 replaces the generated
  Editor document with a versioned native read model and one browser
  application for Editor and Play.** §15 logs the direction changes.

**Elevator pitch:** Uhura is a minimal Svelte without JavaScript — Svelte-
flavored markup and bindings over a closed, total transition language and
typed service ports, compiled by a Rust checker into a deterministic,
replayable, headless machine. Styling is real CSS. The machine is the spec.

---

## 1. Mission and scope

**The spike must prove, end to end:**

1. A closed, checkable `.uhura` source language — Svelte-flavored markup +
   a `store` machine block — under a meta-framework file convention
   (`app/feed/page.uhura`, `components/post-card.uhura`).
2. A deterministic, I/O-free core: `step-u(P, U, X, E) → (U', V, C, I, G, T)`.
3. A renderer-neutral **semantic** view `V` (styling is deliberately
   web-native CSS — see adjudication #21) consumed by one shared browser
   renderer under explicit inert-Editor and interactive-Play policies.
4. A language-neutral port seam a real Spock provider could satisfy without
   touching Uhura source, IR, or core (§9.6).
5. A **small, closed-but-extensible semantic element set** with a checked
   ruleset — interaction/content semantics only; layout and aesthetics
   belong to CSS and are free to drift.
6. **Example-defined design**: named, checked example states per
   page/component/surface, rendered as previews on the Editor's infinite
   board (§6).

**Content: the Instagram main flow.** In: a relationship-filtered home feed
(story tray, image, carousel, and playable video), optimistic likes and private
saves, comments, pagination, Search/Explore, vertical Reels, multi-frame Story
viewing, post detail, profiles with posts/Reels/Saved/tagged grids and truthful
relational counts, Followers/Following lists, follow/unfollow, and a signed
storage upload-to-publish flow. Loading, failed, empty, pending, and refusal
states are part of the checked experience rather than illustrative frames.

Out: DMs and realtime delivery, camera capture and filters, Story authoring,
audio selection, notifications, production auth (Play only switches the dev
actor), i18n, rich text, and recommendation ranking.

### Why not Svelte itself (recorded once)

Svelte's templates and handlers are arbitrary JS: nothing is enumerable,
provable, replayable, or headless — rendering *is* its semantics. Building
on `svelte/compiler` would define Uhura by **subtraction** over a JS-shaped,
non-semver-stable AST (v4→v5 rewrote it), with semantics-by-implementation
and no Rust/wasm core. What Svelte *does* contribute is its authoring
surface — single-file components, `{#each}`/`{#if}` blocks, moustache
expressions, `on:` bindings, co-located `<style>` — and Uhura adopts that
surface while owning the grammar. The three properties no existing framework
provides, and which are the entire reason Uhura exists: total no-JS
semantics, a deterministic headless step with traces, and typed ports.

---

## 2. Architecture overview

```
 .uhura source ──► uhura-check ──► checked IR (versioned canonical JSON)
 (+ CSS/examples)      │                 │
                       │       ┌─────────┴──────────┐
              diagnostics      ▼                    ▼
                              core             example resolver
                       step-u / eval-view      + pure replay
                              │                    │
                              │                    ▼
                              │          uhura-editor-model
                              │          immutable EditorState
                              │                    │
                              └─────────┐  native host  ◄── saved-file watch
                                        │   HTTP + SSE
                                        ▼       ▼
                              one browser application
                              `/` Editor · `/play` Play
                                        │
                              shared semantic renderer
                              Editor policy · Play policy
                                        ▲
                         provider envelope (JSON, the Spock seam)
                                        ▼
                    fixture driver (scripted ticks) ⇄ Spock adapter
```

Hard boundaries, each enforced mechanically (§12): native crates own
language/project truth and publish a complete versioned read model; the web
application owns all browser presentation; core interprets, V carries, and
the shared renderer realizes according to policy. The provider envelope is
the Spock seam. Core admits no ambient clock, randomness, floats, unordered
maps, network, or geometry.

### Reconciliation constitution — one owner per contract

| Contract | Owner section |
|---|---|
| Source syntax, file convention, expression/machine language | §4 |
| Example files and replay semantics | §6 |
| Step, dispatch, identity, state semantics | §7 |
| V JSON, descriptor shape, `EditorState`, renderer policies | §8 |
| Port TOML schema, provider envelope, fixture format | §9 |
| Semantic element catalog + extension rules | §10 |
| Content: screens, port inventory, fixtures, cast | §11 |
| Crate layout, native host, wasm ABI, CLI, diagnostics envelope, CI | §12 |

---

## 3. File convention

**Path defines; `use` references.** Element names resolve against the pinned
catalog; route names against the closed route table derived from `app/`.

```
examples/instagram/client/
  uhura.toml                          # manifest: entry route, catalog pin,
                                      #   play profiles (fixture + script)
  app/
    feed/page.uhura                   # route "feed"    → /feed
    feed/page.examples.uhura
    profile/[user]/page.uhura         # route "profile" → /profile/:user, param user: id
    profile/[user]/page.examples.uhura
  components/                         # pure templates: props in, declared emits out
    post-card.uhura                   #   (+ .examples.uhura siblings)
    stories-tray.uhura  comment-row.uhura  profile-header.uhura
    bottom-nav.uhura    notice-bar.uhura
  surfaces/
    comments-sheet.uhura              # + comments-sheet.examples.uhura
  ports/
    feed.port.toml  comments.port.toml  profile.port.toml
  fixtures/
    standard.toml                     # named typed data slices (fixture.feed.page-1, …)
    scripts/                          # canonical script list (§11.4):
      demo.toml  like-ok.toml  like-refused.toml  comment-ok.toml
      paginate.toml  feed-failed.toml  feed-empty.toml
    assets/                           # local fixture media + manifest.toml (id → file, alt)
  styles/
    theme.css                         # design tokens as custom properties + app styles
  catalog/base.toml                   # semantic element catalog DATA (pinned)
```

Rules: `app/**/page.uhura` defines a route (folder path = route, `[seg]` =
dynamic segment, page declares `param <seg>: <type>`). One definition per
file; header matches basename; components/surfaces corpus-public; pages
never importable; import graph is a DAG; shadowing forbidden.
`*.examples.uhura` siblings are design artifacts excluded from the runtime
bundle (checked-IR bytes are identical with and without them).

---

## 4. Source language

A `.uhura` file has up to four parts, in order: **header** (kind + `use` +
`props`/`emits`/`param` declarations), **`store { }`** (pages/surfaces only:
state + handlers — "the model + controller"), **markup** (the view), and an
optional **`<style>`** block of real CSS. Kebab-case names and UTF-8 apply;
comments, declaration docs, and markup annotations follow accepted
[RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md) and the
[living specification](../spec/README.md#13-source-comments-documentation-and-markup-annotations).
One canonical formatter, zero options. Bounds: file ≤ 256 KiB, nesting ≤ 32,
≤ 512 nodes/view, ≤ 128 handlers/page.

### 4.1 Header

```
component post-card                    // or: page  |  surface comments-sheet [modality sheet]

use component comment-row              // explicit, item-exact imports
use surface comments-sheet
use port feed { projection feed-page, command like-post, type post-summary }

props { post: post-summary, liked: bool }      // components/surfaces
emits { like-toggled(post: id, now-liked: bool) }
param user: id                                  // pages with dynamic segments
```

Port imports bring items into file scope unqualified; `type` is a legal
import kind; an imported command makes `send` legal and generates two
handleable events, `<command>.ok` / `<command>.err`.

### 4.2 The `store` block — statements closed at five

`state { field: type = literal, … }` plus `on` handlers.

| Statement | Semantics |
|---|---|
| `set <field>[key]? = <expr>` | Write to own-scope state. `= none` removes a map entry / optional. Record literal `{…}` legal on rhs. Paths: `field` or `field[key]` only. |
| `send <command>(args) [as <t>]` | Mints a core tag (`U.counters`), emits a command envelope, records pending. `as t` binds the tag (type `tag`) for keying optimistic state. Duplicate identical in-flight send → warning; suppression is the author's guard's job. |
| `open-surface <name>(args)` | Structural, applied at dispatch end. Idempotent per (definition, canonical context). Records opener + triggering node for focus restore. |
| `dismiss` | Surface scope only. Pops the instance; `FocusRestore` intent when topmost. No result plumbing (deferred). |
| `navigate <route>(named-args)` / `navigate replace <route>(named-args)` / `navigate back` | Structural; ≤ 1/step. Closed route table; args cover dynamic segments exactly. Plain navigation pushes. `replace` swaps only the top entry for a freshly initialized page and force-closes the replaced page's surfaces. `back` pops; the revealed page keeps state and the popped page's surfaces force-close. |

No `emit` statement, no internal events, no lifecycle events — boot data is
provider-seeded (§9), so every step dispatches exactly one external event.

**Handlers.** `on <event>(params) [when guard] { statements }`. Multiple
handlers per event: identical signatures, source order, first satisfied
guard wins, none ⇒ dropped with a trace note; unguarded-above-guarded is
`unreachable-handler`. Outcome handlers have fixed name-only signatures:
`on <command>.ok(tag, cmd)` / `on <command>.err(tag, cmd, refusal)` — `cmd`
is the echoed payload (rollback needs no bookkeeping); `unavailable` routes
to `.err`.

**Handler dispatch is transactional.** Writes are visible sequentially
*within* the handler but commit only if the handler completes. A not-ready
projection read anywhere in guard or body **aborts the dispatch**: guard
position ⇒ guard is false; body position ⇒ no writes commit, no commands
emit, trace note `projection-not-ready`. (Guards should still cover
readiness explicitly; the abort is the deterministic backstop.)

**Carried fields:** for catalog events declaring `carries`
(`textfield` `change { value: text }`), the emitted payload is author args
∪ carried fields; the author may not name a carried field; the handler
signature must include it.

### 4.3 Expressions — total, tiny, closed

Types: `bool`, `int` (i64, saturating), `text`, `id`, `tag`, port
records/unions/enums, catalog token enums (icon names and similar closed
sets typecheck as enum values), `list[T]`, `map[K]V` (K ∈ {id, tag}), `T?`.
**No floats.** Operators: `.field`, `x[k]` (option-returning), `??`, `!`,
unary `-`, `+ -`, `++` (text), comparisons, `&& ||`,
`if c then a else b`; builtins `to-text`, `count`. No `* / %`, user
functions, loops, recursion. State initializers are literals only. `for`
over a map iterates keys in deterministic order.

### 4.4 Markup

Svelte-flavored, closed:

- **Elements** — semantic catalog names (§10) or imported components:
  `<button …>`, `<post-card …/>`. Attributes: `attr="literal"`,
  `attr={expr}`, bare `attr` (= true). `class` is an ordinary attribute.
- **Text** — literal text and `{expr}` interpolation inside `<text>` (and
  only there — human-readable content is always typed data).
- **Blocks** — `{#if e}…{:else}…{/if}`;
  `{#each list as x (key-expr)}…{/each}` (the parenthesized key is
  **required** — Svelte's own keyed-each syntax, made mandatory);
  `{#match e}{:when variant [binding]}…{:else}…{/match}` for port unions
  and projection availability.
- **Widget events** — `on:<event>={emit <machine-event>(named-args)}`.
  Legal only where the catalog declares the event for that element (event
  eligibility); `on:` never attaches to `view` or other layout-class nodes.
- **Component emits (one model, explicit)** — a component's declared emits
  are consumed at the call site:
  `on:dismissed={emit notice-dismissed()}` rebinds with args;
  bare `on:like-toggled` **forwards** the event, same name and payload, to
  the enclosing machine scope. An unbound emit is the `unhandled-event`
  warning. There is no implicit propagation.
- A component's markup has **exactly one root element**; expansion is
  transparent (the root receives the call-site key; a `{#match}` root keeps
  the call-site key across arms, widget change ⇒ renderer replacement).

### 4.5 Styling — real CSS, checked shallowly

Per-file `<style>` blocks and `styles/theme.css` are **actual CSS**, passed
through to both browser surfaces. Design tokens are custom properties in
`theme.css`. The checker's whole CSS surface is: (1) selectors in a
component/page `<style>` must be class-rooted, with a lint recommending the
subject's root class (`.post-card`, `.feed-page`); (2) a class referenced in
markup that appears in no style source is a warning; (3) everything inside
declarations is passed through verbatim. Aesthetics are *free to drift* by
doctrine — the contract covers semantics, not spacing. Scoped-CSS
transformation (Svelte-style hashing) is deferred; rooting-by-convention is
the spike policy.

### 4.6 Normative source — `components/post-card.uhura`

```
component post-card

use port feed { type post-summary }

props {
  post: post-summary
  liked: bool
  like-pending: bool
}

emits {
  like-toggled(post: id, now-liked: bool)
  comments-requested(post: id)
  author-tapped(user: id)
}

<view class="post-card">
  <region label="View profile"
      on:activate={emit author-tapped(user: post.author.id)}>
    <view class="author-row">
      <img class="avatar" src={post.author.avatar.src} alt={post.author.avatar.alt} />
      <text class="username">{post.author.username}</text>
    </view>
  </region>

  {#match post.media}
    {:when image m}
      <region label="Like this post" supplementary
          on:activate-double={emit like-toggled(post: post.id, now-liked: true)}>
        <img class="media" src={m.image.src} alt={m.image.alt} />
      </region>
    {:when carousel c}
      <region label="Like this post" supplementary
          on:activate-double={emit like-toggled(post: post.id, now-liked: true)}>
        <pager class="media" indicator="dots" label="Photo carousel">
          {#each c.slides as s (s.id)}
            <img src={s.src} alt={s.alt} />
          {/each}
        </pager>
      </region>
    {:when video v}
      <view class="media video-fallback">
        <img src={v.poster.src} alt={v.poster.alt} />
        <view class="video-fallback-badge">
          <icon name="video-off" />
          <text>Video isn't supported in this preview</text>
        </view>
      </view>
  {/match}

  <view class="action-row">
    <button pressed={liked} busy={like-pending}
        label={if liked then "Unlike" else "Like"}
        on:press={emit like-toggled(post: post.id, now-liked: !liked)}>
      <icon name="heart" />
    </button>
    <button label="Comments"
        on:press={emit comments-requested(post: post.id)}>
      <icon name="message-circle" />
    </button>
  </view>

  <text class="likes">{to-text(post.like-count
      + (if liked && !post.viewer-has-liked then 1
         else if !liked && post.viewer-has-liked then 0 - 1
         else 0)) ++ " likes"}</text>
  <text class="caption">{post.caption}</text>
  <text class="meta">{to-text(post.comment-count) ++ " comments · " ++ post.posted-label}</text>
</view>

<style>
  .post-card { display: flex; flex-direction: column; gap: var(--space-2); }
  .post-card .author-row { display: flex; align-items: center;
                           gap: var(--space-2); padding: 0 var(--space-4); }
  .post-card .avatar { inline-size: 32px; block-size: 32px;
                       border-radius: var(--radius-full); object-fit: cover; }
  .post-card .media { aspect-ratio: 1; inline-size: 100%; object-fit: cover; }
  .post-card .video-fallback { display: grid; }
  .post-card .video-fallback > * { grid-area: 1 / 1; }
  .post-card .video-fallback-badge { place-self: center; display: flex;
      flex-direction: column; align-items: center; gap: var(--space-1);
      color: var(--color-on-media); }
  .post-card .action-row { display: flex; gap: var(--space-4);
                           padding: 0 var(--space-4); }
  .post-card .likes { font-weight: 600; padding: 0 var(--space-4); }
  .post-card .caption { padding: 0 var(--space-4);
      display: -webkit-box; -webkit-line-clamp: 3;
      -webkit-box-orient: vertical; overflow: hidden; }
  .post-card .meta { color: var(--color-ink-subtle); padding: 0 var(--space-4); }
</style>
```

Note what moved to CSS: the entire former layout/style taxonomy (`column`,
`row`, `gap=sm`, `ratio=square`, `shape=circle`, `lines=3`, button `kind`).
The optimistic like-count is computed in the view from `liked` vs projection
truth — the count moves the instant the overlay does.

### 4.7 Normative source — `app/feed/page.uhura`

```
page

use component bottom-nav
use component post-card
use component stories-tray
use component notice-bar
use surface comments-sheet
use port feed {
  projection feed-page, projection viewer,
  command like-post, command unlike-post,
  command load-next-page, command reload
}

store {
  state {
    like-overlay: map[id]bool = {}
    like-pending: map[id]bool = {}
    load-pending: bool = false
    load-failed: bool = false
    reload-pending: bool = false
    notice: text? = none
  }

  // like / unlike: optimistic overlay; guard-ordered multi-handler dispatch
  on like-toggled(post: id, now-liked: bool)
      when now-liked && !(like-pending[post] ?? false) {
    set like-overlay[post] = true
    set like-pending[post] = true
    send like-post(post: post)
  }
  on like-toggled(post: id, now-liked: bool)
      when !now-liked && !(like-pending[post] ?? false) {
    set like-overlay[post] = false
    set like-pending[post] = true
    send unlike-post(post: post)
  }
  on like-post.ok(tag, cmd)  { set like-pending[cmd.post] = none
                               set like-overlay[cmd.post] = none }
  on like-post.err(tag, cmd, refusal) {
    set like-pending[cmd.post] = none
    set like-overlay[cmd.post] = none
    set notice = "Couldn't like this post. Try again."
  }
  on unlike-post.ok(tag, cmd) { set like-pending[cmd.post] = none
                                set like-overlay[cmd.post] = none }
  on unlike-post.err(tag, cmd, refusal) {
    set like-pending[cmd.post] = none
    set like-overlay[cmd.post] = none
    set notice = "Couldn't unlike this post."
  }

  // pagination: the guard IS the dedupe; exhausted derives from truth
  on feed-near-end()
      when !load-pending && !load-failed
        && feed-page.has-more && feed-page.cursor != none {
    set load-pending = true
    send load-next-page(cursor: feed-page.cursor)
  }
  on load-next-page.ok(tag, cmd)           { set load-pending = false }
  on load-next-page.err(tag, cmd, refusal) { set load-pending = false
                                             set load-failed = true }
  on retry-load-tapped()
      when load-failed && feed-page.cursor != none {
    set load-failed = false
    set load-pending = true
    send load-next-page(cursor: feed-page.cursor)
  }

  on retry-reload-tapped() when !reload-pending {
    set reload-pending = true
    send reload()
  }
  on reload.ok(tag, cmd)           { set reload-pending = false }
  on reload.err(tag, cmd, refusal) { set reload-pending = false }

  on comments-requested(post: id) { open-surface comments-sheet(post: post) }
  on author-tapped(user: id)      { navigate profile(user: user) }
  on tab-selected(section: text) when section == "profile" {
    navigate profile(user: viewer.id)          // viewer is a boot projection
  }
  on notice-dismissed() { set notice = none }
}

<view class="screen feed-page">
  {#if notice != none}
    <notice-bar text={notice ?? ""} on:dismissed={emit notice-dismissed()} />
  {/if}

  {#match feed-page}
    {:when loading}
      <view class="fill-center"><text class="muted">Loading your feed…</text></view>
    {:when failed reason}
      <view class="fill-center stack-sm">
        <text>Your feed didn't load.</text>
        <button label="Retry" busy={reload-pending}
            on:press={emit retry-reload-tapped()}>
          <text>Retry</text>
        </button>
      </view>
    {:when ready f}
      {#if count(f.posts) == 0}
        <view class="fill-center">
          <text class="muted">Follow people to fill your feed.</text>
        </view>
      {:else}
        <scroll class="feed-scroll" on:near-end={emit feed-near-end()}>
          <stories-tray stories={f.stories} />
          <view role="list" class="post-list">
            {#each f.posts as p (p.id)}
              <post-card post={p}
                  liked={like-overlay[p.id] ?? p.viewer-has-liked}
                  like-pending={like-pending[p.id] ?? false}
                  on:like-toggled on:comments-requested on:author-tapped />
            {/each}
          </view>
          {#if load-pending}
            <text class="muted row-center">Loading more…</text>
          {/if}
          {#if load-failed}
            <view class="row-center gap-sm">
              <text class="muted">Couldn't load more.</text>
              <button label="Retry" on:press={emit retry-load-tapped()}>
                <text>Retry</text>
              </button>
            </view>
          {/if}
          {#if !f.has-more}
            <text class="muted row-center pad-md">You're all caught up.</text>
          {/if}
        </scroll>
      {/if}
  {/match}

  <bottom-nav current="feed" on:tab-selected />
</view>

<style>
  .feed-page { display: flex; flex-direction: column; block-size: 100%; }
  .feed-scroll { flex: 1; min-block-size: 0; }
  .post-list { display: flex; flex-direction: column; gap: var(--space-6); }
  /* fill-center / row-center / stack-sm / gap-sm / pad-md / muted live in theme.css */
</style>
```

`surfaces/comments-sheet.uhura` follows the same shape: header
`surface comments-sheet modality sheet`, `props { post: id }`, store
`{ draft: text = "", pending-appends: map[tag]text = {}, notice: text? = none }`;
`send add-comment(post: post, body: draft) as t` keys the optimistic row;
`.err` restores `draft = cmd.body` from the echo; `on dismiss-requested()
{ dismiss }`. The composer:
`<textfield value={draft} label="Add a comment" on:change={emit
composer-changed()} on:submit={emit submit-requested()} />` (`change`
carries `value: text`); pending rows render via
`{#each pending-appends as t (t)}`.

### 4.8 What the checker rejects (highlights)

Unknown element/component (`<avatar>` → "the avatar pattern is `<img
class=…>` — see docs/widgets/patterns"); unkeyed `{#each}` (parse error — the key is
grammar, not lint); `on:` on `view` or any layout-class node ("wrap in
`<region>`"; never auto-repaired); undeclared component emit; unbound
required prop; unresolved bind with did-you-mean; duplicate keys; unreachable
handler; a view-position projection read outside `{#match}` availability;
a class-rooting violation in `<style>`; a markup-referenced class defined
nowhere (warning). Diagnostics are structured JSON (§12.4) with spans,
labels, notes, and safe mechanical fixes only.

---

## 5. (Merged into §14.)

---

## 6. Examples — example-defined design

Unchanged from v1 in substance; the term decisions, file convention, and
replay semantics survive the reframe untouched (examples address the
machine, not markup syntax).

**Terms:** an **example** is the authored artifact (pinned or derived
presentation state of a page/component/surface); a **preview** is one
rendered example frame on the Editor board. Rejected: *snapshot* (collides with
V), *scenario* (reserved for cross-artifact flows), *story* (borrowed), *variant*
(collides with props).

### 6.1 Files and grammar

`<basename>.examples.uhura` siblings; excluded from the runtime bundle;
**checked-IR bytes identical with and without them**; spec files can never
reference example declarations. (Excerpt — the full feed set is §11.3.)

```
use fixture standard

example loading {
  note "cold start — nothing delivered yet"
  // boot projections (viewer) are auto-bound from the fixture by the resolver
}

example first-page default {
  projection feed.feed-page = fixture.feed.page-1
}

example like-pending {
  from first-page
  events [ like-toggled(post: "post-lena-glaze", now-liked: true) ]
  note "optimistic heart + count while like-post is in flight"
}

example like-refused {
  from like-pending
  events [ outcome like-post.err(reason: "network unavailable") ]
  note "transport unavailable — rollback, notice explains"
}

example comments-open {
  from first-page
  projection comments.for-post("post-lena-glaze") = fixture.comments.lena-glaze
  events [ comments-requested(post: "post-lena-glaze") ]
  note "the sheet mounts because the machine mounted it"
}

example appended {
  from first-page
  events [
    feed-near-end()
    projection feed.feed-page = fixture.feed.pages-1-2
    outcome load-next-page.ok()
  ]
}
```

Components: `props { … }` bindings from fixture slices; `from` =
props-merge. Pages with dynamic segments: `params { user = "user-lena" }`.
Clause legality matrix (checker-enforced): components — `props/from/note/
default`; pages — `params/projection/state/from/events/note/default`;
surfaces — pages' set plus `props`, and a derived surface example may not
dismiss its own subject. `default`: at most one (error if two), zero → info
lint, cover falls back to first declared. Multiple namespaced `use fixture`
imports are legal. **Boot projections are auto-bound from the fixture** so
the bare-read permission (§9.2) holds during replay.

### 6.2 Semantics — pinned and derived

- **Pinned** — literal bindings. May express unreachable states; captions
  mark them `pinned`; pinned-ness propagates through `from` chains
  (state-pins taint; projection-pins do not — any projection value is a
  legitimate external input).
- **Derived** — `from` + `events`: replay is a left fold of pure `step-u`
  over the timeline (semantic events, projection updates, injected
  outcomes). Rules: an injected outcome must settle the **oldest unsettled
  matching command** in the replay prefix (uncorrelated injection is a check
  error — "pin this state instead"); leaving the subject route at any step
  is a check error; evaluation order = parent chain → pins → timeline fold;
  per-step `C`/`I` are recorded, and captions surface unsettled commands
  ("1 command in flight"); failures attribute to the first failing step,
  descendants report "blocked by ancestor"; no cycles/forward refs; depth
  capped.

Derived examples are **self-verifying** — change the machine and
`uhura check` fails them. The modal case falls out for free: `comments-open`
fires the real event; in Figma that connection is a wire that can drift,
here it *is* the transition. **Replay is a build/check phase** resolving
each example to a frozen `(route, U, X, surface stack)` snapshot. The
`uhura-editor-model` layer consumes only checked, resolved results and
evaluates their semantic preview content; it does not execute new
transitions. Entity-id payloads are linted against bound fixture slices.

### 6.3 Editor presentation

One row per page (390×844 device frames, declaration order, `default`
badged); components content-sized on a dotted backdrop; surfaces standalone
+ in-context under the deriving page example. Captions: `subject /
example-name`, provenance (`pinned` | `from X → events…`), in-flight
commands, `note`. No examples ⇒ page renders initial state + info lint;
component absent from the Editor board + same lint.

A resolved `from` relationship renders as a directed replay-provenance
connector in a reserved rail above that subject's frames. Its label summarizes
only the child example's directly authored semantic events, projection
updates, and outcomes; ancestor steps remain on their own edges. These edges
explain how checked previews were derived and are not a separate runtime state
graph. Rail lanes are allocated deterministically so connectors do not cross
through preview frames. Shared endpoints fan out across deterministic top-edge
ports instead of collapsing sibling edges onto one stem, and the orthogonal
route clears the measured bounds of every intervening frame. Selecting a
preview highlights its immediate parents and children.

Selecting a derived preview also exposes a checked workflow trace for those
same direct steps. Each record carries the authored event kind and payload,
the runtime definition and absolute handler selected, every consulted guard's
result, and only committed writes, commands, intents, structural operations,
and projection deliveries. Page traces reuse `StepTrace`; standalone surface
and component replay exposes the same dispatch record without inventing a
second transition engine. The Inspector is therefore an explanation of the
checked replay, not a best-effort reconstruction from rendered frames.

When a direct replay step emits `open-surface`, Editor matches that structural
effect's mounted instance key to the target snapshot's surface stack. The edge
is then labeled as opening a child surface, the target caption and Inspector
show its modality and definition, and the in-frame overlays receive explicit
back-to-front stack indices. For a derived preview, Editor follows the checked
`from` lineage and joins every still-mounted instance to its recorded opener
scope. The Inspector therefore renders `page → surface → surface` recursively
instead of flattening nested sheets, dialogs, or popovers. A standalone surface
example is not treated as the child of a page merely because its definition
name matches: only checked instance keys and opener scopes establish that
parent-child relationship.

---

## 7. Core semantics

Unchanged by the reframe. One pure Rust function over owned, ordered data.

### 7.1 Step and state

```rust
pub fn step_u(p: &CheckedProgram, u: UiState, x: &Projections, e: Event) -> StepResult;

pub struct StepResult {
    pub u: UiState,               // rev = old + 1, always
    pub v: ViewSnapshot,          // full snapshot; v.revision == u.rev
    pub c: Vec<CommandEnvelope>,
    pub i: Vec<IntentEnvelope>,
    pub g: Vec<Diagnostic>,
    pub t: StepTrace,             // one JSONL record; the conformance artifact
}

pub struct Projections {          // ABSENT until delivered; keyed instances
    pub snapshots: BTreeMap<(Ident, Option<Value>), ProjectionSnapshot>,
    pub failed:    BTreeMap<(Ident, Option<Value>), String>,
}
```

`UiState`: `rev`, `nav` stack (entries own page state), `surfaces`
(bottom→top; instance = minted serial + def + canonical context + opener +
restore-focus node + fields), core-internal `pending` command table
(correlation, payload echo, origin scope), and `counters` (all identity —
replay mints identical ids). `Value`: unit, bool, i64, text, id, tag, list,
record (`BTreeMap`), none/some — **no floats**; `HashMap` banned in anything
feeding `U/V/C/I/G/T`.

### 7.2 Events, acceptance, dispatch

```rust
pub enum Event {
    Ui { descriptor: Descriptor, data: Option<Value>, view_rev: Rev },
    Outcome { correlation: String, result: OutcomeResult, updates: Vec<ProjectionUpdate> },
    Projection { updates: Vec<ProjectionUpdate> },
    ProjectionFailed { reference: ProjectionRef, reason: String },
    Init { route: RouteId, params: BTreeMap<Ident, Value> },
}
pub enum OutcomeResult { Ok, Refused { refusal: Ident }, Unavailable { reason: String } }
```

`Ui` acceptance in order (first failure drops with a trace record): scope
alive → else `stale-scope`; top-surface modality → else `occluded`; emit
declared with matching payload → else `ineligible`. **Stale `view_rev` is
accepted** — descriptors are self-contained, guards are the backstop;
rejecting by revision would eat taps on recomputes.

`Outcome` dispatches to the pending entry's origin scope; unmounted origin ⇒
`stale-outcome` drop (pending removed, truth already lives in `X`).
Piggybacked `updates` apply (revision-checked) **before** the outcome
dispatches — clear-overlay-on-ok is flicker-free (§9.4).

One external event per step; multi-handler guard selection runs at most one
handler; the handler is **transactional** (§4.2); structural statements
apply at dispatch end; `V` evaluates once from final `U'` and `X`. No
internal queue ⇒ termination by construction.

### 7.3 Optimistic overlays

An overlay is an **authored state diff over projection truth**; the view is
a pure function of (overlay, base); settlement and rollback are both diff
deletion; rebase is free recomputation. Core contributes exactly-once,
origin-addressed, payload-echoing outcome delivery — never hidden policy.

### 7.4 Navigation and history

`nav` is core state; the host's history moves only via intents. `navigate`
pushes + `HistoryPush`; `navigate replace` keeps the stack depth but replaces
the top with a freshly initialized page + `HistoryReplace`; `back` pops
(destroys popped page state, force-closes its surfaces) + `HistoryBack`.
Replace also force-closes the outgoing page's surfaces; pages revealed by
`back` keep state. This gives redirect-like flows (for example, leaving a
completed entry flow) a first-class operation without encoding a push/back
trick in application code. **The spike shell mirrors these intents for page
instance identity but does not mutate browser URL/history or synthesize
`LocationChanged`** — physical history reconciliation remains deferred RFC
material; the contract stays visible in `T`.

### 7.5 Determinism and trace

Forbidden inputs: clocks, randomness, unordered iteration, floats,
locale/env, network/storage/URL/clipboard, renderer geometry (only
catalog-declared semantic observations enter, as events), scheduling,
pointer identity. Canonical JSON (sorted keys, integers only) + SHA-256.
`StepTrace` (JSONL: input event, dispatch record with per-handler guard
results and writes, structural ops, `C`/`I`, drops, diagnostics,
`u-hash`/`v-hash`) is the golden format; `uhura trace --expanded` renders
full V per step as presentation.

---

## 8. The semantic view protocol and shared browser renderer

### 8.1 Snapshot (`uhura-view/0`)

```text
Snapshot        := { protocol: "uhura-view/0", revision: u64,
                     page: { route: text, root: Node },
                     surfaces: SurfaceInstance[] }          // bottom → top
SurfaceInstance := { key: text,                             // "comments-sheet:2" (def:serial)
                     definition: text, modality: "sheet",
                     restore-focus?: KeyPath,
                     dismiss: Descriptor,                   // first-class; Escape/scrim wire here
                     root: Node }
Node            := { key: Key, element: text,               // catalog name
                     class?: text,                          // authored CSS classes, passed through
                     props: { [name]: Value },              // SEMANTIC props only
                     children?: Node[], on?: Descriptor[], debug?: {…} }
Value           := bool | i64 | text
                 | { t: "plain", v: text }                  // inert human text — never markup
                 | { t: "image", asset: text }              // opaque asset id; core never fetches
Descriptor      := { kind: "input" | "observe",
                     event: text, emit: text,
                     scope: text,                           // "page:1" | "surface:2" — minted serials
                     payload: InertJson,                    // PREBUILT by core
                     carries?: { [field]: "text" } }
```

Full snapshots only; revision +1 per step; apply-in-order/drop-stale.
Payloads are prebuilt, never templates (a template hands the renderer a data
context — rebuilding the interpreter). **Descriptor presence is the
subscription** (feed exhausts ⇒ `near-end` descriptor disappears ⇒ renderer
stops observing). `class` is an opaque string V carries verbatim — styling
is deliberately not part of the semantic contract (adjudication #21).

Keys: sibling-unique; key-path is global identity; static nodes take source
ordinals, `{#each}` items take `"<ordinal>.<key-value>"` — collisions
unrepresentable, appends identity-preserving by construction. Component
expansion is transparent; one-root rule per §4.4.

### 8.2 Renderer contract

MUST: realize every supported element (honest labeled placeholder
otherwise); preserve identity by key-path; insert `{t:"plain"}` via text
nodes only; emit events only from present descriptors, echoing `payload`
verbatim + declared `carries`; apply snapshots atomically in revision order;
preserve focus across applies; meet the a11y minimums (button
`aria-pressed`/`aria-busy`, img `alt` xor bare `decorative`, surface
`role=dialog aria-modal` + page `inert` + focus restore, region keyboard
synthesis; DOM order = V order; no positive tabindex); apply authored CSS
as-is.

MUST NOT: read source/IR/fixtures/projections; hold semantic state;
synthesize events or attach them to ineligible elements; disable a `busy`
control on its own initiative; reinterpret older snapshots.

Near-end contract: descriptor presence = someone listens (core); physical
proximity = remaining extent below **100% of one viewport extent** (integer
percentage, stated once in the catalog); edge-triggered latch re-armed on
content-extent growth (renderer); exactly-one in-flight command (core
guard); cursor meaning (port). The four-way split that discharges the
stress-test corpus's `ui:list` P0 — semantics (`role="list"` + keyed each),
viewport (`scroll`), windowing (renderer license), pagination observation
(split as above).

### 8.3 Model-driven Editor — `uhura editor`

The native side captures one coherent saved-file revision, checks it, resolves
examples, evaluates semantic page snapshots/component fragments, and asks
`uhura-editor-model` to serialize one immutable `uhura-editor-state/2`
document. The document contains source and render revisions, current
diagnostics, application metadata, stable preview groups and identities,
semantic content, example values and provenance, interaction summaries, the
compiled application stylesheet, and an asset table. It
contains no prepared DOM, Editor layout, selectors, or browser behavior.

Revision identity is explicit. A renderable candidate publishes matching
source/render revisions with `freshness: current`. A broken saved revision
publishes its current diagnostics with the last renderable payload marked
`freshness: stale`; an initially broken project has `render: null`. Thus the
browser never presents diagnostics and old preview content as though they came
from one revision. The native host exposes the complete state at
`/api/editor/state`; `/api/editor/events` announces revision changes over SSE.
Events carry ordering, not fragments: the browser refetches, decodes, realizes,
and atomically replaces one whole state.

The `/` route owns the complete read-only Editor UI in TypeScript: navigator,
search, frames, selection, inspector, toolbar, camera, pan, zoom, Cursor and
Hand tools. The shared renderer's Editor policy realizes semantic nodes
one-shot inside inert preview hosts, ignores runtime descriptors, performs no
provider/scroll/textfield effects, and treats source-less video as poster-only.
Controls still have honest platform and accessibility structure, while the
inspector presents what an interaction would emit. Example values and their
authored origins remain visible but immutable; see the
[read-only provenance design note](referential-example-data-and-read-only-provenance.md).
Application styles and Editor chrome have separate ownership so app CSS does
not become chrome CSS. Assets use the state's data-URI table. Semantic icon
family/name tokens remain in preview content; a separate revision-matched
renderer-resource manifest supplies checked codepoints and content-addressed
WOFF2 bytes. Neither glyph data nor font identifiers enter EditorState or Core
protocols.

Wheel/trackpad deltas pan; `H` or held Space enables drag-panning; pinch zooms
around its midpoint; Cursor drag reserves an intentionally inert marquee.
`Cmd+\` / `Ctrl+\` hides the UI, and Play is entered from the inspector at
`/play`. Bare `uhura` defaults to `uhura editor`. Because the web application
stays mounted during state replacement, camera, tools, search, chrome
visibility, and semantic selection survive valid → invalid → valid edits as
ordinary browser state. No document replacement or reload-survival storage is
involved. RFC 0002 defines the saved-change and last-renderable lifecycle.

### 8.4 Play policy and the unified TypeScript host

`uhura play` enters `/play` in the same framework-free TypeScript application
used by the Editor. The native host serves the same entry document for `/` and
`/play`, plus namespaced Play artifacts and provider endpoints. The Play mount
boots the wasm `Session`, checked IR, fixtures or configured provider, and
compiled stylesheet. The semantic DOM mechanics live under the shared
renderer; the Play policy adds reconciliation, descriptors, focus, scroll,
textfield, surface, media, and runtime-delivery effects. This keeps the V
protocol and its mechanics independent of any future framework choice.

`web/src/` and its Vite configuration are authoritative. One application build
emits hashed ESM/CSS into generated, ignored `web/dist/`; generated provider
bundles are ignored as well. CI runs the frontend typecheck, lint, production
build, and tests before the Rust checks. Release packaging builds the web app,
Wasm, and release CLI, then installs the generated assets beside the native
executable under `share/uhura`; Node and pnpm are build-time dependencies only.
During development Vite serves the same app and proxies `/api` to the native
host. Production uses no Node server: the native process serves the compiled
application unchanged.

The running prototype sits in host-owned chrome over a black stage. The host
offers Mobile (390 × 844) and Desktop (1280 × 800) visual frames, a full UI
session restart, provider selection, readiness, and provider-authored actor
selection. None of these values enter Uhura state or author-visible events.
Actor/provider changes and Restart perform a full navigation so the old
session, ticks, browser capabilities, signed-media cache, and in-flight
provider work retire together. Restart does **not** reset Spock authority
truth. The frame choice is persistent browser-local Play-chrome state;
provider and actor choices are tab-local session state. Host controls neither
read nor rewrite the running Uhura program's query parameters. The frame switch
is deliberately labeled as visual framing: because the v0 app is not isolated
in an iframe, browser media queries and viewport units still observe the host
window. True device emulation is deferred.

- **Reconciler:** keyed `insertBefore`-sweep (~350 lines): element change ⇒
  replace; semantic-prop appliers + class swap; keyed child reconciliation
  with zero-move appends. Route change remounts the page subtree; a
  per-route scroll cache restores position on back.
- **Reentrancy (normative):** renderer emissions always **enqueue**; a
  `pumping` flag makes nested pumps no-ops; post-apply observation checks
  run in a microtask after the pump drains (prevents the wasm `RefCell`
  re-entry panic).
- **textfield (normative):** core owns the draft; renderer owns caret/IME.
  Per field, a counter of in-flight change emissions; while nonzero,
  external replacement never applies (stashed) — a tick-scheduled outcome
  landing mid-typing cannot eat keystrokes. IME composition buffers locally,
  one `change` at `compositionend`.
- **Ticks:** the fixture driver schedules outcomes in integer ticks
  (`after-ticks ≥ 1`, so optimistic states are always observable); the shell
  maps wall time to `driver.tick()`; traces depend only on tick ordinals.

### 8.5 Theme

`styles/theme.css` owns the tokens (4px space scale as `--space-*`, type
scale, `--radius-*`, neutral ink ramp, one accent `#ed4956` used only where
it means something) and the shared utility classes the slice uses
(`fill-center`, `row-center`, `muted`, …). Good-looking is achieved by an
**authored** theme rather than a generated class system — consistent with
"aesthetics are free to drift"; the checker never validates a declaration.

---

## 9. Ports, the provider envelope, and the fixture driver

Unchanged by the reframe (the seam never touched markup).

### 9.1 Contract schema — `ports/*.port.toml`

Kind-tagged type tables; closed type grammar (`bool | int | text |
option<T> | list<T> | <declared>`); kinds `record`, `union` (closed,
exhaustively matchable), `enum`, `id`, `opaque` (cursors — echoable, never
inspectable), `asset`. Projections singleton or keyed (`key = "<id-type>"`).
Commands declare a payload record and declared refusals; every outcome
union is implicitly extended by `unavailable { reason: text }`. **Ok
payloads are empty for every spike command** — settlement data travels as
projection updates; no fact has two carriers. Refusals carry no fields in
the spike (`Refused { refusal }`).

```toml
# ports/feed.port.toml (abridged)
[port]  name = "feed"  version = "0.1.0"

[types.image-ref]    kind = "record"
[types.image-ref.fields]  src = "asset"  alt = "text"

[types.user-ref]     kind = "record"
[types.user-ref.fields]
id = "id"  username = "text"  display-name = "text"  avatar = "image-ref"

[types.slide]        kind = "record"
[types.slide.fields]  id = "id"  src = "asset"  alt = "text"

[types.story-ring]   kind = "record"
[types.story-ring.fields]
id = "id"  user = "user-ref"  has-unseen = "bool"  is-self = "bool"

[types.media]        kind = "union"
[types.media.variants.image]     image  = "image-ref"
[types.media.variants.carousel]  slides = "list<slide>"
[types.media.variants.video]     poster = "image-ref"

[types.post-summary] kind = "record"
[types.post-summary.fields]
id = "id"  author = "user-ref"  media = "media"  caption = "text"
like-count = "int"  comment-count = "int"  viewer-has-liked = "bool"
posted-label = "text"        # provider-formatted; core has no clock

[types.feed-cursor]  kind = "opaque"
[types.feed-page]    kind = "record"
[types.feed-page.fields]
stories = "list<story-ring>"  posts = "list<post-summary>"
cursor = "option<feed-cursor>"  has-more = "bool"

[projections.viewer]     type = "user-ref"   boot = true   # delivered before Init
[projections.feed-page]  type = "feed-page"

[refusals.not-authorized]  [refusals.not-found]

[commands.like-post]       payload = { post = "id" }
refusals = ["not-authorized", "not-found"]
[commands.unlike-post]     payload = { post = "id" }
refusals = ["not-authorized"]
[commands.load-next-page]  payload = { cursor = "option<feed-cursor>" }
[commands.reload]          payload = {}
```

`comments.port.toml`: keyed projection `for-post` + `add-comment`;
`profile.port.toml`: keyed projection `profile`. **No `loading`/`failed`
variants exist in any projection type** — a Spock read model cannot honestly
export non-delivery or transport failure as data (adjudication #1).
Versioning: canonical-form hash pinned in `uhura.lock`; drift is a link
error, never silent.

### 9.2 Availability — session truth, not contract truth

Projections are **absent until delivered**. View-position reads must sit
inside an availability `{#match}` (`loading | failed reason | ready v` —
language-level arms, not contract types). Keyed reads: `for-post(post)`,
`profile(user)`. Guard/body reads short-circuit per §4.2's transactional
rule. `boot = true` projections (viewer) are delivered before `Init`; bare
reads of boot projections are legal; the examples resolver auto-binds them.
No subscription-interest machinery in the spike — the fixture delivers
authored instances eagerly on scripted ticks (interest derivation is a
documented future refinement; the sheet's loading arm is exercised by
examples and a delayed-delivery trace script).

### 9.3 The provider envelope (`uhura-provider/0`)

```json
{ "kind": "command", "port": "feed", "command": "like-post",
  "correlation": "c-4", "payload": { "post": "post-lena-glaze" } }

{ "kind": "projection", "port": "feed", "projection": "feed-page",
  "key": null, "revision": 2, "value": { … } }
{ "kind": "projection-failed", "port": "feed", "projection": "feed-page",
  "key": null, "reason": "unreachable" }
{ "kind": "outcome", "correlation": "c-4",
  "outcome": { "ok": {} },
  "updates": [ { "port": "feed", "projection": "feed-page",
                 "key": null, "revision": 3, "value": { … } } ] }
```

Rules: exactly one outcome per command, eventually (adapters map transport
failure to `unavailable`; core has no timeouts); one ordered stream, never
re-entrant; projection revisions strictly increase per (projection, key),
stale drops diagnosed; correlation ids are core-minted, opaque, echoed
verbatim — author-level anything never crosses the wire.

### 9.4 Settlement — enforceable, not fixture-lore

The projection consequence of an accepted command arrives **either as an
earlier standalone update or piggybacked on the outcome's `updates`**,
applied atomically before the outcome dispatches. A real adapter satisfies
this by carrying the command's resulting read-model fragment in the command
response (ordinary CQRS) or holding the outcome until its subscription
catches up — checkable per outcome. Clear-overlay-on-`.ok` is flicker-free
by construction; the conformance suite asserts it against any provider.

### 9.5 The fixture driver

Data slices in `fixtures/standard.toml` (named, typed; L8-validated against
contracts at link time — an ill-typed fixture is a link error). Scripts in
`fixtures/scripts/*.toml`: entries match `on = { command, where }` in file
order among unconsumed entries, one-shot unless `repeat = true`;
`after-ticks ≥ 1`; `on-unscripted = "error"` (a duplicate in-flight
load-next **is** the dedupe assertion). Replies reference **whole authored
snapshot slices** — no set-op path language; the only substitutions are
`{ from = "payload.<field>" }` and `fresh-id`. **One driver implementation**
(`uhura-fixture`, Rust), compiled to wasm for play and linked natively for
traces; the TS shell owns only the tick pump, and envelope JSON between
`Session` and `FixtureDriver` stays visible in the shell.

### 9.6 The Spock-replaceability argument

Provably unchanged when Spock replaces the fixture: port contracts (hash
pin), every `.uhura` file (no fixture vocabulary in the grammar —
grep-provable), checked IR and core (no provider-identity input;
`uhura-core`'s dependency closure excludes `uhura-fixture`, CI-enforced),
the envelope and its rules (conformance suite runs against any driver), V,
the shared renderer and both policies, the stylesheet, examples, and replay
determinism. What
changes: one `uhura.lock` binding line per port; the driver implementation
(ticks vanish; wall-clock lives only in the adapter); live delivery order
becomes nondeterministic (per-step determinism untouched; the fixture stays
the permanent CI test double); fixture-only semantics disappear.

---

## 10. The semantic element catalog

**Ten elements, three classes, checked by generic rules; layout and
aesthetics belong to CSS.** The catalog is data (`catalog/base.toml`,
versioned + hash-pinned); source cannot invent an element, prop, or event by
naming it; the checker validates the catalog against a meta-schema (input
events only on interactive elements; observation events only on viewports).

| Element | Class | Semantic props | Events |
|---|---|---|---|
| `view` | layout | `role(none\|list\|navigation\|tablist)` | — (never) |
| `scroll` | layout/viewport | `direction(vertical\|horizontal)` | `near-end` (observe) |
| `pager` | layout/viewport | `indicator(none\|dots)`, `label`; children from one keyed each; **uncontrolled** | (`page-change` when controlled — unused) |
| `text` | content | content = typed data / interpolation | — |
| `img` | content | `src` (asset ref), `alt` xor bare `decorative` | — |
| `video` | content | `src`, optional `poster`, `label`, `autoplay`, `muted`, `loop`, `controls`, `playsinline` | native media controls |
| `icon` | content | optional literal `family`, checked `name`; decorative | — |
| `button` | interactive | `label`, `disabled`, `busy`, `pressed?`, `current?`; content children, no interactive descendants | `press` |
| `textfield` | interactive | controlled `value`, `placeholder`, `label`, `disabled` | `change{value}`, `submit` |
| `region` | interactive | `label` (required), `supplementary`; one child, no interactive descendants | `activate`, `activate-double` |

Every element additionally takes `class` (opaque, CSS-owned). Icon names no
longer live in the semantic element catalog. They come from the selected,
locked icon-font family; the bundled default is Lucide. See
[`<icon>`](../widgets/elements/icon.md) and the
[icon-font integration](../widgets/integrations/icon-font.md).

**What is deliberately NOT an element:** `column/row/stack/grid/spacer`
(CSS layout on `view`), `card/avatar/tab-bar/app-bar/list` (documented
patterns in `docs/widgets/patterns/`, golden-checked), `sheet/dialog` (core surface
stack), any styling prop
(`gap/pad/ratio/shape/kind/size/color/lines` — all CSS now).

**Extension is first-class in design, deferred in exercise:** a catalog is
data, so a user/renderer pair may register additional elements with full
signatures (props, events, a11y contract, projection behavior); the checker
enforces them identically; a renderer must declare which catalogs it
implements. The spike ships only `base`; the mechanism is specified so
"users can define their own widgets" is true at both levels — components
(composition, today) and catalog elements (new primitives, with a renderer).

The checker rules survive unchanged in spirit: catalog authority, event
eligibility (`on:` only where declared — a `view` can never become
interactive), children models, **required keyed each**, no nested
interactives, controlled-state promotion (binding `value` obligates handling
`change`), a11y completeness (alt xor bare decorative; accessible names;
`role=list` requires one keyed each), viewport sanity. Style-prop closure is
**deleted** — replaced by the shallow CSS checks of §4.5.

**Double-tap-to-like** stays owned by `region` — the catalog-declared
accessible gesture owner (focusable, named, keyboard synthesis, AT-exposed
action). `supplementary` regions need a same-named emit reachable from a
focusable element in the same component (name-level check; payload equality
is a lint).

---

## 11. The Instagram slice

### 11.1 Inventory

Nine routed pages: Feed, Search, Create, Reels, profile, post detail,
story detail, and separate profile follower/following lists. One comments surface,
five live bottom-nav destinations, and a notice bar (persists until dismissed
— no timers exist). Profile and tagged tiles carry real post ids and open the
shared post-detail route; story rings, relationship counts/lists, tags, likes,
comments, and uploaded posts all derive from authority rows and reconcile
through typed provider commands.

### 11.2 The cast (no lorem ipsum)

Viewer: **Mira Santos** (`mira.santos`), food & travel photographer,
Lisbon. Mira's home feed is derived from her six follow edges: **Lena Holt**
(ceramicist — glaze tiles, 7 real likes, 4 comments), **Marco Reyes** (surfer
— 3-slide Baja carousel), **Priya Raman** (baker — "Day 400 of the starter.
She's earned a name: Clint Yeastwood."), Ayla Demir, June Park, and **Kenji
Tanaka** (pre-liked — proves projection-truth hearts without overlays). Reels
also exposes real stored video from Nils Bergman and Theo Okafor. Mira's demo
comment: *"Saving this palette for my kitchen reno — stunning work!"*
Counts are integers derived from relational rows; age labels are formatted
from authority timestamps. Local image posters and stored videos have
manifest/port-checked accessible names.

### 11.3 Example sets (Editor board)

Feed: loading, first page, optimistic/rollback states, comments, story
navigation, pagination, empty, exhausted, and failure. Profile: loading,
Lena, self, tabs, post navigation, and follower navigation. Dedicated rows
cover post detail, story detail, followers, following, Search, Reels, and
Create. Comments-sheet remains standalone. Components cover post-card media
variants, stories, connection rows, comments, all five bottom-nav states,
notice bar, and profile header.

### 11.4 Demo walkthrough (play mode) and CI scripts

1. Launch → loading → feed settles with stories and posts only from Mira and
   accounts she follows.
2. Like Lena's post → heart fills **and count reads 8** instantly (the
   count is computed from the overlay in post-card, §4.6), button busy; ok
   settles via piggybacked update; trace shows exactly one command.
3. A scripted unavailable like → optimistic beat → heart and count roll
   back; notice bar appears; **after dismissing the notice, the
   feed subtree is byte-identical to pre-like** (the scoped invariant).
4. Double-tap Priya's photo → same event via `region` (keyboard: focus,
   Enter).
5. Open comments on Lena's post → sheet mounts, feed inert, focus enters;
   4 authored comments.
6. Type Mira's comment, Post → dimmed "Posting…" row instantly; the
   authoritative comment replaces it atomically; post-card meta shows
   5 comments.
7. Close sheet → focus returns to the comment button (FocusRestore intent
   traced).
8. Scroll to bottom → one `load-next-page`; wiggle-scroll → no second
   command (guard).
9. Failure → retry → page 2 appends with zero scroll jump, keys preserved;
   Kenji pre-filled; end cap; zero further commands.
10. Tap `lena.holt` → profile (real stats and posts); open a grid tile into
    post detail; open Followers/Following and toggle a real edge.
11. Open a multi-frame story; move previous/next; Search accounts and caption
    text; play a stored MP4 in Reels; save it; return through the five live
    destinations without manufacturing a stack entry for each tab hop.
12. Choose an image, optionally author caption/alt text, upload through signed
    Spock storage, and publish it into both Feed and Mira's profile.

**Canonical trace scripts** (one list, used by §3, CI, and goldens):
`like-ok`, `like-refused`, `comment-ok`, `paginate`, `feed-failed`,
`feed-empty`, plus `demo` (play only).

### 11.5 Coverage

Every element, machine feature, and core mechanic is forced at least once;
the matrix lives next to the acceptance test. Three `scroll` uses (feed
with observation; stories tray horizontal without; comments without) prove
the list-concern split in both directions.

---

## 12. Toolchain and workspace

### 12.1 Crates

Cargo workspace under `uhura/`; pinned Rust toolchain; `edition = "2024"`,
`unsafe_code = "forbid"`, all `publish = false`. Language-only commands and
crates build without Node or frontend output. Browser surfaces require either a
development `web/dist/` build or packaged web assets and fail clearly when
neither exists. The separate `web/` pnpm package owns TypeScript authoring,
tests, and deterministic browser builds; Cargo never shells out to it.

```
uhura/crates/
  uhura-base       # foundation: Value/Ident + the canonical-JSON/SHA-256 choke
                   #   point, spans, SourceMap, diagnostics, UHnxxx registry
  uhura-syntax     # lexer + RD parsers (markup, store DSL, CSS-selector
                   #   tokenizer for the shallow style checks), AST, formatter
  uhura-port       # contract model + provider envelopes — the Spock seam crate
  uhura-check      # the front half: resolve, routes, catalog-as-data (module),
                   #   typecheck, ports L1–L8, style checks, examples replay, lower
  uhura-core       # checked IR (module `ir`), view protocol (module `view`),
                   #   step_u + eval_view; dep closure = {base, port}
  uhura-fixture    # scripted driver (native + wasm)
  uhura-editor-model # browser-neutral, deterministic EditorState from resolved
                     #   examples; semantic content/provenance/assets, no I/O/DOM
  uhura-wasm       # wasm-bindgen: Session + FixtureDriver, JSON-string ABI
  uhura-cli        # bin `uhura`: check | fmt | editor | play | trace (all I/O here)
  uhura-tests      # goldens, purity tests, acceptance integration test
```

Topology note (post-M1 simplification): earlier drafts listed 14 crates; the
implemented workspace is these **10**. Crate boundaries exist only where they
are load-bearing — mechanically enforced theses (core purity, the port seam,
the fixture outside core, I/O quarantined in the CLI) or technical necessity
(wasm cdylib). `value`+`diag` merged into `uhura-base`; `catalog` became a
module of `uhura-check`; `ir` and `view` became modules of `uhura-core`
(every consumer of those types already depends on core, and the purity
allowlist got simpler). `syntax` stays separate so checker edits never
recompile the parser and the formatter is reusable alone.

The Editor model boundary is also load-bearing: `uhura-editor-model` may
depend on checking/evaluation data and deterministic serialization, but owns no
filesystem capture, HTTP, HTML, browser CSS, or Editor chrome. The CLI owns
coherent capture, saved-file observation, current-candidate diagnostics,
last-renderable publication, and serving; the browser owns presentation and
interaction state.

Core purity is a failing test, not a convention: dependency-DAG allowlist
asserted via `cargo metadata` (`uhura-core` closure == `{base, port}`) plus a
`cargo tree` check that a core-only build never compiles `toml`; per-crate
`clippy.toml` disallowing `std::{fs,net,time,thread,env}` in pure crates;
CI `-D warnings`.

### 12.2 Parser and IR

Hand-rolled recursive descent for both the store DSL and the markup (exact
diagnostic codes, trivia-preserving formatter round-trip, recovery trees;
the closed grammar makes generators overhead). CSS handling is a selector
tokenizer only — declarations pass through verbatim. The checked IR **is
serialized** (versioned canonical JSON, hard version check) as a Play
artifact: `uhura play` checks natively and serves IR + compiled stylesheet,
so `.uhura`/CSS edits never trigger a wasm rebuild. The Editor browser does
not consume canonical IR; it consumes the purpose-built, versioned
`EditorState` read model.

### 12.3 Wasm ABI

```rust
#[wasm_bindgen] pub struct Session { … }      // new(ir_json) · boot(boot_json)
                                              //   · dispatch(event_json) → step-result JSON
                                              //   · view() · revision() · ir_version()
#[wasm_bindgen] pub struct FixtureDriver {…}  // new(fixture_json, script_json)
                                              //   · deliver(cmd_json) · tick() → event-json[] · idle()
```

JSON strings across the boundary; the shell wires the two by passing
envelope JSON — the seam stays visible. No timers/fetch/DOM inside wasm.

### 12.4 CLI and diagnostics

`uhura [path] [--port]` (default Editor) · `uhura check [--emit-ir]` ·
`uhura fmt [--check]` · `uhura editor [--port]` (explicit default spelling) ·
`uhura play [--port]` · `uhura trace --script [--expanded]`; `uhura dev`
remains a compatibility alias for Play. Exit codes 0/1/2; `--deny-warnings`
in CI. `editor` and `play` start the same native host and serve the same web
application; the selected browser route is `/` or `/play`. Editor state and
events live under `/api/editor/*`; Play artifacts, media, provider calls, and
events live under `/api/play/*`.

The host watches saved project files, captures each candidate coherently, and
atomically publishes current diagnostics plus either its current render or an
explicitly stale last-renderable render. The Editor fetches complete model
replacements without remounting the application. Editor publication never
restarts or migrates a Play session; state-preserving Play source updates
remain separate deferred work. The native host serves compiled application
files from the package/development asset location and never synthesizes browser
markup. One versioned diagnostics envelope (`uhura-diagnostics/0`: `code
UHnxxx` + `rule` slug, span, labels, notes, `fix{title, edits}`).

### 12.5 Tests

Unit tests per crate; goldens (`UPDATE_GOLDEN=1` blessing): fmt round-trip,
diagnostics, whole-app IR, resolved example → V per preview, canonical
`EditorState`, and the trace scripts (§11.4) in `StepTrace` hash form. Model
contract tests cover deterministic serialization, protocol rejection,
page/surface/component content, unique identities, and the current/stale/cold
revision invariants. Shared-renderer conformance tests feed the same semantic
nodes through both policies and prove that Editor is inert while Play retains
descriptors and effects. Host state-transition tests cover current → stale →
recovered publication and cold-invalid recovery. Browser update-session tests
cover revision ordering, retry, and atomic install; the running watcher/browser
lifecycle remains an acceptance scenario rather than an automated integration
test.

**The acceptance trace is one executable integration test** asserting golden
traces plus structural invariants: exactly one like command per press;
optimistic view precedes outcome; **post-refusal, after
`notice-dismissed()`, the feed subtree hash equals pre-like**; one in-flight
load-next; append preserves key order; dismiss emits FocusRestore; Editor
model construction introduces no extra commands or transitions; IR bytes are
identical with/without examples files. CI first runs the frontend typecheck,
lint, production build, and browser tests, then Rust fmt, clippy, unit/
integration tests, example fmt/check/trace, and the wasm-target build. No
generated frontend files or exported Editor document are diff-checked or
uploaded. Release/package verification runs `scripts/package.sh`, which builds
and installs the web app, Wasm, and native CLI together.

### 12.6 Milestones (each ends demoable)

| # | Builds | Demo after it |
|---|---|---|
| M0 | Workspace skeleton, toolchain pin, purity tests, CI green | the boundary exists before any feature |
| M1 | Lexer/parser (store DSL + markup + CSS selectors), formatter; `uhura fmt`, parse-only `check` | format the whole example; spanned diagnostic on a planted error |
| M2 | Full check: routes, types, catalog rules, ports L1–L8, style checks, IR emit | `check` clean on the slice; `on:press` on a `view` fails correctly |
| M3 | `eval_view` + stylesheet compile + **pinned** examples + browser-neutral Editor model + Editor route | **the good-looking Editor board** — pan/zoom every pinned preview |
| M4 | step_u (dispatch, guards, sends, overlays, surfaces), fixture driver, `uhura trace`, **derived-example replay** | headless like→optimistic→refusal→rollback as diffable golden JSON; derived previews join the Editor board |
| M5 | wasm Session + Driver, shared semantic renderer, unified Editor/Play application and native host | live Play prototype plus saved-file Editor replacement without remounting the app |
| M6 | Pagination, profile route, focus restore, full acceptance test | the complete walkthrough in CI and on screen |

(Derived examples fold `step_u`, so replay lands in M4, after the machine
exists; M3's Editor board is pinned examples only. The milestone numbering is
historical; the v3 topology describes the maintained end state.)

---

## 13. Acceptance criteria

1. `uhura check examples/instagram/client` is clean; each documented rejection
   produces its exact diagnostic.
2. Like emits exactly one typed command per press; the optimistic view
   (heart *and* computed count) precedes the outcome; refusal rolls the
   affected post-card subtree back to byte-equality with its pre-like
   render (asserted after notice dismissal); settlement via piggybacked
   update never flickers.
3. Comments: open mounts one keyed surface instance with typed input;
   optimistic append visible ≥ 1 tick; dismissal restores focus (intent
   asserted in trace).
4. Pagination: one `load-next-page` per near-end episode, duplicates
   guard-rejected and traced; failure → retry; append preserves existing
   keys in order; exhausted derives from projection truth.
5. Navigation: feed → profile → back retains feed page state; history
   intents are emitted and traced (and executed as no-ops).
6. `uhura-editor-model` deterministically publishes every resolved example in
   one valid `uhura-editor-state/2` render without HTML or I/O and without
   executing extra transitions (derivation remains a checked build step over
   pure `step-u`). The browser's Editor policy cannot dispatch runtime events.
   A broken current revision carries its own diagnostics and an explicitly
   stale prior render, or `render: null` before any valid revision; derived
   examples fail check when the machine no longer reaches them, and
   uncorrelated outcome injection is a check error.
7. Fixed inputs reproduce byte-identical traces and V hashes across native
   and wasm runs.
8. The checked IR is byte-identical with and without `*.examples.uhura`.

---

## 14. Deferred register

Language: component slots/children; shared layouts; import aliasing;
surface results to opener; match-in-expressions; local enum declarations
(tab sections use `text` guards); string builtins; floats; division;
scoped-CSS transformation (rooting-by-convention for the spike). Core:
command cancellation/timeouts; browser-history reconcile +
`LocationChanged`; projection-update handlers; Play-session migration across
source changes; pending garbage policy. View: patches; visibility observation;
annotated/rich text; controlled pager exercise; prepend/refresh anchoring;
capability negotiation; dark theme; aria-live conventions. Ports:
subset/superset provider satisfaction; contract evolution; subscription
interest sets; realtime `[[push]]` scripting; client-minted ids. Examples:
cross-page journey strips; `expect` assertions over V; board
curation. Toolchain: incremental CST; binary IR/ABI; release policy.
Catalog: third-party catalog exercise (mechanism specified, §10).

---

## 15. Adjudication log

### v1 — reconciling seven parallel designs (all upheld in v2 unless noted)

| # | Conflict | Resolution |
|---|---|---|
| 1 | `loading/failed` inside projection contract types vs derived availability vs epoch-0 seeding | Contracts carry only authoritative data; availability is session/protocol truth; boot projections declared in the contract |
| 2 | Settlement ordering only a fixture could honor | Piggybacked `updates` on the outcome envelope, applied before dispatch — checkable per outcome |
| 3 | Subscription-interest set vs un-keyed projections | Keyed projections; interest machinery cut; eager fixture delivery |
| 4 | Four correlation-tag models | Core-minted tags (`send … as t`), payload echo to `.ok(tag, cmd)` / `.err(tag, cmd, refusal)`; wire carries opaque `c-<n>` only |
| 5 | Handler multiplicity | Multi-handler, source order, first satisfied guard |
| 6 | Two "closed" statement sets; internal events | Five statements; no internal events at all |
| 7 | Widget vocabulary drift | v1: taxonomy catalog normative. **v2: superseded by #21–#23** |
| 8 | Controlled carousel demo vs uncontrolled pager | Uncontrolled; false demo beats deleted |
| 9 | Floats | None anywhere; near-end threshold restated as an integer percentage |
| 10 | Overlay shadowing projection truth after `.ok` | Overlays deleted on both `.ok` and `.err` |
| 11 | Dismiss descriptor on a layout node | First-class `dismiss` on `SurfaceInstance` |
| 12 | Stale-event policy | Core's rule: scope-alive + occlusion + guards |
| 13 | Scope/instance identity | Core-minted serials; logical (def, context) key core-internal |
| 14 | Competing schemas (port TOML, fixture, ABI, diagnostics, traces) | One owner each (ports / ports / toolchain / toolchain / core) |
| 15 | Dual Rust+TS fixture drivers | Single Rust driver, compiled to wasm for play |
| 16 | Generated-SVG vs JPEG assets | JPEGs, deduped data-URI custom properties; SVG fallback |
| 17 | Browser-history scope | Intents traced, executed as no-ops; reconcile deferred |
| 18 | Examples: outcome laundering, projection injection, route params, pinned propagation | Correlation required; timeline projection entries; `params`; taint propagation |
| 19 | Ok payloads carrying data | All empty — data travels only as projection updates |
| 20 | Grid tiles as buttons to nowhere | Plain images — no dead affordances |

### v2 — the direction reframe ("successor of HTML/CSS + widgets")

| # | Challenge | Resolution |
|---|---|---|
| 21 | "Let authors leverage CSS; the token/layout taxonomy is verbose reinvention" | **Conceded.** Styling is web-native CSS (`class`, co-located `<style>`, tokens as custom properties); style-prop closure and the generated class system deleted; checker does shallow selector/rooting/existence checks only. Consequence, named plainly: renderer neutrality is retained for *semantics* (V) and relinquished for *styling* (CSS is web-targeted; a native renderer would need its own style layer). |
| 22 | "Drop the layout taxonomy" | **Conceded.** `column/row/stack/grid/spacer` and every style prop removed; `view` (+`role`) is the only container; CSS does layout. The initial element set shrank to nine; later media dogfooding added the tenth, `video`, because playback, policy flags, and accessible labeling could not truthfully be represented by an image poster. |
| 23 | "View should be extensible; users define their own widgets; core is just xml/html" | **Half-conceded.** Markup-shaped Svelte-flavored syntax adopted; catalog extension specified as first-class data (user-registered elements with full signatures + a renderer that implements them). **Held:** the element set stays closed-but-extensible, never raw HTML — `<div onclick>` must not typecheck, because event eligibility, inert user content, a11y contracts, and the corpus's invented-signature P0s all die otherwise. |
| 24 | "Why not Svelte / build on its compiler" | Svelte's surface adopted (SFC shape, blocks, `on:`, keyed each, co-located styles); its compiler rejected as a foundation (JS-hosted semantics, spec-by-implementation, unstable AST, no Rust/wasm core). Recorded in §1. |
| 25 | "Uhura store + uhura view" split | Adopted as the file anatomy: `store { }` is the model/controller (unchanged machine language); markup is the view. The reframe confirmed rather than changed the machine design. |
| 26 | "Minimal Svelte that does not support JS" | Adopted as the project's elevator pitch — with the addition that makes it Uhura: a deterministic, replayable, headless machine and typed ports where the JS would have been. |

### v2 — verification-pass repairs (final-gate verifier, all folded)

Component-emit consumption defined (one explicit call-site model: rebind or
bare-forward; no implicit propagation) — was the blocker. Optimistic
like-count computed in post-card from the overlay (demo step 2 now
implementable). Rollback byte-equality scoped to the post subtree /
post-dismissal. `type` added to port-import grammar. Handler dispatch made
transactional (not-ready reads abort atomically). M3/M4 split (pinned
examples vs derived replay). `story-ring`/`slide` declared; `Refused.data`
dropped; carried-field type unified to `text`; canonical trace-script list;
boot-projection auto-binding in the examples resolver; `notice = none`
initializer; token-enum values admitted to the type system; post-card
renders `posted-label`/`comment-count`; threshold restated as integer; the
TERM-rule and `pad`-on-`text` findings were mooted by the markup/CSS
reframe.

### v3 — model-driven Editor topology

| # | Challenge | Resolution |
|---|---|---|
| 27 | Native code generated the entire Editor document, mixing language truth with browser presentation | Replaced by `uhura-editor-model`, a browser-neutral deterministic read-model builder. Rust publishes semantic preview content, provenance, diagnostics, and source assets; TypeScript owns all markup, chrome, and renderer-local icon geometry. An initial structured-icon wire table was removed in `uhura-editor-state/2`. |
| 28 | Saved changes replaced the document and therefore required UI-state survival machinery | Replaced by whole-state `EditorState` publication over HTTP/SSE. The application remains mounted and swaps only successfully decoded preview state; current diagnostics and an older render carry distinct revisions and explicit freshness. |
| 29 | Editor and Play had separate frontend delivery and semantic realization paths | Replaced by one application with `/` and `/play` routes and one semantic renderer. Explicit Editor/Play policies make inertness versus runtime effects a type-level construction choice rather than convention. |
| 30 | Generated frontend output behaved like authoritative source and constrained native builds around its emitted shape | `web/src/` is authoritative; generated output is ignored. CI builds/tests it before native integration, Vite proxies the native API in development, and release packaging assembles the web app, Wasm, and CLI without making Node a runtime dependency. |

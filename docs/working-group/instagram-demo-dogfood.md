# Instagram demo dogfood — July 2026

Status: implementation feedback from the Spock-backed Uhura Play demo. This
is feedback about the language/runtime boundary, not a proposal to move
Instagram product truth into Uhura.

## Outcome and boundary

The demo is an executable social application slice rather than a collection
of linked mockups. Spock owns users, follow edges, posts, storage objects,
carousel slides, story frames and views, tags, likes, comments, private saves,
and published timestamps. The provider derives every displayed count and
relationship state from those rows. Uhura owns only disposable session state:
optimistic overlays, pending flags, notices, the selected profile tab, form
text, surfaces, and navigation.

The product behavior was calibrated against Meta's public Instagram help for
[Feed](https://www.facebook.com/help/instagram/1986234648360433/),
[likes](https://www.facebook.com/help/instagram/500150933343536),
[Stories](https://www.facebook.com/help/1660923094227526/),
[Search](https://www.facebook.com/help/instagram/145838832413709),
[Explore](https://www.facebook.com/help/487224561296752),
[Reels](https://www.facebook.com/help/instagram/193454758141770),
[tagged posts](https://www.facebook.com/help/instagram/153434814832627), and
[multi-photo publishing](https://www.facebook.com/help/instagram/269314186824048).
The demo follows those interaction shapes; it does not claim to reproduce
Instagram's ranking, moderation, privacy, or recommendation systems.

## General changes adopted to unlock the demo

### Native semantic video

What was wrong: a `video` media variant ended at a JPEG poster and a
"not supported" label. That made the model claim video while the rendered
experience could only express an image.

Why this is general: source, poster, accessible name, controls, inline policy,
muting, looping, and autoplay policy are media semantics. A component cannot
truthfully reconstruct them from `image` plus a decorative play icon.

Choice: add a catalog `video` content element. Play renders a native
`<video>` and resolves both signed source and poster assets independently.
Editor previews remain deterministic and network-free by rendering only the
inert poster. No playback state enters the Uhura machine.

### Replace navigation

What was wrong: every bottom-tab hop pushed a new page. A short Feed → Search
→ Reels session manufactured a deep Back history that users do not perceive
as nested navigation.

Why this is general: redirect and peer-destination transitions recur outside
this demo and cannot be encoded honestly as push followed by synthetic Back.

Choice: add `navigate replace route(args)`. It swaps only the top entry,
creates fresh page state, closes surfaces owned by the replaced page, and
emits a distinct history-replace intent. Push and Back semantics are unchanged.

## Demo-level choices

- Home Feed and Story rings are filtered by the current actor's follow graph.
  A follow/unfollow command refreshes the affected profile, relationship
  lists, search results, Feed, and Stories from one new Spock snapshot.
- Like and comment counts are relational aggregates; follower/following and
  post counts are also derived. No display count is stored or seeded as a
  scalar.
- Saved posts use a Spock-owned `(user, post)` edge. Saves are private,
  actor-isolated, and deliberately have no public count.
- A Story ring groups an author's active ordered frames. The viewer gets
  progress plus previous/next IDs; opening a frame records a real view edge.
  Advancement is explicit because wall-clock timers do not belong in the
  deterministic core.
- Reels use stored H.264 MP4 objects and the same canonical post, like,
  comment, save, and profile routes as Feed. There is no parallel fake Reel
  database.
- Search returns both accounts and post thumbnails. Empty-query results form
  the Explore view; nonempty queries match account identity and post content.
- Create follows select → local preview → caption/alternative text → signed
  storage upload → publish. Caption and authored alternative text are optional;
  when no alternative text is supplied, the provider sends a stable descriptive
  fallback so the rendered image never loses its accessible name.
- Desktop and mobile presentation are renderer CSS, not language concepts.
  Mobile retains the bottom bar; the desktop Play frame promotes the same
  destinations into a persistent side rail.

## Executable demo coverage

| Destination | Authority-backed behavior in Play |
| --- | --- |
| Feed | Actor/self/follow-filtered posts, two-page pagination, Story rings, real like/comment/save state, and canonical profile/Post navigation |
| Search / Explore | All seeded posts plus suggested accounts for an empty query; account identity and post-content matching for submitted queries; follow controls and clickable results |
| Create | Native image selection and preview, optional caption and alternative text, signed Spock upload, RPC publication, and refreshed Feed/profile truth |
| Reels | Three stored vertical H.264 videos with posters and native controls, sharing the same likes, comments, private saves, profiles, and Post details as Feed |
| Profile | Derived post/follower/following counts; real follow edges; clickable Posts, Reels, private Saved, and Tagged grids; working follower/following lists |
| Story | Twelve stored frames grouped into author sequences, real seen state, progress, previous/next traversal, close, and author-profile navigation |
| Post / Comments | One canonical post projection across entry points, Back behavior, serialized optimistic comment submission, and authority refusal recovery |
| Prototype host | Mobile/desktop frames, clean restart, and actor switching; Instagram Play stays on the configured Spock provider while Editor/check/trace retain deterministic fixtures |

The fixture dataset mirrors these projection shapes for read-only Editor previews and
golden tests. Its strict command script remains a deterministic trace driver,
not an alternate interactive authority.

## Remaining language/runtime feedback

These are real gaps exposed by dogfooding, but none blocks this slice:

1. **Visibility-aware media lifecycle.** Uhura can declare video policy but
   cannot say which scroll-snap child is active. Reels therefore uses native
   controls rather than claiming exact Instagram autoplay/pause behavior. A
   future viewport observation primitive should report semantic visibility;
   playback itself should remain renderer-owned.
2. **Multiline controlled text.** Caption, alternative text, and comments use
   the single-line `textfield`. A checked multiline variant is warranted;
   silently styling an `<input>` to resemble a textarea is not.
3. **Per-tab navigation stacks.** Replace prevents false Back depth, but it
   recreates a destination instead of preserving each top-level tab's scroll
   and nested stack. A general tab-router model should be designed explicitly.
4. **Browser URL and deep-link reconciliation.** History intents identify
   push/replace/back, but the spike shell still does not expose canonical URLs
   or synthesize external location changes.
5. **Story timing.** Deterministic examples can model progress data, not a
   renderer clock. Timed advancement needs a host observation/event contract,
   including pause on focus loss and reduced-motion behavior.
6. **Interactive list-item composition.** Direct interactive children of an
   ARIA list need both list-item and button semantics, which one DOM `role`
   cannot carry. Demo grids use a list-item wrapper; a renderer-level semantic
   wrapper policy would remove that authoring concern.
7. **Demand-driven keyed projections.** The provider currently snapshots a
   broad graph (within Spock's real 200-row collection ceiling) and then
   assembles keyed routes. A production seam should make route demand,
   not-found, cancellation, snapshot-consistent pagination, and stale
   responses first-class without weakening deterministic delivery to the core.
8. **Server-enforced private-row reads.** Saves are correctly actor-filtered in
   the demo provider, but Spock v0's open GraphQL data floor can still expose
   every raw `save` edge to a direct client. UI filtering is not authorization.
   Row-level read policy tied to the authenticated actor is the important
   Spock runtime follow-up before this pattern can be called secure.

## Deliberate non-goals for this pass

Direct messages, notifications, camera capture, editing filters, Story
authoring, audio selection, realtime delivery, recommendation ranking, and
production authentication remain outside the current demo. Play's actor
switch is a developer-host control, not an imitation of Instagram account
management.

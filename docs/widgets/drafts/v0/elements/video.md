# `<video>`

- **Status:** Implemented playback element; a stale checker note still calls it deferred, and playback observation is absent
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Added to the base catalog (present in 0.3.0); a contradictory deferral note survives in the checker; no accepted widget RFC
- **Specification:** Pre-specification; declarative playback policy implemented
- **Implementation:** Checker, Core semantic view, browser Editor (poster pose), and Play (playing video) implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<video>` declares first-class time-based media with a deliberately small
semantic surface: provider-resolved source and poster assets, a required
accessible name, and five explicit playback-policy booleans. It is a catalog
element, not a user-authored component, and source cannot invent its
properties or events. It takes no children.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

## The stale deferral note

The catalog and the checker currently disagree in text, though not in
behavior, and this page records the contradiction precisely.

The checker's unknown-element note table still contains:

> `video` — "video is deferred: poster `<img>` + `video-off` badge pattern"

That table is consulted in exactly one place: the
`ElementResolution::Unknown` arm of element resolution. Element resolution
classifies any name the catalog owns as `CatalogElement` before the unknown
arm can be reached — and `video` **is** a catalog element in base 0.3.0, with
the contract corpus asserting its presence in the element list. The note is
therefore unreachable dead data: no input can produce it, because `<video>`
in source resolves to the catalog element and checks normally.

Reading: the catalog is the current truth — video was promoted from the
earlier poster-plus-badge deferral into a real element — and the note is a
leftover from the deferral era. It is retained here as a documented defect
pending maintainer confirmation in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22); the fix is
deleting the entry (or, if the deferral is somehow still intended, deleting
the catalog element — the corpus makes that reading untenable, since two
components use `<video>` today).

## Why Uhura needs an explicit video element

HTML's [`video`](https://html.spec.whatwg.org/multipage/media.html#the-video-element)
couples a huge imperative surface (media controllers, ready states, track
lists) with permissive markup: no accessible name is required, autoplay
policy is a moving browser target, and sources are raw URLs. Uhura's element
keeps the portable part and makes the policy explicit:

- **Playback policy is authored data, not renderer magic.** The catalog
  comment states the intent: *"playback policy remains explicit instead of
  renderer magic."* `autoplay`, `muted`, `loop`, `controls`, and
  `playsinline` are declared booleans; a renderer applies exactly what the
  author declared, and a design tool can read the policy statically without
  executing anything.
- **A name is mandatory.** `label` is required (`UH5004
  markup/missing-required-prop`), so unnamed media fails checking. The
  corpus names videos from their poster's alternative text.
- **Assets are provider-resolved.** `src` and `poster` are typed `asset`
  values resolved through the asset pipeline, not free-form URLs; the
  browser realization resolves them to concrete mp4/jpg URLs.
- **Previews are safe by construction.** The Editor realization is a poster
  pose: playback booleans are forced off and no source is attached, so a
  static board can never start playing or downloading media. This
  deterministic preview pose is pinned by a renderer policy test — the
  legitimate heir of the old "poster plus badge" deferral pattern.

CSS keeps owning size, aspect, cropping, and placement. `<video>` exists
because media identity, naming, and playback policy are semantic
capabilities rather than aesthetics.

## Current semantic contract

Real usage from the Instagram corpus — the post card:

```uhura
<video class="media" src={v.src} poster={v.poster.src} label={v.poster.alt} controls playsinline />
```

and the reel card:

```uhura
<video class="reel-media" src={v.src} poster={v.poster.src} label={v.poster.alt} muted loop controls playsinline />
```

| Contract | Current behavior |
|---|---|
| Class | `content` |
| Children | None |
| `src` | Required `asset`; the media source |
| `poster` | Optional `asset`; the pre-playback image |
| `label` | Required `text`; the accessible name |
| `autoplay` | Optional `bool` |
| `muted` | Optional `bool` |
| `loop` | Optional `bool` |
| `controls` | Optional `bool` |
| `playsinline` | Optional `bool` |
| Events | None declared; playback state is invisible to the machine |
| Playback position | Renderer-owned; absent from Uhura state and the semantic view |

## Ownership

Core owns nothing playback-related: the element contributes a semantic node
with evaluated props, and no event ever flows back. The renderer owns the
media pipeline, playback state, controls chrome, and the platform's autoplay
gating. The asset provider owns resolving `src` and `poster` references to
URLs. This means play/pause state, progress, buffering, and errors are all
currently outside the semantic model — an honest limitation listed below.

## Accessibility and validation

The checker enforces, with golden or corpus coverage:

- catalog closure — unknown properties and events on `<video>` are rejected;
- `src` and `label` are required (`UH5004`);
- the children model is `none` (`UH5006 markup/bad-children`); and
- boolean props are presence markers written bare, per the corpus style.

The browser realization maps `label` to `aria-label` on the native element.

Known accessibility gaps, stated honestly:

- **No captions or subtitles.** There is no track/caption concept at all —
  no semantic property, no asset kind, no renderer surface. For real media
  content this is the largest gap in the contract.
- **No text alternative model beyond the name.** Unlike `<img>`'s checked
  `alt`/`decorative` choice, video has only `label`; audio description and
  transcript association have no owner.
- **Autoplay against user preference.** `autoplay` is applied as declared;
  no reduced-motion or user-preference gate exists in the contract, and
  browsers' own autoplay blocking becomes silent divergence between what the
  author declared and what plays.
- The checker validates that `label` is bound, not that its text is
  non-empty or useful.

## Rendering and platform behavior

Browser Editor and Play realize the element as a native `video` tag with
class `uh-video` (plus authored classes) — one of the few catalog elements
with a non-`div` realization:

```html
<video class="uh-video authored-classes" aria-label="…" poster="…resolved…" src="…resolved…">
```

Play applies all five policy booleans as both content attributes and IDL
properties, and resolves `src` (mp4) and `poster` (jpg) through the asset
applier, which detaches and reloads the media element when a source is
removed. Editor forces every boolean off, never attaches `src`, applies only
the poster, and stamps `data-video-preview="poster"` — the pinned poster-only
preview pose. The renderer policy test asserts a board video has a resolved
poster, no source, and no autoplay/controls.

A native renderer may realize the contract with its platform media player.
It must reproduce the observable semantics — accessible name, poster-first
presentation, and literal application of the declared playback policy — or
reject the capability honestly.

## Motion

`<video>` is itself motion, but defines no semantic motion contract: no
transition, no completion event, no reduced-motion behavior. Whether
`autoplay` should be subordinated to a reduced-motion preference is an open
question below.

## Conformance

Existing executable coverage proves:

- the base catalog exposes `video` as a childless content element with the
  eight properties above and no events (the contract corpus asserts the
  catalog element list and meta-schema);
- the complete Instagram corpus checks and lowers with video semantic nodes
  in the post card and reel card; and
- the Editor poster-only pose is pinned by a renderer policy test.

A durable support claim additionally requires conformance coverage for:

- deleting (or justifying) the unreachable deferral note, with a test that
  `<video>` resolves as a catalog element;
- asset-resolution failure behavior for `src` and `poster`;
- autoplay policy divergence: declared `autoplay` versus browser gating,
  and the `muted`-autoplay interaction;
- accessibility-tree assertions for the media name; and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

## Decisions and open questions

This page is part of the v0 element documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC.

Known gaps and open questions:

1. The stale checker note (above) needs maintainer adjudication in #22:
   delete the unreachable `video` entry from the unknown-element note table.
2. Playback observation has no design: no `play`/`pause`/`ended` events, no
   position, no buffering state. Whether any of it becomes observable to
   the machine (the pager's `page-change` faces the same question) is open.
3. Captions, subtitles, audio description, and transcripts have no owner.
4. Whether `autoplay` should be gated on reduced-motion or become a
   renderer-mediated request rather than a literal command.
5. The asset pipeline currently assumes mp4 and jpg; multiple sources,
   adaptive streaming, and format negotiation have no owner.
6. Whether the machine can ever command playback (a play intent) or the
   contract stays declarative-policy-only.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking, including the stale note](../../../../../crates/uhura-check/src/markup.rs)
- [Browser tag selection and property mapping](../../../../../web/src/renderer/appliers.ts)
- [Asset resolution for source and poster](../../../../../web/src/renderer/assets.ts)
- [Renderer policy tests (Editor poster-only pose)](../../../../../web/src/renderer/tests/policies.test.ts)
- [Contract corpus tests](../../../../../crates/uhura-check/tests/contracts_corpus.rs)
- [Current Instagram post-card usage](../../../../../examples/instagram/client/components/post-card.uhura)
- [Current Instagram reel-card usage](../../../../../examples/instagram/client/components/reel-card.uhura)
- [Specification router](../../../../spec/README.md)

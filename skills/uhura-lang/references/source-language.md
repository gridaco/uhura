# Current Uhura Source Language

Use the checked Instagram corpus as the accepted syntax reference. The language is incubating and not compatibility-frozen.

## File placement and headers

Paths define identity:

```text
app/feed/page.uhura                       route feed
app/profile/[user]/page.uhura             route profile(user)
components/post-card.uhura                 component post-card
surfaces/comments-sheet.uhura              surface comments-sheet
```

One definition lives in each file. Pages are route definitions and cannot be imported. Components and surfaces are imported explicitly. Use lowercase kebab-case names.

Headers:

```uhura
page

component post-card

surface comments-sheet modality sheet
```

Imports bring exact items into file scope:

```uhura
use component post-card
use surface comments-sheet
use port feed {
  projection feed-page
  projection viewer
  command like-post
  type post-summary
}
```

Declare route params, component/surface props, and component emits explicitly:

```uhura
param user: id

props {
  post: post-summary
  liked: bool
}

emits {
  like-toggled(post: id, now-liked: bool)
}
```

## Store blocks

Pages and surfaces own reconstructible UI-session state and handlers:

```uhura
store {
  state {
    like-overlay: map[id]bool = {}
    like-pending: map[id]bool = {}
    notice: text? = none
  }

  on like-toggled(post: id, now-liked: bool)
      when now-liked && !(like-pending[post] ?? false) {
    set like-overlay[post] = true
    set like-pending[post] = true
    send like-post(post: post)
  }

  on like-post.ok(tag, cmd) {
    set like-pending[cmd.post] = none
    set like-overlay[cmd.post] = none
  }

  on like-post.err(tag, cmd, refusal) {
    set like-pending[cmd.post] = none
    set like-overlay[cmd.post] = none
    set notice = "Could not like this post."
  }
}
```

Current value shapes include `bool`, `int`, `text`, `id`, `tag`, optionals, lists, records, and maps. Core has no floats, clock, randomness, environment, network, storage, URL, clipboard, renderer geometry, or unordered iteration.

## Store statements

Only these statement forms are current:

```text
set field = expression
set map[key] = expression
set map[key] = none
send command(args)
send command(args) as tag_binding
open-surface name(args)
dismiss
navigate route(named_args)
navigate replace route(named_args)
navigate back
```

`set` writes only the current scope. Handler execution is transactional. At most one guarded handler for an event runs. Structural statements apply at dispatch end.

`send` emits a typed provider command and creates a pending correlation. An imported command provides `<command>.ok` and `<command>.err` events. Provider updates settle authority truth before the outcome handler clears an optimistic overlay.

Use `as t` when local optimistic state needs the minted command tag as a stable key.

## Navigation and surfaces

Use plain `navigate` for hierarchical push navigation, `navigate replace` for peer/redirect replacement, and `navigate back` to reveal retained previous page state.

```uhura
on comments-requested(post: id) {
  open-surface comments-sheet(post: post)
}

on profile-tab-selected(user: id) {
  navigate replace profile(user: user)
}
```

Only a surface may `dismiss`. Dismissal pops that instance and emits focus restoration intent. Replacing or popping a page force-closes surfaces owned by the removed page.

## Markup

Use catalog elements and semantic events:

```uhura
<view class="post-row">
  <button label="Like" pressed={liked}
          on:press={emit like-toggled(post: post.id, now-liked: !liked)}>
    <icon name={if liked then "heart-filled" else "heart"} />
  </button>
  <text>{post.caption}</text>
</view>
```

The catalog, not HTML, defines legal elements, props, slots, and events. Elements represent semantics; the renderer decides concrete controls and pixels.

Structural forms:

```uhura
{#if condition}
  ...
{:else}
  ...
{/if}

{#each items as item (item.id)}
  ...
{/each}

{#match value}
  {:when variant binding}
    ...
{/match}
```

Every repeated child needs a stable key. Match closed unions exhaustively. Projection availability uses `loading`, `failed reason`, and `ready value` arms.

Expressions are deliberately small: literals, lexical names, field access, map lookup, option fallback `??`, boolean/comparison operators, integer arithmetic, text concatenation `++`, `if ... then ... else ...`, record literals, and accepted builtins such as `count` and `to-text`. Check the existing corpus before assuming another operation exists.

Forward a declared component emit with `on:event-name`; produce a semantic event with `emit event-name(...)`. Do not invent DOM event names unless the catalog declares equivalent semantics.

## CSS

An optional final `<style>` block contains real CSS. Uhura shallow-checks selectors against source structure; declarations pass through to the renderer stylesheet.

Use CSS for layout, visual treatment, responsive presentation, and theme tokens. Do not use CSS to simulate state, authorization, command settlement, navigation, surface ownership, accessibility semantics, or missing language features.

## Source bounds

Current spike bounds include one definition per file, files up to 256 KiB, nesting up to 32, up to 512 nodes per view, and up to 128 handlers per page. Keep authoring substantially below these ceilings.

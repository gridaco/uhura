# Editor seam TODO

This tracker is intentionally local to the browser Editor. Items here describe
preview-host and canvas implementation work; they do not change the Uhura
language, stylesheet syntax, model protocol, or runtime semantics.

## Current tactical boundary

- [x] Parse and adapt one constructed stylesheet before preparing preview DOM,
      then share that immutable sheet across every preview ShadowRoot.
- [x] Remove the detached `<style>.sheet` lifecycle dependency that silently
      published an unstyled board.
- [x] Reconcile saved-source updates by stable preview identity, retaining
      unchanged frames and ShadowRoots while realizing only changed previews.
- [x] Keep global CSS updates separate from frame identity: retained previews
      adopt the next shared stylesheet without remounting.
- [ ] Add a real-browser regression test that builds the board detached,
      installs it, and verifies `:root` tokens plus `body` typography inside a
      preview ShadowRoot.
- [ ] Automate the live-update browser regression for one changed preview,
      unchanged sibling identity, stale-save retention, and stylesheet-only
      updates. The current Vitest suite exercises detached preparation and the
      current -> metadata-only -> stale -> cold-invalid -> recovered transplant
      lifecycle with a dependency-free DOM model; computed layout and actual
      constructed-stylesheet behavior still require a real browser.

The current root adaptation is a compatibility shim for Editor previews, not a
general CSS emulation layer. Do not expand it into a home-grown CSS parser.
Constructed stylesheets discard `@import`; do not treat that as a language
decision here. Track or validate it explicitly if imports enter real projects.

## Follow-ups

- [ ] Decide whether long-term preview fidelity requires an independent
      document boundary. If it does, prototype one scriptless iframe per
      visible preview and keep iframe activation/disposal local to Editor.
- [ ] If iframe previews are prototyped, measure the Instagram board (currently
      91 previews) before choosing eager mounting, visibility-based
      activation, or a bounded pool.
- [ ] Extract the duplicated `.uh-*` semantic renderer base rules from Editor
      and Play into one authoritative source. Application CSS must continue to
      come from the same compiled stylesheet payload in both surfaces.
- [ ] Document and test known environment differences while Shadow DOM remains:
      viewport media queries, viewport units, fixed positioning, and document
      root selectors do not create a per-preview browsing context.
- [ ] Keep any future styling restriction or application-root contract out of
      this tracker; that would require an explicit product/design decision.

## Non-goals for the quick fix

- Rewriting arbitrary CSS text.
- Changing authored Instagram theme CSS to accommodate the Editor.
- Changing the Editor state protocol or native model builder.
- Moving Play into an iframe or ShadowRoot.

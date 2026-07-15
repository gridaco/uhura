# Instagram — Uhura client slice

This is the Uhura client half of the live Instagram example. It has a paged
feed filtered by the active actor's follow graph, real aggregate counts,
clickable profile/Post/Reels/Saved/tagged grids, multi-frame stories,
follower/following lists with follow controls, Search/Explore, playable stored
Reels, comments, optimistic likes and private saves, and single-image post
creation. Every displayed count comes from post, like, comment, or follow rows
rather than a seeded headline. The same checked
program has two deliberately separate data paths:

- `uhura play` uses the app-owned TypeScript provider built to
  `providers/dist/spock.js`.
  It reads authority truth through Spock GraphQL, signs image and video downloads and
  uploads selected files through Spock storage, and sends domain commands
  through Spock RPC.
- `uhura editor` checks the project and hosts the read-only, model-driven
  Editor; examples, checks, and traces continue to use the deterministic
  fixture under `fixtures/`. Editor previews never need a running backend.

The Spock authority for live play is
[`../../../examples/instagram-poc/app.spock`](../../../examples/instagram-poc/app.spock).
The provider is an explicit demo binding: it translates Spock's schema into
the Uhura port projections without putting Spock knowledge in Uhura
Core. Browser `File` values stay between the play shell and the provider;
Uhura Core sees only the resulting storage-object id plus serializable display
metadata such as the filename—never the file object or its bytes.

When Play is served by the combined framework host, the provider first reads
the strictly versioned `spock-host-environment/1` document from
`/~project/environment`. Valid authority paths are same-origin capabilities
and win over `uhura.toml`; the absolute URLs in `uhura.toml` are a standalone
fallback only when discovery is unavailable, invalid, or does not answer
within two seconds. A valid environment may advertise `graphql_path: null`.
That means GraphQL is deliberately absent: storage and RPC paths remain the
integrated capabilities, and the first snapshot query reports the missing
GraphQL capability instead of silently contacting a configured fallback host.

## Run the current split checkout (transition path)

Canonical framework projects use `spock dev` or `spock start` to own one
project, origin, port, and lifecycle. This repository's Instagram dogfood still
keeps its Spock authority and Uhura client in separate example roots, so the
two-process composition runner remains the in-place transition and comparison
oracle.

From the Spock repository root, use the general Spock–Uhura composition runner
with this example's two inputs. It starts the authority, waits for it, launches
the Uhura Editor, and stops both together:

```sh
./scripts/spock-uhura.sh \
  examples/instagram-poc/app.spock \
  uhura/examples/instagram-uhura
```

Open <http://127.0.0.1:8787/> and use **Play** in the Editor's right details
panel to enter the live prototype at `/play`. The runner builds the frontend,
provider, and Wasm artifacts, then serves them without a Node process at
runtime.

For low-level development of this split example, the equivalent standalone
two-terminal commands are:

From the Spock repository root, start the authority and the Uhura Editor server
in separate terminals:

```sh
cargo run --locked -p spock-cli -- run examples/instagram-poc/app.spock --port 4000
```

```sh
uhura/scripts/build-wasm.sh # needed once, and after wasm changes
(cd uhura/web && corepack pnpm build) # after web/provider changes
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  editor uhura/examples/instagram-uhura --port 8787
```

Open <http://127.0.0.1:8787/> and click **Play**. The Play toolbar can restart
the UI session, switch between the 390 × 844 Mobile and 1280 × 800 Desktop
frames, and select any seeded Spock actor. The app runs exclusively against the
configured Spock provider; live play defaults to Mira and to the endpoints in
`uhura.toml` only when no valid integrated host environment was discovered.
The Mobile/Desktop frame is Play-chrome preference state persisted in browser
local storage. Actor selection is tab-local session-storage state. Play never
reads or rewrites the application's query parameters for any
of these controls, so the URL remains entirely available to the real Uhura app.

Restart creates a clean Uhura UI session; it does not reset or roll back Spock
data. Actor selection is local prototype impersonation over the demo's
`X-Spock-Actor` seam, not production authentication. The Mobile/Desktop choice
is a visual frame in this placeholder host; true browser viewport/media-query
emulation is intentionally deferred.

The strict fixture script is intentionally bounded to deterministic Editor,
check, and trace walkthroughs, so it is not offered as a Play provider. This
keeps every valid control in the interactive Instagram demo Spock-backed.

Use the Create tab to choose a JPEG, PNG, or WebP file. Play sends the bytes
directly to Spock's signed upload URL, then publishes the returned object id
through `create_image_post`; the post appears first in the active actor's feed
and profile grid. Captions and authored alternative text are optional; the
provider supplies a stable descriptive fallback when alternative text is left
blank. Spock's demo database is in memory by default and starts from the seed
on each server restart.

This is intentionally a local v0 integration seam, not an authorization
boundary: `X-Spock-Actor` is forgeable, the data floor is open, and storage v0
trusts the upload's declared Content-Type. The RPC still demonstrates the
domain checks the future policy/identity floor must make authoritative.

## Run the read-only editor (default)

```sh
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  uhura/examples/instagram-uhura --port 8787
```

Open <http://127.0.0.1:8787/>. The Editor exposes deterministic,
fixture-backed examples as a read-only projection. Its searchable navigator
and artboards select previews; the details panel then shows that preview's
metadata, computed example values and provenance, and declared interactions.
The headerless Editor uses a compact floating toolbar for Cursor, Hand, zoom,
and centering. Press
`Cmd+\` (`Ctrl+\` on Windows/Linux) to hide or restore all editor chrome;
that preference is remembered in local browser storage. The wheel pans,
Ctrl/Cmd-wheel and pinch zoom, `V`/`H` switch tools, and holding Space
temporarily activates Hand. None of these controls edits the source or live
Spock data.

Click **Play** in the right details panel to enter the real Play shell at
`/play` on the same server; the dedicated `uhura play` command remains
available when the Editor is not needed. Saving a valid source change replaces
the preview model without restarting the command or reloading the application.
An invalid save keeps the last renderable previews visible, marks them stale,
and shows diagnostics until a later valid save recovers.
When running from the project directory, bare `uhura` is equivalent to
`uhura editor`. Build the wasm bundle once with `uhura/scripts/build-wasm.sh`
before using Play in a source checkout.

## Layout

- `app/` — routes and pages (`.uhura`)
- `components/` — reusable markup + `store` machines
- `ports/` — the typed service seam the Spock-backed provider satisfies
- `providers/` — play-only app bindings to live authority
- `fixtures/` — scripted provider data and scripts for Editor, trace, and CI
- `catalog/`, `styles/`, `surfaces/` — icon/token catalog, CSS, surface defs
- `uhura.toml` / `uhura.lock` — app manifest and port-binding lock

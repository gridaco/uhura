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
- `uhura editor` builds and hosts the read-only Canvas; examples, checks, and
  traces continue to use the deterministic fixture under `fixtures/`. Canvas
  output never needs a running backend. `uhura project` remains available when
  only the self-contained HTML artifact is wanted.

The Spock authority for live play is
[`../../../examples/instagram-poc/app.spock`](../../../examples/instagram-poc/app.spock).
The provider is an explicit demo binding: it translates Spock's schema into
the Uhura port projections without putting Spock knowledge in Uhura
Core. Browser `File` values stay between the play shell and the provider;
Uhura Core sees only the resulting storage-object id plus serializable display
metadata such as the filename—never the file object or its bytes.

## Run the Editor with live Play

From the Spock repository root, use the general Spock–Uhura composition runner
with this example's two inputs. It starts the authority, waits for it, launches
the Uhura Editor, and stops both together:

```sh
./scripts/spock-uhura.sh \
  examples/instagram-poc/app.spock \
  uhura/examples/instagram-uhura
```

Open <http://127.0.0.1:8787/> and use **Play** in the Editor's right details
panel to enter the live prototype at `/play`. The frontend and Wasm artifacts
are built as part of contributor setup and then served without a Node process
at runtime.

For low-level development, the equivalent two-terminal commands are:

From the Spock repository root, start the authority and the Uhura Editor server
in separate terminals:

```sh
cargo run --locked -p spock-cli -- run examples/instagram-poc/app.spock --port 4000
```

```sh
uhura/scripts/build-wasm.sh # needed once, and after wasm changes
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  editor uhura/examples/instagram-uhura --port 8787
```

Open <http://127.0.0.1:8787/> and click **Play**. The Play toolbar can restart
the UI session, switch between the 390 × 844 Mobile and 1280 × 800 Desktop
frames, and select any seeded Spock actor. The app runs exclusively against the
configured Spock provider; live play defaults to Mira and to the endpoints in
`uhura.toml`.
The Mobile/Desktop frame is Play-chrome preference state persisted in browser
local storage. Actor selection is tab-local session-storage state. Play never
reads or rewrites the application's query parameters for any
of these controls, so the URL remains entirely available to the real Uhura app.

Restart creates a clean Uhura UI session; it does not reset or roll back Spock
data. Actor selection is local prototype impersonation over the demo's
`X-Spock-Actor` seam, not production authentication. The Mobile/Desktop choice
is a visual frame in this placeholder host; true browser viewport/media-query
emulation is intentionally deferred.

The strict fixture script is intentionally bounded to deterministic Canvas,
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
  uhura/examples/instagram-uhura --port 8787 --out=uhura/renders
```

Open <http://127.0.0.1:8787/>. This first editor surface deliberately exposes
the deterministic, fixture-backed Canvas as a static, read-only projection.
Its searchable navigator and artboards select previews; the details panel then
shows that preview's metadata and declared interactions. Replay-derived
previews are connected to their direct parent in a dedicated rail above each
row; edge labels summarize the directly authored replay steps, and selection
highlights immediate parent/child relationships. These are checked example
provenance edges, not a second runtime state graph. The headerless editor
uses a compact floating toolbar for Cursor, Hand, zoom, and centering. Press
`Cmd+\` (`Ctrl+\` on Windows/Linux) to hide or restore all editor chrome;
that preference is remembered in local browser storage. The wheel pans,
Ctrl/Cmd-wheel and pinch zoom, `V`/`H` switch tools, and holding Space
temporarily activates Hand. None of these controls edits the source or live
Spock data.

Click **Play** in the right details panel to enter the real Play shell at
`/play` on the same server; the dedicated `uhura play` command remains
available when the Canvas is not needed. Restart the command to rebuild the
Canvas after source changes.
When running from the project directory, bare `uhura` is equivalent to
`uhura editor`. Build the wasm bundle once with `uhura/scripts/build-wasm.sh`
before using Play in a source checkout.

To generate the same self-contained Canvas without starting a server:

```sh
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  project uhura/examples/instagram-uhura --out=uhura/renders
```

The generated `uhura/renders/canvas.html` contains the deterministic page,
component, loading, failure, and interaction previews without contacting
Spock.

## Layout

- `app/` — routes and pages (`.uhura`)
- `components/` — reusable markup + `store` machines
- `ports/` — the typed service seam the Spock-backed provider satisfies
- `providers/` — play-only app bindings to live authority
- `fixtures/` — scripted provider data and scripts for canvas, trace, and CI
- `catalog/`, `styles/`, `surfaces/` — icon/token catalog, CSS, surface defs
- `uhura.toml` / `uhura.lock` — app manifest and port-binding lock

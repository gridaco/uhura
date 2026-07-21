# Uhura 0.4 project, resolution, and identity contract

- **Status:** Active candidate project boundary
- **Source syntax:** [Source and lowering](source.md)
- **Runtime semantics:** [Source-neutral kernel](kernel.md)
- **Application and host:** [Application profile](application.md)
- **Validation:** [Conformance](conformance.md)

This document owns the non-source inputs to the 0.4 compiler: project
discovery, logical modules, package resolution, the lock, semantic identities,
source provenance, and the boundary between machine, presentation,
deployment, and physical-source identity.

The central rule is:

> A locator tells the compiler where to find a declaration. An identity tells
> the runtime and tools what resolved declaration or program it is. Physical
> source coordinates explain where it was authored. These are different
> values.

Changing a physical file or logical module locator must not change semantic
machine identity when the resolved public names, composition names, and
lowered semantic IR remain unchanged.

## 1. Project files and authority

The project root is the directory containing `uhura.toml`.

| File | Authority | Required |
| --- | --- | --- |
| `uhura.toml` | Language version, package identity, logical modules, dependency requirements, and optional project resources | Always |
| `uhura.lock` | Exact resolved non-standard package graph and integrity | Exactly when `[dependencies]` is non-empty |
| `host.toml` | One live deployment entry, machine configuration, presentation selection, and host adapter bindings | Only for live admission such as Play |

Core checking, pure execution, and evidence do not require `host.toml`.
`host.toml` is read only after source resolution and checking succeed. Its
exact schema is defined by
[Application profile §5](application.md#5-host-boundary).

All three files are closed schemas. Unknown keys are errors. A path mentioned
by any schema is interpreted relative to the project root unless the owning
field says otherwise.

## 2. Names and locator taxonomy

### Package name and package identity

A package name is one or more lowercase ASCII kebab-case segments separated
by `.`. Each segment matches:

```text
[a-z][a-z0-9]*(-[a-z0-9]+)*
```

The project compatibility version is a positive TOML integer. It is not
SemVer and does not claim package-manager release semantics. Exact acquired
content is pinned by the lock.

```text
PackageId = package-name "@" compatibility-version
```

For example:

```text
examples.programs@1
```

Changing either component changes every public identity in that package.

### Logical module path

A logical module path is one or more `::`-separated lowercase ASCII
snake-case segments. Each segment matches:

```text
[a-z_][a-z0-9_]*
```

`crate` and `uhura` are reserved roots and cannot be module segments or
dependency aliases.

A logical module path is a compile-time namespace and locator. It is not a
package identity, declaration identity, runtime owner, or machine instance.

### Source declaration locator

A source locator has one of these shapes:

```text
crate :: logical-module-path :: public-name
dependency-alias :: logical-module-path :: public-name
```

The standard package may explicitly export selected declarations at its root,
which admits standard locators such as `uhura::ui`. Ordinary package source
does not infer a root export from a filename.

`pub use` may add a second locator for one declaration. A local `as` may add a
local binding. Neither creates a second declaration identity.

### Public declaration identity

Public names are unique across all source modules in one package.

```text
PublicId = PackageId "::" public-name
```

For example, all valid locators for the declaration ultimately identify:

```text
examples.programs@1::Notice
```

The logical module route does not occur in `PublicId`. Moving `Notice` from
logical module `notice` to `shared::notice` and updating its locators therefore
preserves `PublicId`.

### Host declaration selector

A host selector is deliberately not a source module locator:

```text
crate :: public-name
dependency-alias :: public-name
```

It resolves to one `PublicId` before deployment admission and hashing. This
keeps `host.toml` independent of source module organization. Module-qualified
host selectors are rejected.

### Composition owner path

The root machine owner is written conceptually as `root`. A composed part has
a stable dot-separated path:

```text
root
feed
feed.card
```

A composition owner path is semantic. Renaming one of its segments changes
the composed program identity. Moving the part declaration between modules
does not.

### Port locator

A port locator is its owner path followed by the port name:

```text
router
feed.api
feed.card.media
```

A root port omits `root.`. Every segment is a source composition identifier;
`.` is only the separator and cannot occur inside a segment. Port locators are
sorted by the UTF-8 bytes of their segments for canonical host and composition
operations.

## 3. `uhura.toml`

The complete 0.4 schema is:

```toml
[project]
name = "examples.programs"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"
"shared::notice" = "src/shared/notice.uhura"

[evidence.modules]
programs = "evidence/programs.uhura"

[dependencies]
vendor = { package = "vendor.icons", version = 1, path = "vendor/icons" }

[assets]
manifest = "fixtures/assets/manifest.toml"

[icons]
default = "lucide"

[icons.brand]
font = "assets/brand.woff2"
glyphs = "assets/brand.json"
```

The allowed top-level tables are exactly `project`, `modules`, `evidence`,
`dependencies`, `assets`, and `icons`.

### `[project]`

`project` is required and admits exactly:

- `name`: required package name;
- `version`: required positive compatibility integer; and
- `language`: required exact string `"0.4"`.

There is no default package identity or language version.

### `[modules]`

`modules` is required and non-empty. It is a one-to-one map from logical
module path to safe project-relative `.uhura` file.

Resolution rejects:

- an empty or invalid logical path;
- `crate` or `uhura` as a segment;
- a duplicate logical path or physical file;
- an absolute path, URL, backslash, NUL, empty segment, `.` segment, or `..`
  segment;
- a non-`.uhura` file;
- a missing, non-regular, non-UTF-8, case-ambiguous, or symlink-escaping file;
  and
- a project-owned `.uhura` file outside ignored output and dependency roots
  that is not present in the map.

Ignored output roots are `.git`, `build`, `node_modules`, `renders`, and
`target`. Dependency roots declared by `[dependencies]` are checked as their
own packages, not as modules of the current package.

Manifest table order and physical path spelling do not establish semantic
order. Before parsing, the resolver supplies each source as:

```text
ResolvedSource {
  package: PackageId,
  module: LogicalModulePath,
  physical_path: ProjectRelativePath,
  utf8_bytes
}
```

Source contains no language or module header. A framework may generate this
map, but the checked resolver input remains this explicit closed map.

### `[evidence.modules]`

`evidence.modules` is optional and maps logical evidence-module paths to
physical source files with the same path rules as `[modules]`. Every value is
a distinct safe project-relative `.uhura` file and cannot also occur in
`[modules]`:

```toml
[evidence.modules]
programs = "evidence/programs.uhura"
```

These tooling modules use the same 0.4 lexer, parser, expressions, imports, and
source identity as core modules. Their manifest role is the capability
boundary: they may declare only scenarios, checkpoints, and examples, and may
import public declarations from the resolved package plus compiler-provided
`uhura` contracts. They cannot contribute a machine, type, value, UI
declaration, re-export, external package authority, or host authority, and do
not enter `MachineProgramId` or `PresentationId`. Omission is the canonical
empty evidence set; an empty table is rejected.

### `[dependencies]`

`dependencies` is optional. Each key is the source prefix used by `use`; it
matches one logical-module segment and cannot be `crate` or `uhura`.

Each value admits exactly:

- `package`: required package name;
- `version`: required positive compatibility integer; and
- `path`: required safe project-relative directory containing that package's
  `uhura.toml`.

The 0.4 candidate supports only vendored path acquisition. Registry names,
URLs, Git references, mutable channels, version ranges, and implicit global
search are outside this contract. Adding a future acquisition kind must not
change `PackageId`, public-name resolution, or semantic hashing.

Dependency aliases are local locators. Renaming an alias and updating its
`use` sites preserves semantic IR when it resolves to the same package and
declarations.

The reserved `uhura` root resolves to the compiler-provided standard package
selected by the language and semantic-IR protocol. A project cannot shadow
it. Referenced standard definitions and contracts enter semantic IR in the
same way as referenced dependency definitions; no user lock entry is needed.

### `[assets]` and `[icons]`

These tables are optional root-application resources:

- `assets` admits only optional `manifest`, a safe project-relative path;
- `icons` admits optional `default` plus zero or more family tables;
- each family admits exactly `font` and `glyphs`, both safe
  project-relative paths; and
- `lucide` remains the built-in family and cannot be replaced locally.

Resource paths and manifest formatting do not enter machine-program identity.
Resolved resource content enters presentation or deployment identity only
when the applicable profile uses it.

Vendored dependency packages are source-only in Uhura 0.4. Their manifests
must omit `[assets]` and project-local `[icons]` families; the built-in
`lucide` default remains available without package content. Canonical logical
resource names for acquired packages are deliberately deferred instead of
guessing a path-based identity in this version.

## 4. Module and package resolution

Resolution is closed and precedes checking:

1. parse and validate the root `uhura.toml`;
2. when dependencies exist, parse and validate `uhura.lock`;
3. capture each resolved package and verify its integrity;
4. build the exact package-alias and logical-module maps;
5. parse all mapped source against its supplied resolved source identity;
6. collect package-global public declarations and explicit re-exports;
7. resolve every `use` to one declaration or standard profile feature; and
8. pass a closed resolved declaration graph to the checker.

Filesystem discovery, current working directory, case folding, environment
variables, package installation state, and import execution never participate
in resolution.

Package resolution is acyclic. This does not forbid a statically closable
cycle among declarations or types inside the already resolved package graph.
The source checker owns those dependency-cycle rules.

The resolver rejects:

- a dependency requirement without one exact lock binding;
- a lock binding with the wrong package name or compatibility version;
- a missing, duplicate, unreferenced, or integrity-invalid package record;
- a dependency alias collision;
- a public name collision anywhere in one package;
- a vendored dependency manifest with `[assets]` or project-local `[icons]`;
- an unresolved, ambiguous, private, wildcard, dynamic, conditional, or
  side-effect-only source locator; and
- a locator whose resolved declaration kind is invalid in that source
  context.

## 5. `uhura.lock`

When `[dependencies]` is empty, `uhura.lock` must be absent. When dependencies
are present, it is required and has this closed schema:

```toml
protocol = "uhura-lock/0"

[root]
package = "examples.programs@1"
dependencies = { vendor = "vendor.icons@1" }

[[package]]
package = "vendor.icons@1"
source = { kind = "path", path = "vendor/icons" }
integrity = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
dependencies = {}
```

The root table admits exactly `package` and `dependencies`. Every package
record admits exactly `package`, `source`, `integrity`, and `dependencies`.
The `source` inline table admits exactly `kind = "path"` and a safe
project-relative directory.

`root.package` must equal the manifest-derived root `PackageId`.
`root.dependencies` and each package record's `dependencies` map dependency
aliases to exact `PackageId` values. They must equal the corresponding
package manifest requirements after resolution.

There is exactly one record for every resolved non-standard dependency and no
record for the root or standard package. A package compatibility line may
resolve to only one artifact in a lock.

`integrity` is `sha256:` followed by 64 lowercase hexadecimal digits. It is
computed over:

```text
frame(
  "uhura-package-artifact/0",
  PackageId,
  sorted(logical-module-path, raw UTF-8 source bytes),
  sorted(dependency-alias, resolved PackageId),
  empty-resource-collection
)
```

`frame(tag, fields...)` is the UTF-8 tag and each field encoded as an unsigned
64-bit big-endian byte length followed by those bytes. Every sorted pair is
`frame("", key, value)`. One collection is the concatenation of those pair
frames and enters the outer artifact frame as one field. Sorting compares
canonical UTF-8 key bytes. Physical paths, manifest whitespace, lock
whitespace, and dependency acquisition paths are not artifact-integrity
material.

`empty-resource-collection` is the zero-byte collection field. It is
normative for Uhura 0.4 because acquired packages are source-only. The field
is reserved so a future language version can introduce canonical logical
resource names without changing the framing of the other collections.

The lock is generated resolution data, never source authority. Its table
order is nonsemantic. The lock digest is not inserted wholesale into a
machine program hash: only actually referenced resolved semantic definitions
and contract instances enter that hash.

## 6. Semantic node and source provenance

Every lowered declaration and editor-visible semantic construct receives a
stable node identity after resolution and composition:

```text
NodeId = sha256(
  frame(
    "uhura-node/0",
    resolved-public-owner,
    composition-owner-path,
    node-kind,
    canonical-semantic-node-path
  )
)
```

`resolved-public-owner` is the owning `PublicId`. The root composition path is
`root`; a part uses its stable part path. A private nominal declaration must
have the one public owner required by the source contract. A private
structural helper used beneath several public owners is canonicalized once
per owner and therefore receives owner-specific node identities.

The canonical semantic node path uses:

- declared names for named fields, handlers, updates, and constructors;
- canonical owner and constructor order;
- an ordinal only where order is semantic, such as statements and invariant
  obligations; and
- branch constructor identity rather than a physical source offset.

Example paths include:

```text
state/count/initial
handler/Increment/statement/0
invariant/0
before-commit/statement/2
```

It excludes logical module path, source locator, `use` layout, filename,
comments, formatting, line, column, and byte offset.

`SiteId` is a `NodeId` whose kind can be observed through a deterministic
program fault. Runtime-observable site identities enter semantic machine IR.
Node identities used only to join editor data to source provenance do not
independently add behavior to the machine hash.

Physical authorship is retained in a separate sidecar:

```json
{
  "protocol": "uhura-provenance/0",
  "sources": [
    {
      "source": 0,
      "package": "examples.programs@1",
      "module": "programs",
      "path": "programs.uhura",
      "sha256": "<64-lowercase-hex>",
      "bytes": 1200
    }
  ],
  "occurrences": [
    {
      "node": "<NodeId>",
      "source": 0,
      "start": 100,
      "end": 130,
      "role": "definition",
      "owner": "root"
    }
  ],
  "topology": {
    "protocol": "uhura-authored-interaction-topology/0",
    "nodes": [
      {
        "id": "part:examples.programs@1::Application:counter",
        "kind": "part",
        "machine": "examples.programs@1::Application",
        "label": "counter",
        "sources": [
          {
            "node": "<NodeId>",
            "role": "definition",
            "owner": "root"
          }
        ]
      }
    ],
    "edges": []
  }
}
```

`start` and `end` are UTF-8 byte offsets with
`0 <= start <= end <= bytes`. `role` is one of `definition`, `reference`,
`generated`, or a versioned profile-owned role. Occurrences sort by node
identity, package, logical module, source path, byte range, then role. A
relative path is unique within its package; two locked packages may both
contain a source such as `main.uhura`. The sidecar may change after a move or
reformat while all semantic identities remain equal.

`topology` is the checker-owned overlay for authored facts erased by
lowering: module and part ownership, composition, computed values, invariants,
update definitions, handlers, committed observations, and their direct
`reads`, `calls`, and `observes` dependencies. A read edge is emitted for a
resolved machine or part read used anywhere in an authored expression or
statement; lexical bindings and assignment targets do not invent read edges.
Its source selectors must match occurrences in the same sidecar. The host
merges this overlay with the graph derived from the single runtime IR, then
publishes that same merged graph and physical-source projection to Editor and
Play. It is inspection data, not a second executable IR, and does not enter
machine identity.

The merged `uhura-interaction-graph/0` graph carries a closed
`outcome_policies` object keyed by every outcome node ID, with exactly one
`"commit"` or `"abort"` value for each outcome and no other keys. This keeps
the policy inspectable in the canonical typed graph and Editor without
encoding policy into a display label or publishing a parallel artifact.

Diagnostics may use revision-local file indices internally, but persisted
editor and inspection artifacts use the source table and node identities.
The compiler retains sufficient occurrences to recover module and part
hierarchy, state ownership, handlers, dependencies, ports, invariants,
outcomes, UI edges, and generated lowering relationships.

## 7. Identity layers and hashing

The following identities are independent:

| Identity | Meaning | Physical source participates |
| --- | --- | --- |
| `PackageId` | Public compatibility namespace | No |
| `PublicId` | One public declaration | No |
| `NodeId` / `SiteId` | One resolved semantic node or fault site | No |
| `MachineProgramId` | One complete executable machine meaning | No |
| `PresentationId` | One checked UI projection meaning | No |
| `DeploymentId` | One host-selected executable deployment | Only selected resource contents, never their paths |
| `SourceRevisionId` | One captured set of physical project inputs | Yes |
| Runtime instance identity | One admitted execution | Host supplied |

Unless a field explicitly uses the textual `sha256:` integrity form, every
SHA-256 identity below is represented as exactly 64 lowercase hexadecimal
digits.

### Machine program identity

```text
MachineProgramId = sha256(
  frame("uhura-machine-program/0", canonical-semantic-machine-IR)
)
```

Canonical semantic IR is compact canonical JSON with UTF-8 object keys sorted
lexicographically, no insignificant whitespace, exact tagged Uhura values,
and no floating-point JSON numbers. Semantic arrays retain the canonical
order fixed by the kernel and source composition contract.

The machine projection includes:

- the semantic-IR and kernel protocol identities;
- resolved machine `PublicId`;
- complete composed configuration, state, input, command, outcome, and
  observation domains;
- initialization, requirements, handlers, updates, reconciliation,
  invariants, and commit/abort policy;
- canonical owner, field, and constructor order;
- runtime-observable semantic site identities;
- reachable nominal types, constants, functions, and route tables;
- resolved port contract instances, codecs, and configuration expressions;
  and
- exact `PublicId` values of composed public part declarations, part
  composition paths, and lowered dependency edges.

It excludes:

- physical source paths, byte spans, source hashes, and `SourceRevisionId`;
- logical module paths and the set of modules;
- source `use`, `pub use`, alias, and re-export routes;
- dependency aliases, acquisition paths, and the lock as a whole;
- comments, formatting, and equivalent source spelling;
- unused declarations and unreferenced package records;
- provenance occurrences;
- evidence and static examples;
- UI and presentation IR; and
- host configuration, adapters, resources, and runtime instance identity.

The selected machine's transitive reachable semantic closure is hashed.
Changing an unused declaration does not change that machine's identity.

### Presentation identity

Presentation identity is separate:

```text
MachineUiInterfaceId = sha256(
  frame(
    "uhura-machine-ui-interface/0",
    bound machine PublicId,
    canonical reachable input and observation contracts
  )
)

PresentationId = sha256(
  frame(
    "uhura-presentation/0",
    resolved presentation PublicId,
    bound machine PublicId,
    MachineUiInterfaceId,
    canonical presentation IR
  )
)
```

The machine UI-interface projection includes the complete input and
observation contracts reachable by the presentation. Presentation IR includes
referenced UI types, functions, framework declarations, semantic UI node
identities, and resolved resource logical identities and contents. It excludes
physical paths, formatting, evidence, host adapters, renderer-local preview
pose, and machine implementation details outside that interface.

Changing UI does not change `MachineProgramId`. Changing machine behavior
without changing its public input/observation interface preserves
`PresentationId` but changes the later deployment identity through
`MachineProgramId`. Changing the bound machine identity or interface changes
`PresentationId` even if the presentation source is unchanged.

Deployment identity is defined after host resolution by
[Application profile §5](application.md#5-host-boundary).

### Source revision identity

`SourceRevisionId` exists for incremental rebuilding, cache invalidation, and
Editor freshness:

```text
SourceRevisionId = sha256(
  frame(
    "uhura-source-revision/0",
    filesystem-case-mode,
    sorted(project-relative-path, raw-file-bytes)
  )
)
```

It covers every captured project input, including manifests, lock, sources,
and referenced resources. It is deliberately path- and formatting-sensitive.
It never appears as machine program identity, checkpoint compatibility, or
semantic receipt identity.

## 8. Required invariance

The following changes preserve `PublicId`, semantic IR, `NodeId`,
`SiteId`, and `MachineProgramId` when all resolved meanings are unchanged:

- moving a physical source and updating `[modules]`;
- renaming a logical module and updating every affected source locator;
- splitting or recombining declarations across modules;
- adding or changing comments and formatting;
- renaming a dependency alias and updating its uses;
- changing a vendored acquisition path to an artifact with the same
  `PackageId` and resolved semantic contents; and
- reordering manifest or lock tables.

They may change physical provenance, package integrity, or
`SourceRevisionId`.

The following change semantic identity:

- changing package name or compatibility version;
- renaming a public declaration;
- renaming a part composition path;
- changing a semantic field or constructor order;
- changing a statement or invariant order where order is semantic;
- changing any reachable type, constant, function, route, contract,
  configuration expression, or machine behavior; and
- changing a runtime-observable fault site path.

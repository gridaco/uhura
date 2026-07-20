# Relay B3 historical record

- **Status:** Completed, superseded implementation experiment
- **Authority:** Historical provenance only
- **Implemented result:** Uhura 0.3
- **Successor design:** [Uhura 0.4 incubation candidate](../0.4/)

Relay B3 was the disposable candidate that established the transactional
machine model implemented as Uhura 0.3:

```text
one admitted input
  -> one finite non-reentrant reaction
  -> one declared commit/abort outcome or fixed program fault
  -> atomic state and ordered command publication
  -> pure observation
```

The experiment was implemented directly in Uhura. Relay never became a
separate runtime, module tree, authored language, file extension, or product.
The checked-in Uhura 0.3 answers and implementation tests are the executable
record of its result.

The former multi-document candidate contract mixed pre-implementation gates,
post-implementation records, concrete syntax, application semantics, and
kernel claims under contradictory authority labels. It has been removed from
normal navigation. Git history retains the exact grammar, design review,
implementation record, and evidence when historical reconstruction is useful.

No current design inherits Relay's source tokens, file ordering, source-token
hash, monolithic application layout, or documentation topology. The successor
candidate restates every retained kernel property in source-neutral terms.

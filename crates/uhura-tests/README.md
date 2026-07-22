# Uhura acceptance tests

This workspace crate exercises the one maintained Uhura implementation across
its public boundaries. The canonical Instagram project must:

- resolve and check as one machine, 9 page presentations, 8 pure components,
  1 pure surface, one generated application entry, and 91 targeted examples;
- pass every evidence scenario;
- restore every example snapshot through the core runtime and project its
  checked page or direct component/surface target; and
- produce one host candidate with a current 91-preview Editor and admitted
  Play deployment.

The crate deliberately has no alternate runtime, fixture driver, or
version-specific golden corpus. Unit tests stay with their owning crates;
cross-crate acceptance belongs here.

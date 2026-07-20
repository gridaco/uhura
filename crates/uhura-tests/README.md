# Uhura acceptance tests

This workspace crate exercises the one maintained Uhura implementation across
its public boundaries. The canonical Instagram project must:

- parse and check as one machine, 18 UI presentations, and 91 targeted
  examples;
- pass every evidence scenario;
- restore every example snapshot through the core runtime and project it
  through the example's named presentation; and
- produce one host candidate with a current 91-preview Editor and admitted
  Play deployment.

The crate deliberately has no alternate runtime, fixture driver, or
version-specific golden corpus. Unit tests stay with their owning crates;
cross-crate acceptance belongs here.

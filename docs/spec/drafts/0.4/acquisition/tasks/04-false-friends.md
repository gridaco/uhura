# Task 04 — diagnose and repair false friends

Read the arm-specific `04-false-friends` worksheet. For all ten snippets,
write one object in `04-false-friends.json` using the exact worksheet ID:

```json
{
  "id": "worksheet ID",
  "problem": "the incorrect transferred assumption",
  "replacement": "the valid replacement or exact editing instruction"
}
```

Do not merely say that a snippet is invalid. State the language-model error and
give a replacement that preserves its stated intent. Do not introduce foreign
execution or move behavior into prose.

The arms intentionally probe different negative transfer. Rust-shaped Uhura
tests assumptions imported from Rust, Svelte, and older Uhura. Plain
TypeScript tests assumptions imported from permissive JavaScript and common
frontend execution patterns. Scores are reported by probe and are not treated
as token-for-token equivalent mistakes.

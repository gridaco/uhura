# Submission contract

The controller materializes each response as a directory. The participant may
return labeled code fences, but every label must map exactly to one required
file.

```text
RUN/
├── meta.json
├── first/
│   └── REQUIRED ARM FILES
├── diagnostics.json       # added by the adjudicator after first scoring
└── repair/                # optional until the one repair response exists
    └── REQUIRED ARM FILES
```

`meta.json`:

```json
{
  "protocol": "uhura-0.4-paper-acquisition/1",
  "run": "opaque-run-id",
  "arm": "rust",
  "phase": "paper",
  "model": "exact model identifier",
  "model_settings": "controller-supplied exact settings or unknown",
  "packet_sha256": "digest printed by prepare"
}
```

Use `"typescript"` for the control arm. The required filenames are printed by
`run.mjs prepare`.

`00-comprehension.json` is one JSON object whose keys are `C01` through `C10`
and whose values are short JSON strings.

`04-false-friends.json` is one JSON array of ten objects:

```json
[
  {
    "id": "F-R01",
    "problem": "short diagnosis",
    "replacement": "valid replacement or exact editing instruction"
  }
]
```

Use the IDs printed in the arm worksheet. Source files must contain complete
replacement source, including the supplied scaffold where a task says to
retain it. A repair response repeats the complete required file set so that it
can be inspected without applying a conversational patch.

The adjudicator supplies `diagnostics.json` after the first response. It is a
JSON array:

```json
[
  {
    "id": "D01",
    "file": "02-l1.uhura",
    "rubric": "S-L1-04",
    "message": "Ordered violations are not preserved on the two-violation path."
  }
]
```

Diagnostics identify a violated concept and location. They do not paste the
answer. Both arms receive one report and one repair opportunity.

Do not include chain-of-thought, repository observations, tool transcripts,
or files outside the required set. A short `notes` string may be added to
`meta.json` for an explicit ambiguity or language gap.

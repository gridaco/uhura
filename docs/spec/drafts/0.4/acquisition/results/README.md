# Acquisition results

Raw study results are not language semantics. Store an accepted run as:

```text
results/<study>/<run>/
├── submission/
│   ├── meta.json
│   ├── first/
│   ├── diagnostics.json
│   └── repair/
├── adjudication.json
└── score.json
```

Do not commit chain-of-thought, credentials, unrelated tool output, or private
participant information. A study README must record recruitment or model
selection, randomization, settings, exclusions, packet revision, and whether
the study is a pilot.

## Adjudication shape

The reviewer creates:

```json
{
  "protocol": "uhura-0.4-paper-acquisition/1",
  "run": "opaque-run-id",
  "arm": "rust",
  "comprehension": {
    "C01": true
  },
  "first": {
    "validity": {
      "V-L0": true
    },
    "semantic": {
      "S-L0-01": true
    },
    "falseFriends": {
      "F-R01": {
        "recognized": true,
        "repaired": true
      }
    },
    "authority": true
  },
  "repair": {
    "validity": {
      "V-L0": true
    },
    "semantic": {
      "S-L0-01": true
    },
    "falseFriends": {
      "F-R01": {
        "recognized": true,
        "repaired": true
      }
    },
    "authority": true,
    "initialDefects": ["D01"],
    "resolved": ["D01"],
    "remaining": [],
    "introduced": []
  }
}
```

Every key declared by the oracle must be present; the abbreviated example is
not itself valid. `resolved` and `remaining` must be a disjoint partition of
`initialDefects`. `introduced` contains new defects caused by repair.

Reviewers score semantic behavior, not textual similarity to an answer. Paper
validity means coherent under the supplied reference; it is not a claim that a
real parser accepted the source.

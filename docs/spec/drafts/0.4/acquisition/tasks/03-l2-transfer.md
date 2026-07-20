# Task 03 — complete correlated progress classification

Copy the complete arm-specific `03-l2` scaffold into `03-l2` with your arm's
source extension and replace every `TRIAL-TODO`.

The scaffold is a bounded slice of a keyed asynchronous supervisor. It contains
the full state required to classify one `progress(task, attempt, value)` input.
The full supervisor's scheduler runs only after an accepted input. This slice
emits no command itself.

Classification order:

1. unknown task, non-positive attempt, non-finite value, or value outside
   inclusive `0..1` is invalid;
2. an attempt greater than the task's greatest started attempt is invalid;
3. an attempt lower than the greatest started attempt is stale;
4. the greatest attempt is stale when the task is no longer running;
5. for the current running attempt, a lower value is stale, an equal value is
   duplicate, and a greater value is accepted.

Only accepted progress changes state. It preserves task identity, the greatest
started attempt, and the running correlation, replacing only the current
progress. Duplicate, stale, and invalid results publish the original state,
no commands, and no opportunistic scheduling.

Value validity precedes correlation age. Thus an out-of-range value attached to
an old attempt is invalid rather than stale. A value of `1` remains running;
progress never implies completion.

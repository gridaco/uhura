# Task 00 — semantic comprehension

Answer `C01` through `C10` in `00-comprehension.json`. Use one short sentence
or compact data value rendered inside a JSON string per answer. No source code
is required.

`C01`. A bounded counter is already at its configured maximum and receives its
declared increment input. Is the step accepted or rejected, and does the state
change?

`C02`. A counter configuration has `minimum > initial`. When is it rejected
relative to state initialization?

`C03`. In a crossing program, a named passenger is not on the current side of
the operator. Should the program construct and safety-check a tentative
crossing before refusing it?

`C04`. A tentative crossing violates relationship A and relationship B, whose
declared canonical order is A then B. What ordered refusal payload is
required?

`C05`. A normalized report carries an out-of-range value and refers to a
previously started but old correlation. Is the report invalid or stale when
value validity has declared precedence over correlation age?

`C06`. A report for the current attempt repeats its current normalized value.
What classification and publication behavior are required?

`C07`. Cancelling running work frees a slot and starts the oldest queued work
in the same step. In what order are the two commands published?

`C08`. A reaction buffers a command and changes draft state, then selects an
abort-policy result. What state and commands are published?

`C09`. Source contains `part notice = Notice();`. Does this allocate or
schedule a child runtime object?

`C10`. A host begins work after receiving a published command. May its report
synchronously re-enter the still-running reaction?

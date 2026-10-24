# Routing XTest replacement-focus readiness candidate

The reviewed combined batch at
`bf799a968b1cd6a079fd0d066055b1e17d62d8d5` passes its local native aggregate,
but PR workflow [40139675385](https://github.com/jee-mj/pecanmixer/actions/runs/40139675385)
fails one GTK case with 343 passing. The failed assertion is replacement-button
`grab_focus()` in `xtest_held_route_release_replacement_and_return_failure_repeat_require_fresh_activation`,
after Space release, graph replacement and a fixed 50 ms settling interval.
Core/backend checks pass. This does not establish a production audio failure.

The unchanged isolated case passes twelve diagnostic executions on the cloud
GTK environment; the hosted failure is not reproduced locally. The candidate
therefore addresses a fixture precondition without claiming the hosted cause is
proven. The next actual hosted run and independent review remain required.

The test now awaits explicit fresh-input readiness: no held/cancelled input,
a mapped and sensitive replacement, and positive allocation. It pumps only the
synthetic GTK context with a two-second bound, then attempts focus once. A stuck
ledger or permanently disabled/unmapped control fails with those state values;
a ready widget rejecting focus still fails. No focus/activation retry, additional
key event, skipped test, workflow retry or suppressed assertion is introduced.
All held-release, fixed off/on intent, Return autorepeat and fresh-activation
assertions remain. No application/input policy or production dependency changes.

The corrected isolated case passes twelve local executions. All 344 all-feature GTK tests pass locally, with the existing XTest and isolated
D-Bus assertions retained. Changed-file formatting, whitespace and test-only
source/privacy scope checks pass. Core/backend/default-only suites
and native build are not repeated for this test-only delta; their preceding
combined-tree results remain baseline evidence. No hardware, real desktop,
audio, profile, performance or actual recording test is performed.

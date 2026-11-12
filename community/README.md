# Community recipes

Recipes anyone can submit, judged on evidence: the session reports Unilux
writes after each play session. Nothing here is hand-curated.

## Submitting

1. Make a recipe (`unilux recipe export recommended > mine.toml`, edit,
   `unilux recipe import mine.toml`) and play with it in VR through Unilux.
   Each session leaves a report: `unilux session list`.
2. `unilux recipe verdict mine` shows where it stands, by the same rules as
   below.
3. `unilux recipe share mine --open` opens the GitHub form, filled in. No
   developer account or git needed.

A bot checks the recipe, verifies each report against it, comments the
verdict on the issue and opens a pull request adding
`recipes/<id>/recipe.toml` and `recipes/<id>/reports/*.json`. More reports
for the same recipe go through the same form.

## What a report holds

Recipe id and key (the hash of the recipe, its Proton and the builder),
GPU model and driver, CPU model, RAM, distribution, kernel, VR runtime,
session length, worlds entered, sessions joined, video opens and failures,
crashes (in game or on exit), and MangoHud frame pacing when it is
installed (average fps, 1 % low, p99 frame time, stutters). No names,
paths or account data.

## Rules

- A report counts only if its key matches the submitted recipe (same
  recipe text, same builder) and it was a VR session.
- **proposed**: at least one valid VR session of 5+ minutes (shorter
  sessions still count for crashes and videos).
- **community**: at least 3 sessions of 5+ minutes, from at least 2
  configurations (GPU, distribution, VR runtime), 1+ hour in total, at most
  1 in-game crash per 4 hours, and 90 %+ of videos playing once 5+ were
  opened. Crashes on exit (Unity's handler during shutdown) are listed but
  do not count.
- Frame rates are not compared across machines; the index shows pacing
  (1 % low / average) instead, where 1.0 is perfectly even.

The thresholds live in `crates/unilux-patches/src/community.rs`; Unilux and
the bot run the same code.

## Review

Recipes made only of library ingredients and environment variables need
no code review. A recipe with its own patches (`custom patches` in
listings) is Wine code from someone else: its pull request is labelled
`custom-code` and waits for a maintainer to read the patches.

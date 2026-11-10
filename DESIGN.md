# AASL design system

The app has one job per screen. Home has one primary action: Play. Everything else (components, settings, diagnostics, logs, agents) lives one click away and never competes with it.

## Layout

Three screens, modelled on the An Anime Team launchers:

- **Launcher**: a full-window hero picture (`crates/aasl-gui/assets/hero.jpg`, rendered from `assets/hero.svg`), brand top left, Settings top right, and one frosted bar at the bottom with VR, Proton and patch status on the left and the actions on the right. Play is the only filled button.
- **First run**: the same hero behind one centred glass card that walks through welcome, system check, Proton choice, Somnium's installer, progress and done.
- **Settings**: a sidebar of sections (General, Proton, Patches, VR, Diagnostics, Logs, Agents); each section is grouped rows with a title, a subtitle and one trailing control.

## Tone

Utilitarian, calm, exact. The app reads like a good instrument panel: state first, then the action that changes it.

## Tokens

All colours and type go through `crates/aasl-gui/src/theme.rs`. No literal colour appears in a component.

| Token | Dark | Light | Use |
|---|---|---|---|
| `bg` | #121417 | #f6f5f2 | window background |
| `surface` | #1b1e23 | #ffffff | panels |
| `border` | #2b3038 | #dcd9d2 | separators, input outlines |
| `text` | #e9e7e2 | #1a1c1f | body text |
| `muted` | #9a9da3 | #62656b | secondary text |
| `accent` | #e8a33d | #b36b00 | the primary action, focus ring |
| `glass` | #14161b at 72% | | panels over the hero, with a 24 px backdrop blur |
| `ok` | #6cc28a | #2f7d4a | healthy state |
| `warn` | #e8c33d | #8a6d00 | needs attention |
| `error` | #e5655b | #b3261e | failure |

Type: one family (Inter, system sans fallback). Sizes 13 / 15 / 18 / 24 / 44. Body never below 15. Headings use a single weight (600).

Spacing scale: 4, 8, 12, 16, 24, 32, 48. Radius: 6 for controls, 10 for panels, never a panel inside a panel.

## Controls

Every control has these states: default, hover, pressed, focused, disabled, loading, success, error. Focus shows a 2 px `accent` ring right away, with no fade. Disabled buttons say why in a tooltip ("Connect your headset first").

## Motion

Only opacity and transform, 120 to 200 ms. When the desktop asks for reduced motion, no animation runs. Progress bars move; nothing else moves on its own.

## Copy rules

- Labels in sentence case, never all caps, never tracked.
- State the fact, then the fix: "No OpenXR runtime found. Start SteamVR or WiVRn, then connect the headset."
- No invented numbers, quotes or user counts.
- No exclamation marks in errors. No "Oops".
- Every string goes through Fluent (`crates/aasl-gui/i18n/{en,fr}`).

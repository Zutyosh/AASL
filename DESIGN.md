# Unilux design system

The app has one job per screen. Home has one primary action: Play. Everything else (components, settings, diagnostics, logs, agents) lives one click away and never competes with it.

## Layout

Three screens, modelled on the An Anime Team launchers:

- **Launcher**: Somnium's official key art full-bleed (downloaded from somniumspace.com at run time and cached in `~/.cache/unilux/background.jpg`, never shipped), a small brand top left, a frosted status card bottom left (VR, Proton, patches) and the actions bottom right: frosted pills for Repair and Desktop mode, a filled Play pill, a round Settings button. Play is the only filled control.
- **First run**: the key art dimmed to 30 %, no card: logo, heading, one sentence and one pill, centred like a libadwaita status page. Pages: welcome (system problems listed inline), Proton (verified builds only), installer source (download by default; drop zone and file picker as the quiet alternative), loading screen (breathing logo inside a spinner, the current step, a rotating line, one overall progress bar), done.
- **Settings**: an opaque sidebar of sections (General, Recipes, VR, Diagnostics, Logs, Agents); each section is grouped rows with a title, a subtitle and one trailing control. Recipes has tabs for the recipes, creating one, Proton versions, sessions and community recipes. Proton versions lists every provider as pills (GE-Proton, GE-Proton RTSP, Proton-CachyOS, Proton-EM, Proton-Sarek) with their releases. A build covers everything with its own screen until it ends or goes to the background.

## Tone

Utilitarian, calm, exact. The app reads like a good instrument panel: state first, then the action that changes it.

## Tokens

All colours and type go through `crates/unilux-gui/src/theme.rs`. No literal colour appears in a component.

| Token | Value | Use |
|---|---|---|
| `bg` | #111113 | window background |
| `surface` | #19191c | sidebar, row groups |
| `raised` | #242428 | selected item, toasts |
| `border` | #303035 | separators, input outlines |
| `text` | #ececef | body text |
| `muted` | #9b9ba4 | secondary text |
| `accent` | #b69aff | selection, toggles, progress, focus ring, logo; primary buttons are off-white `text` on `bg` |
| `glass` | `bg` at 62 % | panels over the key art, 30 px backdrop blur |
| `ok` | #7cd39a | healthy state |
| `warn` | #f0c65a | needs attention |
| `error` | #f27f7f | failure |

Dark only. The accent was picked from Somnium's key art (violet night, pink lights).

Type: one family (Inter, system sans fallback). Sizes 13 / 15 / 18 / 26 / 36. Body never below 15. Two weights: 400 and 500; hierarchy comes from size and colour. Group headings are small muted labels.

Icons: one line set (lucide shapes, `widgets::Icon`), 1.5 stroke, coloured like their text. Neutrals are graphite; Somnium's violet lives in the key art and the accent, not in surfaces. Status banners stay neutral and carry their tone in a dot.

Spacing scale: 4, 8, 12, 16, 24, 32, 48. Radius: 10 for controls, 16 for panels, pills for the main actions; never a panel inside a panel.

App icon: `assets/unilux-logo.svg`, a crescent holding a play mark on a dark squircle, two flat colours. PNGs are rendered from it with `rsvg-convert`. It is used for the desktop icon and inside the app, not in the README.

## Controls

Every control has these states: default, hover, pressed, focused, disabled, loading, success, error. Focus shows a 2 px `accent` ring right away, with no fade. Disabled buttons say why in a tooltip ("Connect your headset first").

## Motion

Only opacity and transform, 120 to 200 ms. When the desktop asks for reduced motion, no animation runs. Progress bars move; nothing else moves on its own.

## Copy rules

- Labels in sentence case, never all caps, never tracked.
- State the fact, then the fix: "No OpenXR runtime found. Start SteamVR or WiVRn, then connect the headset."
- No invented numbers, quotes or user counts.
- No exclamation marks in errors. No "Oops".
- Every string goes through `crates/unilux-gui/src/i18n.rs` (English and French).

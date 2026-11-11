# Wine patches and compatibility recipes for Somnium Space

The patches fix Wine bugs that Somnium Space triggers. Most come from
[Somnilux](https://github.com/Vulps22/somnilux) by Vulps22 and keep
Wine's license, LGPL-2.1 (see `LICENSE` in this folder).
`SOMNILUX_BASE_COMMIT.txt` is Somnilux's original provenance note.

## Layout

- `ingredients.toml`: the ingredient library. Each ingredient is a typed,
  declarative fix: Wine patches (`kind = "patch"`) with the tree they
  build on and the DLLs they change, or launch variables (`kind = "env"`).
- `recipes/`: built-in recipes. A recipe is a Proton (pinned, `latest`,
  or the one picked in AASL) plus ingredient ids in order.
- `wine/`, `ge/`: the patch files.

Everything here is compiled into AASL (`crates/aasl-patches/build.rs`), so
adding a file needs no code change. `AASL_PATCHES_DIR=<this folder>` makes
a development build read it live.

| Ingredient | Built from | Produces |
|---|---|---|
| `tls-client-certs` | upstream Wine at `upstream_commit` | secur32, crypt32, rsaenh |
| `media-by-url` | the Proton's own Wine, prepared like its release | winedmo |
| `ytdlp-hook` | the Proton's own Wine | kernelbase |
| `ytdlp-android-client` | nothing (environment) | `PROTON_YTDLP_EXTRACTOR_ARGS` |

## Recipes

```sh
aasl recipe list                     # * marks the recipe in use
aasl recipe export recommended > mine.toml   # self-contained, patches inlined
# edit id, title, ingredients; add [[ingredient]] tables for new fixes
aasl recipe check mine.toml
aasl recipe import mine.toml
aasl recipe build mine               # podman/docker, cached by content
aasl recipe use mine
```

Recipes carry no scripts: unknown keys are refused, patch names cannot
leave their folder, and patches that are not in this library are flagged
as custom code wherever the recipe is shown. A bundle is identified by the
hash of its Proton, its ingredients and the builder, so identical recipes
share one build.

Published bundles in `data/catalog.toml` are builds of the `recommended`
recipe; other recipes always use a bundle built for them.

## How a build works

- All `base = "proton"` ingredients share the Proton's Wine tree. For
  GE-Proton it is prepared exactly like GE's release: the tag's own
  `patches/protonprep-valve-staging.sh` (wine-staging, game fixes,
  hotfixes, video rework); older tags fall back to the video rework alone.
- `superseded_if` skips an ingredient the Proton already carries in its
  own form. Since GE-Proton11-12 (`0078-winedmo-open-http-and-rtsp-media-asynchronously`
  and friends), `media-by-url` is skipped there.
- Only DLLs the applied patches changed are shipped from a Proton tree; GE's
  own `winedmo` and ffmpeg stay when nothing touched them.
- The builder stops at the first patch that does not apply and names it,
  so a build doubles as a test of whether a new Proton still needs or
  still accepts an ingredient.
- `ge/0005` is AASL's own: the yt-dlp options went after Somnium's `--`,
  so every YouTube video failed with `'--extractor-args' is not a valid URL`.

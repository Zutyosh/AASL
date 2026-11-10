# Wine patches for Somnium Space

These patches fix Wine bugs that Somnium Space triggers. They come from
[Somnilux](https://github.com/Vulps22/somnilux) by Vulps22 and keep
Wine's license, LGPL-2.1 (see `LICENSE` in this folder). AASL builds and
installs them; it does not change them without saying so here.

| Series | Folder | Built from | Produces |
|---|---|---|---|
| `tls` | `wine/` | upstream Wine (commit in `series.toml`) | secur32, crypt32, rsaenh |
| `media` | `ge/` | the selected Proton's own Wine, after GE's video rework | winedmo, kernelbase |

`series.toml` is the machine-readable version of this table. `aasl patch
build --proton <provider:tag>` builds a bundle for any Proton; see
`tools/patch-builder/`. `SOMNILUX_BASE_COMMIT.txt` is Somnilux's original
provenance note.

Adding a patch: drop a `git format-patch` file into the right folder,
keep the numbering, and run a build. The builder stops at the first patch
that does not apply and prints which one, so it doubles as a test of
whether a new Proton still needs or still accepts a patch.

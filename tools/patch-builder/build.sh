#!/usr/bin/env bash
# Build an AASL patch bundle for one Proton build.
#
# Adapted from Somnilux's scripts/build-artifacts.sh (MIT, Vulps22). The
# differences: every input comes from the environment (aasl resolves them),
# the Proton tree is fetched for the requested tag instead of a local
# checkout, and the series list decides what gets built.
#
# Inputs (environment):
#   PROVIDER PROTON_TAG PROTON_REPO   which Proton, e.g. ge GE-Proton11-12
#   WINE_REPO WINE_COMMIT             that Proton's wine submodule
#   SERIES                            space-separated series ids from series.toml
#   PATCHES_DIR WORK_DIR OUT_DIR      directories
#   ARCHIVE_FORMAT BUNDLE_NAME        output stamp and file name
#   WITH_FFMPEG=1                     also build ffmpeg with dav1d (slow)
set -euo pipefail

: "${PROVIDER:?}" "${PROTON_TAG:?}" "${PROTON_REPO:?}" "${WINE_REPO:?}" "${WINE_COMMIT:?}"
: "${SERIES:?}" "${PATCHES_DIR:?}" "${WORK_DIR:?}" "${OUT_DIR:?}" "${ARCHIVE_FORMAT:?}" "${BUNDLE_NAME:?}"
WITH_FFMPEG="${WITH_FFMPEG:-0}"
JOBS="${JOBS:-$(nproc)}"
FFMPEG_VERSION="8.1"
STAGE="$WORK_DIR/stage"

say()  { printf '\n== %s\n' "$*"; }
info() { printf '   %s\n' "$*"; }
die()  { printf '\nFATAL %s\n' "$*" >&2; exit 1; }

# series_field id key -> value (tiny reader for our own series.toml)
series_field() {
    awk -v id="$1" -v key="$2" '
        /^\[\[series\]\]/ { cur = "" }
        $1 == "id" { gsub(/"/, "", $3); cur = $3 }
        cur == id && $1 == key { sub(/^[^=]*= */, ""); gsub(/[\[\]"]/, ""); gsub(/, */, " "); print; exit }
    ' "$PATCHES_DIR/series.toml"
}

fetch_commit() { # dir repo commit
    local dir="$1" repo="$2" commit="$3"
    if [ ! -d "$dir/.git" ]; then
        git init -q "$dir"
        git -C "$dir" remote add origin "$repo"
    fi
    if [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)" = "$commit" ]; then
        return 0
    fi
    # Already fetched (a previous build's patches sit on top): go back to it,
    # keeping the build directory so make stays incremental.
    if git -C "$dir" cat-file -e "$commit^{commit}" 2>/dev/null; then
        git -C "$dir" checkout -q -f "$commit"
        return 0
    fi
    info "fetching $repo @ $commit"
    git -C "$dir" fetch -q --depth 1 origin "$commit"
    git -C "$dir" checkout -q -f FETCH_HEAD
    git -C "$dir" clean -qfdx
}

# superseded tree list patch -> true when <list> says <patch> is already in <tree>
superseded() {
    local tree="$1" list="$2" name="$3" p cond file needle
    [ -f "$list" ] || return 1
    while read -r p cond file needle; do
        [ "$p" = "$name" ] || continue
        case "$cond" in
            has)   grep -qF -- "$needle" "$tree/$file" 2>/dev/null && return 0 ;;
            lacks) [ -f "$tree/$file" ] && ! grep -qF -- "$needle" "$tree/$file" && return 0 ;;
        esac
    done < <(grep -v '^[[:space:]]*\(#\|$\)' "$list")
    return 1
}

apply_series_patches() { # tree dir
    local tree="$1" dir="$2" p
    git -C "$tree" config user.email aasl@localhost
    git -C "$tree" config user.name aasl
    for p in "$PATCHES_DIR/$dir"/*.patch; do
        if superseded "$tree" "$PATCHES_DIR/$dir/superseded" "$(basename "$p")"; then
            info "skip $(basename "$p"): this Proton already carries the fix"
            continue
        fi
        info "apply $(basename "$p")"
        git -C "$tree" am -q --3way "$p" || { git -C "$tree" am --abort || true; die "$(basename "$p") does not apply to this tree"; }
    done
}

# GE-Proton applies its media rework on top of Valve's Wine; the media
# series patches code that only exists after it.
prep_ge_video_rework() { # tree
    local tree="$1" ge="$WORK_DIR/proton-$PROTON_TAG" p failed=()
    fetch_commit "$ge" "$PROTON_REPO" "$(git ls-remote "$PROTON_REPO" "refs/tags/$PROTON_TAG" | cut -f1)"
    [ -d "$ge/patches/ge-video-rework" ] || die "$PROTON_TAG has no patches/ge-video-rework"
    for p in "$ge"/patches/ge-video-rework/*.patch; do
        if ( cd "$tree" && patch -Np1 --forward -s < "$p" >/dev/null ); then
            info "rework $(basename "$p")"
        else
            failed+=("$(basename "$p")")
        fi
    done
    # Register winedmo instead of winegstreamer (the one hunk that never
    # applies cleanly, per Somnilux's notes).
    sed -i 's/winegstreamer\.dll/winedmo.dll/g' "$tree/loader/wine.inf.in" || true
    [ "${#failed[@]}" -eq 0 ] || info "rework patches that did not apply (usually harmless): ${failed[*]}"
    git -C "$tree" add -A >/dev/null && git -C "$tree" -c user.email=aasl@localhost -c user.name=aasl commit -qm "GE video rework" || true
}

# GE-Proton's full Wine preparation: the same wine-staging, game fixes,
# hotfixes and video rework its release was built with, taken from the
# tag's own patches/protonprep-valve-staging.sh. DLLs built on this tree
# match the ones GE ships, plus our patches; the rework-only tree lacked
# everything staging adds (kernelbase included).
prep_ge_full() { # tree
    local tree="$1" ge="$WORK_DIR/proton-$PROTON_TAG" script="$WORK_DIR/ge-wine-prep.sh" c url rc=0
    fetch_commit "$ge" "$PROTON_REPO" "$(git ls-remote "$PROTON_REPO" "refs/tags/$PROTON_TAG" | cut -f1)"
    local prep="$ge/patches/protonprep-valve-staging.sh"
    if [ ! -f "$prep" ]; then
        info "$PROTON_TAG has no protonprep-valve-staging.sh; applying the video rework only"
        prep_ge_video_rework "$tree"
        return
    fi
    c=$(git -C "$ge" ls-tree HEAD wine-staging | awk '{print $3}')
    url=$(git -C "$ge" config -f .gitmodules submodule.wine-staging.url)
    [ -n "$c" ] && [ -n "$url" ] || die "$PROTON_TAG: cannot find its wine-staging submodule"
    fetch_commit "$ge/wine-staging" "$url" "$c"
    # The script reverts upstream commits; a shallow tree needs them and their parents.
    for c in $(grep -o 'git revert --no-commit [0-9a-f]*' "$prep" | awk '{print $4}'); do
        git -C "$tree" fetch -q --depth 2 origin "$c" || die "cannot fetch $c to revert it"
    done
    {
        printf 'apply_patch() { patch -Np1 < "$1"; }\n'
        printf 'apply_all_in_dir() { local p; for p in "$1"/*.patch; do apply_patch "$p"; done; }\n'
        sed -n '/^### (2) WINE PATCHING ###/,/^### END WINE PATCHING ###/p' "$prep"
    } > "$script"
    info "GE wine preparation (staging, game fixes, video rework), log: ge-wine-prep.log"
    rm -rf "$ge/wine" && mv "$tree" "$ge/wine"
    ( cd "$ge" && bash "$script" ) > "$WORK_DIR/ge-wine-prep.log" 2>&1 || rc=$?
    mv "$ge/wine" "$tree" && mkdir -p "$ge/wine"
    [ "$rc" -eq 0 ] || info "GE preparation exited with $rc (GE's own script tolerates failing hunks)"
    c=$(grep -c '^Hunk .*FAILED\|^[0-9]* out of [0-9]* hunks* FAILED' "$WORK_DIR/ge-wine-prep.log" || true)
    info "GE preparation done; failed hunks: $c"
    git -C "$tree" add -A >/dev/null && git -C "$tree" -c user.email=aasl@localhost -c user.name=aasl commit -qm "GE wine preparation" || true
}

generate_proton_sources() { # tree
    local tree="$1"
    ( cd "$tree" && autoreconf -f >/dev/null 2>&1 ) || die "autoreconf failed"
    ( cd "$tree" && ./tools/make_requests >/dev/null ) || die "make_requests failed"
    ( cd "$tree" && ./tools/make_specfiles >/dev/null ) || die "make_specfiles failed"
    if [ -f "$tree/dlls/winevulkan/make_vulkan" ] && [ ! -f "$tree/include/wine/vulkan.h" ]; then
        ( cd "$tree/dlls/winevulkan" && python3 ./make_vulkan >/dev/null ) || die "make_vulkan failed"
    fi
}

build_dlls() { # tree dll...
    local tree="$1"; shift
    mkdir -p "$tree/build"
    [ -f "$tree/build/Makefile" ] || ( cd "$tree/build" && ../configure --enable-win64 --without-x --without-wayland >/dev/null ) || die "configure failed"
    local d
    for d in "$@"; do
        info "make dlls/$d"
        make -j"$JOBS" -C "$tree/build/dlls/$d" >/dev/null || die "build failed: $d"
    done
}

# Debug info doubles the DLLs; Proton ships them stripped, so do the same.
stage() {
    install -Dm644 "$1" "$2"
    case "$2" in
        *.dll) x86_64-w64-mingw32-strip --strip-debug "$2" ;;
        *.so*) strip --strip-unneeded "$2" ;;
    esac
    info "$(printf '%10d  %s' "$(stat -c%s "$2")" "${2#$STAGE/}")"
}

# touched tree rev name... -> the names whose dlls/<name>/ changed since rev
touched() {
    local tree="$1" rev="$2" d; shift 2
    for d in "$@"; do
        git -C "$tree" diff --quiet "$rev" HEAD -- "dlls/$d/" || printf '%s ' "$d"
    done
}

build_series() { # id
    local id="$1" dir base dlls unix tree d
    dir=$(series_field "$id" dir); base=$(series_field "$id" base)
    dlls=$(series_field "$id" dlls); unix=$(series_field "$id" unix_libs)
    [ -n "$dir" ] || die "unknown series $id"
    say "Series $id ($base)"
    if [ "$base" = upstream ]; then
        local commit; commit=$(series_field "$id" upstream_commit)
        # Named by commit: recipes sharing an upstream base share its build.
        tree="$WORK_DIR/wine-upstream-${commit:0:12}"
        fetch_commit "$tree" "$(series_field "$id" upstream_repo)" "$commit"
    else
        tree="$WORK_DIR/wine-$PROVIDER-$PROTON_TAG"
        [ -s "$tree/.aasl-prepared" ] || rm -f "$tree/.aasl-prepared"
        if [ ! -f "$tree/.aasl-prepared" ]; then
            fetch_commit "$tree" "$WINE_REPO" "$WINE_COMMIT"
            if [ "$(series_field "$id" requires_prep)" = ge-video-rework ]; then
                if [ "$PROVIDER" = ge ]; then
                    prep_ge_full "$tree"
                elif [ ! -f "$tree/dlls/winedmo/unix_demuxer.c" ]; then
                    die "series $id needs the winedmo media backend, which $PROVIDER:$PROTON_TAG's Wine does not have. Build with SERIES=tls only."
                fi
            fi
            git -C "$tree" rev-parse HEAD > "$tree/.aasl-prepared"
        fi
        # Start each build from the prepared tree, not from a previous run's patches.
        git -C "$tree" reset -q --hard "$(cat "$tree/.aasl-prepared")"
    fi
    local before; before=$(git -C "$tree" rev-parse HEAD)
    apply_series_patches "$tree" "$dir"
    if [ "$base" = proton ]; then
        # Ship only what the patches changed: when this Proton already
        # carries a fix, its own build of that DLL is newer than ours.
        dlls=$(touched "$tree" "$before" $dlls); unix=$(touched "$tree" "$before" $unix)
        [ -n "$dlls" ] || { info "nothing to build: this Proton already has every fix of $id"; return 0; }
    fi
    [ "$base" = proton ] && generate_proton_sources "$tree"
    # shellcheck disable=SC2086
    build_dlls "$tree" $dlls
    for d in $dlls; do stage "$tree/build/dlls/$d/x86_64-windows/$d.dll" "$STAGE/x86_64-windows/$d.dll"; done
    for d in $unix; do stage "$tree/build/dlls/$d/$d.so" "$STAGE/x86_64-unix/$d.so"; done
}

build_ffmpeg() {
    say "ffmpeg $FFMPEG_VERSION with dav1d"
    local src="$WORK_DIR/ffmpeg-$FFMPEG_VERSION" dst="$WORK_DIR/ffmpeg-dst" lib f
    [ -d "$src" ] || curl -fsSL "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" | tar -xJ -C "$WORK_DIR"
    [ -f "$src/ffbuild/config.mak" ] || ( cd "$src" && ./configure --prefix="$dst" --libdir="$dst/lib" \
        --arch=x86_64 --target-os=linux --enable-shared --disable-static --disable-everything --disable-programs \
        --disable-doc --disable-inline-asm --enable-lzma --enable-bzlib --enable-gnutls --enable-demuxers \
        --enable-protocol=https --enable-decoders --enable-encoders --enable-bsfs --disable-decoder=vvc \
        --enable-libdav1d >/dev/null ) || die "ffmpeg configure failed"
    make -j"$JOBS" -C "$src" >/dev/null && make -C "$src" install >/dev/null || die "ffmpeg build failed"
    for f in avcodec avformat avutil swscale swresample; do
        lib=$(ls "$dst/lib/lib$f.so."*.* | head -1)
        stage "$lib" "$STAGE/x86_64-linux-gnu/$(basename "$lib")"
    done
}

main() {
    mkdir -p "$WORK_DIR" "$OUT_DIR"
    rm -rf "$STAGE"; mkdir -p "$STAGE"
    say "AASL patch bundle for $PROVIDER:$PROTON_TAG (wine $WINE_COMMIT)"
    local s
    for s in $SERIES; do build_series "$s"; done
    [ "$WITH_FFMPEG" = 1 ] && build_ffmpeg
    printf '%s\n' "$ARCHIVE_FORMAT" > "$STAGE/archive-format.txt"
    cat > "$STAGE/manifest.json" <<EOF
{"proton":"$PROVIDER:$PROTON_TAG","wine_repo":"$WINE_REPO","wine_commit":"$WINE_COMMIT","series":"$SERIES","built_at":"$(date -u +%FT%TZ)"}
EOF
    tar -czf "$OUT_DIR/$BUNDLE_NAME" -C "$STAGE" .
    ( cd "$OUT_DIR" && sha256sum "$BUNDLE_NAME" > "$BUNDLE_NAME.sha256" )
    say "Done: $OUT_DIR/$BUNDLE_NAME"
}

main "$@"

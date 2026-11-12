#!/bin/sh
# Build Unilux-x86_64.AppImage from a release build.
# Needs appimagetool on PATH: https://github.com/AppImage/appimagetool
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/target/appimage"
appdir="$out/Unilux.AppDir"

cargo build --release --manifest-path "$root/Cargo.toml" -p unilux-cli -p unilux-gui

rm -rf "$appdir"
mkdir -p "$appdir/usr/bin"
cp "$root/target/release/unilux" "$root/target/release/unilux-gui" "$appdir/usr/bin/"
cp "$root/packaging/unilux.desktop" "$appdir/unilux.desktop"
cp "$root/assets/unilux-256.png" "$appdir/unilux.png"
mkdir -p "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/icons/hicolor/scalable/apps"
cp "$root/assets/unilux-256.png" "$appdir/usr/share/icons/hicolor/256x256/apps/unilux.png"
cp "$root/assets/unilux-logo.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/unilux.svg"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
export PATH="$here/usr/bin:$PATH"
# Any argument means the command line (unilux --help lists them); none
# opens the app.
if [ "$#" -gt 0 ]; then
    exec "$here/usr/bin/unilux" "$@"
fi
exec "$here/usr/bin/unilux-gui"
EOF
chmod +x "$appdir/AppRun"

rm -f "$out/Unilux-x86_64.AppImage"
ARCH=x86_64 appimagetool --no-appstream "$appdir" "$out/Unilux-x86_64.AppImage"
echo "$out/Unilux-x86_64.AppImage"

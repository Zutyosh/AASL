#!/bin/sh
# Build AASL-x86_64.AppImage from a release build.
# Needs appimagetool on PATH: https://github.com/AppImage/appimagetool
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/target/appimage"
appdir="$out/AASL.AppDir"

cargo build --release --manifest-path "$root/Cargo.toml" -p aasl-cli -p aasl-gui

rm -rf "$appdir"
mkdir -p "$appdir/usr/bin"
cp "$root/target/release/aasl" "$root/target/release/aasl-gui" "$appdir/usr/bin/"
cp "$root/packaging/aasl.desktop" "$appdir/aasl.desktop"
cp "$root/assets/aasl-256.png" "$appdir/aasl.png"
mkdir -p "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/icons/hicolor/scalable/apps"
cp "$root/assets/aasl-256.png" "$appdir/usr/share/icons/hicolor/256x256/apps/aasl.png"
cp "$root/assets/aasl-logo.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/aasl.svg"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
export PATH="$here/usr/bin:$PATH"
# Any argument means the command line (aasl --help lists them); none
# opens the app.
if [ "$#" -gt 0 ]; then
    exec "$here/usr/bin/aasl" "$@"
fi
exec "$here/usr/bin/aasl-gui"
EOF
chmod +x "$appdir/AppRun"

rm -f "$out/AASL-x86_64.AppImage"
ARCH=x86_64 appimagetool --no-appstream "$appdir" "$out/AASL-x86_64.AppImage"
echo "$out/AASL-x86_64.AppImage"

#!/bin/sh
# Assemble une archive autonome dillorust (Linux/Unix).
# Usage : ./packaging/package.sh   (depuis la racine du dépôt)
set -eu

cd "$(dirname "$0")/.."
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/dillo/Cargo.toml | head -1)
ARCH=$(uname -m)
[ "$ARCH" = "x86_64" ] || ARCH="$ARCH"
NAME="onionbit-browser-${VER}-linux-${ARCH}"
OUT="dist/$NAME"

cargo build --release --workspace

rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/dpi" "$OUT/etc/dillo"

cp target/release/onionbit-browser target/release/dilloc "$OUT/bin/"
cp target/release/dillo-dpid "$OUT/bin/dpid"
cp target/release/dillo-dpidc "$OUT/bin/dpidc"

# Layout dpid : dpi/<service>/<service>.dpi (install-dpi-local amont)
for svc in bookmarks cookies downloads file ftp datauri vsource hello; do
    mkdir -p "$OUT/dpi/$svc"
    case $svc in
        ftp|datauri|vsource|hello) leaf="$svc.filter.dpi" ;;
        *)                         leaf="$svc.dpi" ;;
    esac
    cp "target/release/dpi-$svc" "$OUT/dpi/$svc/$leaf"
done

cp packaging/dillorc "$OUT/etc/dillo/dillorc"
printf '# dpidrc — dpi_dir système (adapter au chemin d\x27installation)\n# dpi_dir=%s\n' "/opt/$NAME/dpi" > "$OUT/etc/dillo/dpidrc"
cp packaging/README.md "$OUT/README.txt"

mkdir -p dist
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "dist/$NAME.tar.gz"

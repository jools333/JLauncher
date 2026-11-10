#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
BINARY_PATH="${2:-target/x86_64-unknown-linux-gnu/release/jlauncher}"

if [ -z "$VERSION" ]; then
    VERSION=$(grep -m1 '^version' Cargo.toml | cut -d '"' -f2)
fi

echo "=== Building JLauncher .deb package (v${VERSION}) ==="

if [ ! -f "$BINARY_PATH" ]; then
    if [ -f "target/release/jlauncher" ]; then
        BINARY_PATH="target/release/jlauncher"
    elif [ -f "./jlauncher" ]; then
        BINARY_PATH="./jlauncher"
    else
        echo "Error: Binary not found at $BINARY_PATH"
        exit 1
    fi
fi

STAGING_DIR="build/deb-staging"
rm -rf "$STAGING_DIR"

# Directory structure
mkdir -p "$STAGING_DIR/DEBIAN"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$STAGING_DIR/usr/share/doc/jlauncher"
mkdir -p "target/installer"

# Install binary
cp "$BINARY_PATH" "$STAGING_DIR/usr/bin/jlauncher"
chmod 755 "$STAGING_DIR/usr/bin/jlauncher"

# Install icons & desktop file
if [ -f "assets/icon.png" ]; then
    cp "assets/icon.png" "$STAGING_DIR/usr/share/icons/hicolor/256x256/apps/jlauncher.png"
fi
if [ -f "assets/jlauncher.svg" ]; then
    cp "assets/jlauncher.svg" "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/jlauncher.svg"
fi
if [ -f "assets/jlauncher.desktop" ]; then
    cp "assets/jlauncher.desktop" "$STAGING_DIR/usr/share/applications/jlauncher.desktop"
    chmod 644 "$STAGING_DIR/usr/share/applications/jlauncher.desktop"
fi

# Documentation
if [ -f "README.md" ]; then
    cp "README.md" "$STAGING_DIR/usr/share/doc/jlauncher/README.md"
fi

# Control file
cat <<EOF > "$STAGING_DIR/DEBIAN/control"
Package: jlauncher
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: JLauncher Contributors <https://github.com/jools333/JLauncher>
Depends: libc6, libx11-6, libxext6, libxft2, libxinerama1, libxcursor1, libxrender1, libxfixes3, libgtk-3-0, libayatana-appindicator3-1
Homepage: https://github.com/jools333/JLauncher
Description: Fast, lightweight keyboard launcher for Linux and Windows
 JLauncher is an instant keyboard-driven launcher and calculator for Linux
 (X11 & Wayland) and Windows. It features fuzzy app search, calculator,
 system power controls, system tray integration, and layout switching.
EOF

# Post-install script
cat <<'EOF' > "$STAGING_DIR/DEBIAN/postinst"
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
    fi
fi
exit 0
EOF
chmod 755 "$STAGING_DIR/DEBIAN/postinst"

# Post-remove script
cat <<'EOF' > "$STAGING_DIR/DEBIAN/postrm"
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
    fi
fi
exit 0
EOF
chmod 755 "$STAGING_DIR/DEBIAN/postrm"

DEB_NAME="jlauncher_${VERSION}_amd64.deb"
GENERIC_DEB="jlauncher_amd64.deb"

dpkg-deb --build --root-owner-group "$STAGING_DIR" "target/installer/$DEB_NAME"
cp "target/installer/$DEB_NAME" "target/installer/$GENERIC_DEB"

echo "Successfully built .deb package:"
echo "  - target/installer/$DEB_NAME"
echo "  - target/installer/$GENERIC_DEB"

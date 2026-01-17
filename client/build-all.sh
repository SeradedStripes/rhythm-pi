#!/bin/bash
# Build script for all platforms

set -e

echo "=== Rhythm PI Client Build Script ==="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

BUILD_DIR="target"
RELEASE_DIR="releases"

# Create releases directory
mkdir -p "$RELEASE_DIR"

echo -e "${BLUE}Building for all targets...${NC}\n"

# 1. Native Linux build
echo -e "${BLUE}[1/3] Building native Linux (x86_64)...${NC}"
cargo build --release
NATIVE_BIN="$BUILD_DIR/release/rhythm-pi-client"
if [ -f "$NATIVE_BIN" ]; then
    cp "$NATIVE_BIN" "$RELEASE_DIR/rhythm-pi-client-linux-x86_64"
    echo -e "${GREEN}✓ Native Linux build successful${NC}"
else
    echo -e "${RED}✗ Native Linux build failed${NC}"
fi

# 2. Linux ARM build
echo -e "\n${BLUE}[2/3] Building Linux ARM (armv7)...${NC}"
if rustup target list | grep -q "armv7-unknown-linux-gnueabihf (installed)"; then
    cargo build --release --target armv7-unknown-linux-gnueabihf
    ARM_BIN="$BUILD_DIR/armv7-unknown-linux-gnueabihf/release/rhythm-pi-client"
    if [ -f "$ARM_BIN" ]; then
        cp "$ARM_BIN" "$RELEASE_DIR/rhythm-pi-client-linux-armv7"
        echo -e "${GREEN}✓ ARM build successful${NC}"
    else
        echo -e "${RED}✗ ARM build failed${NC}"
    fi
else
    echo -e "${RED}✗ armv7-unknown-linux-gnueabihf target not installed${NC}"
    echo "  Install with: rustup target add armv7-unknown-linux-gnueabihf"
fi

# 3. Windows build
echo -e "\n${BLUE}[3/3] Building Windows (x86_64)...${NC}"
if rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
    cargo build --release --target x86_64-pc-windows-gnu
    WIN_BIN="$BUILD_DIR/x86_64-pc-windows-gnu/release/rhythm-pi-client.exe"
    if [ -f "$WIN_BIN" ]; then
        cp "$WIN_BIN" "$RELEASE_DIR/rhythm-pi-client-windows-x86_64.exe"
        echo -e "${GREEN}✓ Windows build successful${NC}"
    else
        echo -e "${RED}✗ Windows build failed${NC}"
    fi
else
    echo -e "${RED}✗ x86_64-pc-windows-gnu target not installed${NC}"
    echo "  Install with: rustup target add x86_64-pc-windows-gnu"
fi

echo ""
echo -e "${GREEN}=== Build Summary ===${NC}"
echo "Release binaries available in: $RELEASE_DIR/"
echo ""
ls -lh "$RELEASE_DIR/" 2>/dev/null || echo "No binaries found"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# --- 설정 ---
APP_NAME="rplayer"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
OS="$(uname -s)"
ARCH="$(uname -m)"

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'
info()  { echo -e "${B}[INFO]${NC} $*"; }
ok()    { echo -e "${G}[OK]${NC} $*"; }
err()   { echo -e "${R}[ERROR]${NC} $*"; exit 1; }

case "$OS" in
    Darwin)  PLATFORM="macos" ;;
    Linux)   PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *)       err "지원하지 않는 OS: $OS" ;;
esac

DIST_NAME="${APP_NAME}-${VERSION}-${PLATFORM}-${ARCH}"
DIST_DIR="dist/$DIST_NAME"
BINARY="target/release/$APP_NAME"
if [ "$PLATFORM" = "windows" ]; then
    BINARY="${BINARY}.exe"
fi

# --- 릴리스 빌드 ---
if [ ! -f "$BINARY" ]; then
    info "릴리스 바이너리가 없습니다. 빌드를 먼저 실행합니다."
    bash scripts/build.sh release
fi

# --- 패키징 ---
info "패키징: $DIST_NAME"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# 바이너리
cp "$BINARY" "$DIST_DIR/"

# 문서
cp MANUAL.md "$DIST_DIR/"
[ -f LICENSE ] && cp LICENSE "$DIST_DIR/" || true

# --- macOS .app 번들 ---
if [ "$PLATFORM" = "macos" ]; then
    info "macOS .app 번들 생성 중..."
    APP_BUNDLE="dist/${APP_NAME}-${VERSION}.app"
    rm -rf "$APP_BUNDLE"
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"

    cp "$BINARY" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"

    cat > "$APP_BUNDLE/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.rplayer.app</string>
    <key>CFBundleName</key>
    <string>rPlayer</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Video File</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.movie</string>
                <string>public.video</string>
                <string>public.mpeg-4</string>
                <string>public.avi</string>
                <string>org.matroska.mkv</string>
                <string>com.microsoft.windows-media-wmv</string>
            </array>
        </dict>
    </array>
    <key>NSCameraUsageDescription</key>
    <string>rPlayer에서 카메라를 사용합니다.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>rPlayer에서 마이크를 사용합니다.</string>
</dict>
</plist>
PLIST

    ok ".app 번들: $APP_BUNDLE"
fi

# --- 아카이브 ---
info "아카이브 생성 중..."
cd dist
if [ "$PLATFORM" = "windows" ]; then
    if command -v zip &>/dev/null; then
        zip -r "${DIST_NAME}.zip" "$DIST_NAME"
        ok "dist/${DIST_NAME}.zip"
    else
        info "zip이 없어 아카이브를 건너뜁니다."
    fi
else
    tar czf "${DIST_NAME}.tar.gz" "$DIST_NAME"
    ok "dist/${DIST_NAME}.tar.gz"

    if [ "$PLATFORM" = "macos" ]; then
        tar czf "${APP_NAME}-${VERSION}.app.tar.gz" "${APP_NAME}-${VERSION}.app"
        ok "dist/${APP_NAME}-${VERSION}.app.tar.gz"
    fi
fi
cd "$ROOT"

# --- 결과 ---
echo ""
info "배포 파일:"
ls -lh dist/*.tar.gz dist/*.zip 2>/dev/null || true
echo ""
ok "패키징 완료"

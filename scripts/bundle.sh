#!/usr/bin/env bash
#
# macOS 전용: 바이너리 + 모든 비시스템 dylib를 .app 번들로 패키징
# 결과물은 시스템에 FFmpeg/Homebrew 설치 없이 실행 가능
#
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_NAME="rplayer"
VERSION=$(grep '^version = "' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
BINARY="target/release/$APP_NAME"

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'
info()  { echo -e "${B}[INFO]${NC} $*"; }
ok()    { echo -e "${G}[OK]${NC} $*"; }
err()   { echo -e "${R}[ERROR]${NC} $*"; exit 1; }

[ -f "$BINARY" ] || err "바이너리 없음. 먼저 bash scripts/build.sh release"

info "rPlayer v$VERSION 샌드박스 번들 생성"

# === .app 구조 생성 ===
APP="dist/rPlayer.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Frameworks"
mkdir -p "$APP/Contents/Resources"

cp "$BINARY" "$APP/Contents/MacOS/$APP_NAME"
cp MANUAL.md "$APP/Contents/Resources/" 2>/dev/null || true

# === Info.plist ===
cat > "$APP/Contents/Info.plist" << PLIST
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
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Video</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.movie</string>
                <string>public.video</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# === dylib 재귀 수집 ===
info "비시스템 dylib 수집 중..."

COLLECTED=()
FW_DIR="$APP/Contents/Frameworks"

collect_dylibs() {
    local target="$1"
    # 비시스템 dylib 추출
    otool -L "$target" | awk '{print $1}' | grep -v "^$target\|/usr/lib\|/System\|@rpath\|@executable_path\|@loader_path" | while read -r lib; do
        local base
        base=$(basename "$lib")
        # 이미 수집했으면 스킵
        [ -f "$FW_DIR/$base" ] && continue
        # 실제 파일 찾기 (symlink 해소)
        local real
        real=$(realpath "$lib" 2>/dev/null || echo "$lib")
        if [ -f "$real" ]; then
            cp "$real" "$FW_DIR/$base"
            chmod 644 "$FW_DIR/$base"
            echo "  $base"
            # 재귀
            collect_dylibs "$FW_DIR/$base"
        fi
    done
}

collect_dylibs "$APP/Contents/MacOS/$APP_NAME"

LIBCOUNT=$(find "$FW_DIR" -name "*.dylib" | wc -l | tr -d ' ')
ok "${LIBCOUNT} dylib collected"

# === rpath 재작성 ===
info "install_name_tool로 rpath 재작성..."

EXEC="$APP/Contents/MacOS/$APP_NAME"

# 바이너리의 외부 참조를 @executable_path/../Frameworks/ 로 변경
otool -L "$EXEC" | awk '{print $1}' | grep -v "/usr/lib\|/System\|$APP_NAME:" | while read -r lib; do
    base=$(basename "$lib")
    if [ -f "$FW_DIR/$base" ]; then
        install_name_tool -change "$lib" "@executable_path/../Frameworks/$base" "$EXEC" 2>/dev/null
    fi
done

# 각 dylib 내부의 상호 참조도 변경
for fw in "$FW_DIR"/*.dylib; do
    base=$(basename "$fw")
    # id 변경
    install_name_tool -id "@executable_path/../Frameworks/$base" "$fw" 2>/dev/null

    # 의존성 변경
    otool -L "$fw" | awk '{print $1}' | grep -v "/usr/lib\|/System\|$base" | while read -r dep; do
        dep_base=$(basename "$dep")
        if [ -f "$FW_DIR/$dep_base" ]; then
            install_name_tool -change "$dep" "@executable_path/../Frameworks/$dep_base" "$fw" 2>/dev/null
        fi
    done
done

ok "rpath 재작성 완료"

# === 검증 ===
info "번들 검증..."
REMAINING=$(otool -L "$EXEC" | grep "/opt/homebrew\|/usr/local" | wc -l | tr -d ' ')
if [ "$REMAINING" -gt 0 ]; then
    echo -e "${Y}경고: 아직 외부 참조가 남아있음:${NC}"
    otool -L "$EXEC" | grep "/opt/homebrew\|/usr/local"
else
    ok "외부 의존성 없음 — 완전한 샌드박스"
fi

# === 아카이브 ===
info "아카이브 생성..."
cd dist
tar czf "rPlayer-${VERSION}-macos-standalone.tar.gz" "rPlayer.app"
cd "$ROOT"

SIZE=$(du -sh "$APP" | cut -f1)
ARCHIVE_SIZE=$(du -h "dist/rPlayer-${VERSION}-macos-standalone.tar.gz" | cut -f1)

echo ""
ok "번들: $APP ($SIZE)"
ok "아카이브: dist/rPlayer-${VERSION}-macos-standalone.tar.gz ($ARCHIVE_SIZE)"
echo ""
echo "설치: tar xzf rPlayer-${VERSION}-macos-standalone.tar.gz"
echo "실행: open rPlayer.app"
echo "또는: rPlayer.app/Contents/MacOS/rplayer video.mp4"

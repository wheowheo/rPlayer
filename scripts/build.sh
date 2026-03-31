#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# --- 설정 ---
APP_NAME="rplayer"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
TARGET="${1:-release}"  # debug 또는 release

# --- 색상 ---
R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'
info()  { echo -e "${B}[INFO]${NC} $*"; }
ok()    { echo -e "${G}[OK]${NC} $*"; }
warn()  { echo -e "${Y}[WARN]${NC} $*"; }
err()   { echo -e "${R}[ERROR]${NC} $*"; exit 1; }

# --- OS 감지 ---
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Darwin)  PLATFORM="macos" ;;
    Linux)   PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *)       err "지원하지 않는 OS: $OS" ;;
esac
info "플랫폼: $PLATFORM ($ARCH)"
info "버전: $VERSION"

# --- 의존성 확인 ---
check_dep() {
    if ! command -v "$1" &>/dev/null; then
        err "'$1' 을(를) 찾을 수 없습니다. $2"
    fi
}

check_dep cargo "https://rustup.rs 에서 Rust를 설치하세요."
check_dep pkg-config "brew install pkgconf (macOS) / apt install pkg-config (Linux)"

info "의존성 확인 중..."
if ! pkg-config --exists libavcodec libavformat libavutil libswscale libswresample 2>/dev/null; then
    case "$PLATFORM" in
        macos)   err "FFmpeg을 찾을 수 없습니다. brew install ffmpeg" ;;
        linux)   err "FFmpeg dev 패키지를 찾을 수 없습니다. apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev" ;;
        windows) warn "pkg-config로 FFmpeg 확인 불가. FFMPEG_DIR 환경변수를 확인하세요." ;;
    esac
fi
ok "의존성 확인 완료"

# --- 빌드 ---
CARGO_ARGS=()
if [ "$TARGET" = "release" ]; then
    CARGO_ARGS+=(--release)
    PROFILE="release"
    info "릴리스 빌드 시작..."
else
    PROFILE="debug"
    info "디버그 빌드 시작..."
fi

cargo build "${CARGO_ARGS[@]}"

BINARY="target/$PROFILE/$APP_NAME"
if [ "$PLATFORM" = "windows" ]; then
    BINARY="${BINARY}.exe"
fi

if [ ! -f "$BINARY" ]; then
    err "빌드 결과물을 찾을 수 없습니다: $BINARY"
fi

SIZE=$(du -h "$BINARY" | cut -f1)
ok "빌드 완료: $BINARY ($SIZE)"

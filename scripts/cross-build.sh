#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_NAME="rplayer"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
OS="$(uname -s)"
ARCH="$(uname -m)"

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'
info()  { echo -e "${B}[INFO]${NC} $*"; }
ok()    { echo -e "${G}[OK]${NC} $*"; }
warn()  { echo -e "${Y}[WARN]${NC} $*"; }
err()   { echo -e "${R}[ERROR]${NC} $*"; exit 1; }

echo "========================================="
echo " rPlayer 빌드 스크립트"
echo " 현재 환경: $OS $ARCH"
echo "========================================="
echo ""

case "$OS" in
    Darwin)
        info "macOS 네이티브 빌드"
        bash scripts/build.sh release
        echo ""

        # Universal Binary는 CI에서만 가능 (두 아키텍처의 FFmpeg 필요)
        warn "Universal Binary (ARM64+x64)는 GitHub Actions CI에서 생성됩니다."
        warn "  git tag v${VERSION} && git push origin v${VERSION}"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        info "Windows 네이티브 빌드"
        if [ -z "${FFMPEG_DIR:-}" ]; then
            warn "FFMPEG_DIR 환경변수를 설정하세요."
            warn "  예: set FFMPEG_DIR=C:\\ffmpeg"
            err "FFmpeg dev 패키지가 필요합니다."
        fi
        bash scripts/build.sh release
        ;;
    Linux)
        info "Linux 네이티브 빌드"
        if ! pkg-config --exists libavcodec 2>/dev/null; then
            err "FFmpeg dev 패키지를 설치하세요: sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev"
        fi
        bash scripts/build.sh release
        ;;
    *)
        err "지원하지 않는 OS: $OS"
        ;;
esac

echo ""
echo "========================================="
echo " 크로스플랫폼 배포는 GitHub Actions 사용"
echo ""
echo " 릴리스 방법:"
echo "   git tag v${VERSION}"
echo "   git push origin v${VERSION}"
echo ""
echo " CI가 자동으로 macOS/Windows/Linux 빌드 후"
echo " GitHub Releases에 업로드합니다."
echo "========================================="

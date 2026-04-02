#!/usr/bin/env bash
#
# rPlayer 원라인 설치 스크립트
# curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
#
set -uo pipefail

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'

REPO="wheowheo/rPlayer"
INSTALL_DIR="$HOME/Applications"

echo ""
echo -e "${B}rPlayer 설치${NC}"
echo "==============================="

# OS 감지
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *) echo -e "${R}지원하지 않는 OS: $OS${NC}"; exit 1 ;;
esac

echo -e "플랫폼: ${G}$PLATFORM $ARCH${NC}"

# GitHub 최신 릴리스에서 다운로드 URL 획득
echo -e "${B}최신 버전 확인 중...${NC}"

if command -v gh &>/dev/null; then
    # gh CLI 사용
    RELEASE_TAG=$(gh release view --repo "$REPO" --json tagName -q '.tagName' 2>/dev/null)
else
    # curl + GitHub API
    RELEASE_TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*: "\(.*\)".*/\1/')
fi

if [ -z "$RELEASE_TAG" ]; then
    echo -e "${R}릴리스를 찾을 수 없습니다${NC}"
    exit 1
fi

echo -e "버전: ${G}$RELEASE_TAG${NC}"

# 플랫폼별 에셋 이름
case "$PLATFORM" in
    macos)
        ASSET="rPlayer-${RELEASE_TAG#v}-macos-standalone.tar.gz"
        ;;
    windows)
        ASSET="rplayer-windows-x64.zip"
        ;;
    linux)
        ASSET="rplayer-linux-x64.tar.gz"
        ;;
esac

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$RELEASE_TAG/$ASSET"

# 다운로드
TMPDIR=$(mktemp -d)
echo -e "${B}다운로드: $ASSET${NC}"

if ! curl -fSL "$DOWNLOAD_URL" -o "$TMPDIR/$ASSET" 2>/dev/null; then
    echo -e "${R}다운로드 실패: $DOWNLOAD_URL${NC}"
    echo -e "${Y}해당 플랫폼의 릴리스가 아직 없을 수 있습니다.${NC}"
    rm -rf "$TMPDIR"
    exit 1
fi

echo -e "${G}다운로드 완료${NC}"

# 설치
case "$PLATFORM" in
    macos)
        mkdir -p "$INSTALL_DIR"
        echo -e "${B}설치: $INSTALL_DIR/rPlayer.app${NC}"
        # 기존 버전 제거
        rm -rf "$INSTALL_DIR/rPlayer.app"
        cd "$TMPDIR"
        tar xzf "$ASSET"
        mv rPlayer.app "$INSTALL_DIR/"
        echo ""
        echo -e "${G}설치 완료!${NC}"
        echo ""
        echo "실행 방법:"
        echo "  open $INSTALL_DIR/rPlayer.app"
        echo "  또는: $INSTALL_DIR/rPlayer.app/Contents/MacOS/rplayer video.mp4"
        echo ""
        echo "CLI에서 사용하려면:"
        echo "  sudo ln -sf $INSTALL_DIR/rPlayer.app/Contents/MacOS/rplayer /usr/local/bin/rplayer"
        ;;

    linux)
        LINUX_DIR="$HOME/.local/share/rplayer"
        mkdir -p "$LINUX_DIR"
        echo -e "${B}설치: $LINUX_DIR${NC}"
        cd "$TMPDIR"
        tar xzf "$ASSET" -C "$LINUX_DIR"
        chmod +x "$LINUX_DIR/rplayer"
        mkdir -p "$HOME/.local/bin"
        ln -sf "$LINUX_DIR/rplayer" "$HOME/.local/bin/rplayer"
        echo ""
        echo -e "${G}설치 완료!${NC}"
        echo ""
        echo "실행: rplayer video.mp4"
        echo "(~/.local/bin이 PATH에 없으면: export PATH=\$HOME/.local/bin:\$PATH)"
        ;;

    windows)
        WIN_DIR="$HOME/rPlayer"
        mkdir -p "$WIN_DIR"
        echo -e "${B}설치: $WIN_DIR${NC}"
        cd "$TMPDIR"
        unzip -o "$ASSET" -d "$WIN_DIR"
        echo ""
        echo -e "${G}설치 완료!${NC}"
        echo ""
        echo "실행: $WIN_DIR\\rplayer.exe video.mp4"
        ;;
esac

# 정리
rm -rf "$TMPDIR"
echo ""

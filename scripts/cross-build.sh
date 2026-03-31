#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# --- 설정 ---
APP_NAME="rplayer"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'
info()  { echo -e "${B}[INFO]${NC} $*"; }
ok()    { echo -e "${G}[OK]${NC} $*"; }
warn()  { echo -e "${Y}[WARN]${NC} $*"; }
err()   { echo -e "${R}[ERROR]${NC} $*"; exit 1; }

TARGETS=("${@}")
if [ ${#TARGETS[@]} -eq 0 ]; then
    echo "사용법: $0 <target...>"
    echo ""
    echo "지원 타겟:"
    echo "  macos-arm64      aarch64-apple-darwin"
    echo "  macos-x64        x86_64-apple-darwin"
    echo "  macos-universal   위 두 개 합쳐서 Universal Binary"
    echo "  windows-x64      x86_64-pc-windows-msvc (네이티브 전용)"
    echo "  linux-x64        x86_64-unknown-linux-gnu (cross 필요)"
    echo ""
    echo "예: $0 macos-arm64 macos-x64 macos-universal"
    exit 0
fi

mkdir -p dist

build_target() {
    local TRIPLE="$1"
    local LABEL="$2"

    info "빌드: $LABEL ($TRIPLE)"

    # 타겟 설치 확인
    if ! rustup target list --installed | grep -q "$TRIPLE"; then
        info "타겟 추가: $TRIPLE"
        rustup target add "$TRIPLE"
    fi

    cargo build --release --target "$TRIPLE"

    local BIN="target/$TRIPLE/release/$APP_NAME"
    if [ ! -f "$BIN" ]; then
        warn "$LABEL 빌드 실패"
        return 1
    fi

    local SIZE=$(du -h "$BIN" | cut -f1)
    ok "$LABEL 완료 ($SIZE)"
}

for TARGET in "${TARGETS[@]}"; do
    case "$TARGET" in
        macos-arm64)
            build_target "aarch64-apple-darwin" "macOS ARM64"
            ;;
        macos-x64)
            build_target "x86_64-apple-darwin" "macOS x86_64"
            ;;
        macos-universal)
            # 두 아키텍처 빌드 후 lipo로 합침
            build_target "aarch64-apple-darwin" "macOS ARM64"
            build_target "x86_64-apple-darwin" "macOS x86_64"

            info "Universal Binary 생성 중..."
            UNIVERSAL_DIR="dist/${APP_NAME}-${VERSION}-macos-universal"
            mkdir -p "$UNIVERSAL_DIR"
            lipo -create \
                "target/aarch64-apple-darwin/release/$APP_NAME" \
                "target/x86_64-apple-darwin/release/$APP_NAME" \
                -output "$UNIVERSAL_DIR/$APP_NAME"

            local SIZE=$(du -h "$UNIVERSAL_DIR/$APP_NAME" | cut -f1)
            ok "Universal Binary: $UNIVERSAL_DIR/$APP_NAME ($SIZE)"

            file "$UNIVERSAL_DIR/$APP_NAME"
            ;;
        windows-x64)
            if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
                build_target "x86_64-pc-windows-msvc" "Windows x64"
            else
                warn "Windows 타겟은 Windows에서만 빌드할 수 있습니다 (MSVC 링커 필요)."
                warn "대안: GitHub Actions CI를 사용하세요."
            fi
            ;;
        linux-x64)
            if [[ "$(uname -s)" == "Linux" ]]; then
                build_target "x86_64-unknown-linux-gnu" "Linux x64"
            else
                warn "Linux 타겟은 Linux에서 빌드하거나 cross를 사용하세요."
                if command -v cross &>/dev/null; then
                    info "cross로 빌드 시도..."
                    cross build --release --target x86_64-unknown-linux-gnu
                else
                    warn "cross가 설치되어 있지 않습니다. cargo install cross"
                fi
            fi
            ;;
        *)
            warn "알 수 없는 타겟: $TARGET"
            ;;
    esac
done

echo ""
ok "크로스 빌드 완료"

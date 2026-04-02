#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' NC='\033[0m'

CURRENT=$(grep '^version = "' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

usage() {
    echo "사용법: $0 <patch|minor|major|set X.Y.Z> [--tag] [--release]"
    echo ""
    echo "  patch       $CURRENT → $MAJOR.$MINOR.$((PATCH+1))"
    echo "  minor       $CURRENT → $MAJOR.$((MINOR+1)).0"
    echo "  major       $CURRENT → $((MAJOR+1)).0.0"
    echo "  set X.Y.Z   직접 지정"
    echo ""
    echo "옵션:"
    echo "  --tag       git tag 생성 + push"
    echo "  --release   tag + GitHub Release 생성 (dist/ 파일 첨부)"
    echo ""
    echo "현재 버전: $CURRENT"
    exit 0
}

[ $# -eq 0 ] && usage

DO_TAG=false
DO_RELEASE=false
NEW_VERSION=""

while [ $# -gt 0 ]; do
    case "$1" in
        patch)   NEW_VERSION="$MAJOR.$MINOR.$((PATCH+1))" ;;
        minor)   NEW_VERSION="$MAJOR.$((MINOR+1)).0" ;;
        major)   NEW_VERSION="$((MAJOR+1)).0.0" ;;
        set)     shift; NEW_VERSION="$1" ;;
        --tag)   DO_TAG=true ;;
        --release) DO_TAG=true; DO_RELEASE=true ;;
        -h|--help) usage ;;
        *) echo -e "${R}알 수 없는 옵션: $1${NC}"; exit 1 ;;
    esac
    shift
done

if [ -z "$NEW_VERSION" ]; then
    echo -e "${R}버전을 지정하세요${NC}"
    exit 1
fi

echo -e "${B}버전 변경: $CURRENT → $NEW_VERSION${NC}"

# Cargo.toml 업데이트
sed -i '' "0,/^version = \"$CURRENT\"/s//version = \"$NEW_VERSION\"/" Cargo.toml
echo -e "${G}Cargo.toml 업데이트${NC}"

# Cargo.lock 갱신
cargo check --quiet 2>/dev/null
echo -e "${G}Cargo.lock 갱신${NC}"

# 커밋
git add Cargo.toml Cargo.lock 2>/dev/null
git commit -m "v$NEW_VERSION 버전 업데이트

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>" --quiet
echo -e "${G}커밋 완료${NC}"

# 태그
if [ "$DO_TAG" = true ]; then
    git tag "v$NEW_VERSION"
    git push origin main --quiet 2>/dev/null
    git push origin "v$NEW_VERSION" --quiet 2>/dev/null
    echo -e "${G}태그 v$NEW_VERSION push 완료${NC}"
fi

# 릴리스
if [ "$DO_RELEASE" = true ]; then
    echo -e "${B}빌드 + 패키징...${NC}"
    bash scripts/build.sh release 2>&1 | tail -1
    bash scripts/package.sh 2>&1 | tail -1

    echo -e "${B}GitHub Release 생성...${NC}"
    ASSETS=$(find dist -name "*.tar.gz" -o -name "*.zip" 2>/dev/null | tr '\n' ' ')
    if [ -n "$ASSETS" ]; then
        gh release create "v$NEW_VERSION" $ASSETS \
            --title "v$NEW_VERSION" \
            --generate-notes
        echo -e "${G}릴리스 완료: v$NEW_VERSION${NC}"
    else
        echo -e "${Y}배포 파일 없음. gh release 건너뜀${NC}"
    fi
fi

echo ""
echo -e "${G}완료: v$NEW_VERSION${NC}"

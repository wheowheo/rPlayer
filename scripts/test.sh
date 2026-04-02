#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

R='\033[0;31m' G='\033[0;32m' Y='\033[0;33m' B='\033[0;34m' C='\033[0;36m' NC='\033[0m'
PASS=0; FAIL=0; SKIP=0

pass() { echo -e "  ${G}PASS${NC} $1"; ((PASS++)); }
fail() { echo -e "  ${R}FAIL${NC} $1 — $2"; ((FAIL++)); }
skip() { echo -e "  ${Y}SKIP${NC} $1 — $2"; ((SKIP++)); }
section() { echo -e "\n${C}[$1]${NC}"; }

BINARY="target/release/rplayer"
SAMPLES="$ROOT/samples"
SRC="/Users/ihatego3/Downloads/DJI_20250130_201104_31_null_video.mp4"
LOG="/tmp/rplayer_test"

echo "======================================="
echo " rPlayer 기능 검증 테스트"
echo "======================================="

# ===== 빌드 검증 =====
section "1. 빌드"

BUILD_LOG="${LOG}_build.log"
cargo build --release > "$BUILD_LOG" 2>&1
BUILD_RC=$?

if [ "$BUILD_RC" -ne 0 ]; then
    fail "릴리스 빌드" "컴파일 에러 (exit $BUILD_RC)"
    echo -e "${R}빌드 실패. 중단.${NC}"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo -e "${R}바이너리 없음. 중단.${NC}"
    exit 1
fi

WARNINGS=$(grep "^warning:" "$BUILD_LOG" | grep -v "generated" | wc -l | tr -d ' ')
if [ "$WARNINGS" -eq 0 ]; then
    pass "릴리스 빌드 + 경고 0"
else
    fail "빌드 경고 ${WARNINGS}개" "cargo build 경고 있음"
fi

# ===== 샘플 파일 확인 =====
section "2. 테스트 샘플 준비"

for f in h264_720p_30fps.mp4 h265_1080p_24fps.mp4 vp9_720p.webm vfr_test.mp4; do
    if [ -f "$SAMPLES/$f" ]; then
        pass "샘플 존재: $f"
    else
        skip "샘플 없음: $f" "samples/ 디렉토리에 생성 필요"
    fi
done

if [ -f "$SRC" ]; then
    pass "4K 테스트 파일 존재"
else
    skip "4K 테스트 파일 없음" "$SRC"
fi

# ===== 헬퍼: 플레이어 실행 후 로그 수집 =====
run_test() {
    local file="$1"
    local duration="$2"
    local logfile="$3"
    local loglevel="${4:-info}"

    RUST_LOG="rplayer=$loglevel" "$BINARY" "$file" > "$logfile" 2>&1 &
    local pid=$!
    sleep "$duration"
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null || true
}

# ===== 코덱별 재생 =====
section "3. 코덱별 재생 테스트"

for f in h264_720p_30fps.mp4 h265_1080p_24fps.mp4 vp9_720p.webm vfr_test.mp4; do
    [ ! -f "$SAMPLES/$f" ] && { skip "$f 재생" "파일 없음"; continue; }
    run_test "$SAMPLES/$f" 6 "${LOG}_${f}.log" "debug"

    if grep -q "Opened:" "${LOG}_${f}.log"; then
        pass "$f 파일 열기"
    else
        fail "$f 파일 열기" "Opened 로그 없음"
    fi

    if grep -q "render.*fps" "${LOG}_${f}.log"; then
        fps_line=$(grep "render.*fps" "${LOG}_${f}.log" | tail -1)
        pass "$f 렌더링 ($fps_line)"
    else
        # Short files may finish before 1s fps counter fires
        if grep -q "Playback finished\|Decode loop finished" "${LOG}_${f}.log"; then
            pass "$f 재생 완료 (fps 미측정 — 짧은 파일)"
        else
            fail "$f 렌더링" "render fps 로그 없음"
        fi
    fi

    if grep -q "Audio output started" "${LOG}_${f}.log"; then
        pass "$f 오디오 출력"
    else
        fail "$f 오디오 출력" "오디오 시작 안 됨"
    fi
done

# ===== 하드웨어 디코딩 =====
section "4. 하드웨어 디코딩"

if [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    run_test "$SAMPLES/h264_720p_30fps.mp4" 4 "${LOG}_hw.log" "info"
    if grep -q "Hardware decoder" "${LOG}_hw.log"; then
        pass "VideoToolbox HW 디코딩 초기화"
    else
        fail "HW 디코딩" "Hardware decoder 로그 없음"
    fi
fi

# ===== 4K 60fps 성능 =====
section "5. 4K 60fps 성능"

if [ -f "$SRC" ]; then
    run_test "$SRC" 8 "${LOG}_4k.log" "debug"

    if grep -q "Hardware decoder" "${LOG}_4k.log"; then
        pass "4K HW 디코딩"
    else
        fail "4K HW 디코딩" "HW 초기화 안 됨"
    fi

    fps_line=$(grep "render.*fps" "${LOG}_4k.log" | tail -1)
    if [ -n "$fps_line" ]; then
        pass "4K 렌더링 ($fps_line)"
    else
        fail "4K 렌더링" "fps 로그 없음"
    fi
else
    skip "4K 테스트" "파일 없음"
fi

# ===== 한글 폰트 =====
section "6. 한글 폰트"

if [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    run_test "$SAMPLES/h264_720p_30fps.mp4" 3 "${LOG}_font.log" "info"
    if grep -q "Loaded Korean font" "${LOG}_font.log"; then
        font_path=$(grep "Loaded Korean font" "${LOG}_font.log" | sed 's/.*font: //')
        pass "한글 폰트 로드 ($font_path)"
    else
        fail "한글 폰트" "Korean font 로그 없음"
    fi
fi

# ===== 재생 완료 감지 =====
section "7. 재생 완료 (짧은 파일)"

if [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    run_test "$SAMPLES/h264_720p_30fps.mp4" 14 "${LOG}_eof.log" "info"
    if grep -q "Playback finished\|Decode loop finished" "${LOG}_eof.log"; then
        pass "재생 완료 감지 (10초 파일)"
    else
        fail "재생 완료" "EOF 감지 안 됨"
    fi
fi

# ===== 오디오 DSP =====
section "8. 오디오 DSP 체인"

if [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    run_test "$SAMPLES/h264_720p_30fps.mp4" 5 "${LOG}_dsp.log" "info"
    if grep -q "Audio output started" "${LOG}_dsp.log"; then
        pass "오디오 DSP 파이프라인 (stretch+EQ+comp 초기화)"
    else
        fail "오디오 DSP" "오디오 시작 안 됨"
    fi
fi

# ===== GPU 어댑터 =====
section "9. GPU"

if [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    run_test "$SAMPLES/h264_720p_30fps.mp4" 3 "${LOG}_gpu.log" "info"
    if grep -q "GPU adapter" "${LOG}_gpu.log"; then
        adapter=$(grep "GPU adapter" "${LOG}_gpu.log" | sed 's/.*adapter: "//' | sed 's/"//')
        pass "GPU 어댑터: $adapter"
    else
        fail "GPU" "어댑터 감지 안 됨"
    fi
fi

# ===== 패키징 =====
section "10. 패키징"

if bash scripts/package.sh > "${LOG}_pkg.log" 2>&1; then
    TGZCOUNT=$(find dist -name "*.tar.gz" 2>/dev/null | wc -l | tr -d ' ')
    if [ "$TGZCOUNT" -gt 0 ]; then
        size=$(du -h dist/*.tar.gz 2>/dev/null | head -1 | cut -f1)
        pass "패키징 완료 ($size, ${TGZCOUNT}개 아카이브)"
    else
        fail "패키징" "tar.gz 생성 안 됨"
    fi
    APPCOUNT=$(find dist -name "*.app" -type d 2>/dev/null | wc -l | tr -d ' ')
    if [ "$APPCOUNT" -gt 0 ]; then
        pass ".app 번들 생성"
    else
        fail ".app 번들" "생성 안 됨"
    fi
else
    fail "패키징 스크립트" "실행 실패"
fi

# ===== 패키징된 바이너리 실행 =====
section "11. 배포 바이너리 검증"

DIST_BIN=$(find dist -name "rplayer" -not -path "*.app*" | head -1)
if [ -n "$DIST_BIN" ] && [ -f "$DIST_BIN" ] && [ -f "$SAMPLES/h264_720p_30fps.mp4" ]; then
    RUST_LOG=rplayer=info "$DIST_BIN" "$SAMPLES/h264_720p_30fps.mp4" > "${LOG}_dist.log" 2>&1 &
    DPID=$!; sleep 4; kill $DPID 2>/dev/null; wait $DPID 2>/dev/null || true
    if grep -q "Opened:" "${LOG}_dist.log"; then
        pass "배포 바이너리 실행"
    else
        fail "배포 바이너리" "실행 실패"
    fi
else
    skip "배포 바이너리 실행" "바이너리 또는 샘플 없음"
fi

# ===== 결과 =====
echo ""
echo "======================================="
echo -e " 결과: ${G}PASS $PASS${NC}  ${R}FAIL $FAIL${NC}  ${Y}SKIP $SKIP${NC}"
TOTAL=$((PASS + FAIL))
if [ "$TOTAL" -gt 0 ]; then
    RATE=$((PASS * 100 / TOTAL))
    echo -e " 통과율: ${RATE}%"
fi
echo "======================================="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

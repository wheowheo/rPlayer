# rPlayer 개발 삽질 기록

Rust 크로스플랫폼 비디오 플레이어를 처음부터 만들면서 겪은 문제들과 해결 과정.

---

## 1. FFmpeg 8.1 바인딩 호환성

**문제**: `ffmpeg-next` 7.x가 FFmpeg 8.1에서 제거된 `avfft.h`를 참조하여 빌드 실패.

**시도**: `BINDGEN_EXTRA_CLANG_ARGS`, `PKG_CONFIG_PATH` 등 환경변수 조작 → 실패.

**해결**: `ffmpeg-next`를 8.1.0으로 업그레이드. crates.io에 FFmpeg 8.1 대응 버전이 이미 있었음.

**교훈**: FFmpeg 메이저 버전 올릴 때 바인딩 크레이트도 반드시 맞춰야 한다.

---

## 2. 비디오 큐 backpressure가 오디오를 죽임

**문제**: 비디오와 오디오를 같은 스레드에서 디코딩하는데, 비디오 프레임 큐(`bounded(8)`)가 차면 `frame_tx.send()`가 블록 → 같은 스레드의 오디오 패킷도 처리 못함 → 오디오 끊김.

**증상**: 재생 시작 4초 후 "Audio stall detected" 로그. 오디오가 멈추고 벽시계로 전환.

**해결**: 비디오 전송을 `try_send`로 바꿔서 큐 가득 차면 프레임을 드롭. 오디오 연속성이 비디오보다 중요.

**나중에 다시 문제됨**: `try_send`로 바꾸니 큐가 작을 때 프레임의 50%가 드롭됨 → `send_timeout(50ms)`으로 최종 해결. 짧게 기다리되 무한 블록은 안 함.

---

## 3. Seek 후 소리만 나고 화면이 안 나옴

**문제**: seek 실행 → 오디오가 즉시 재생 시작 → 클럭이 비디오보다 앞서감 → 비디오 프레임이 도착했을 때 이미 "과거" → 전부 드롭.

**근본 원인**:
- `clock.reset()` 후 `samples_played` atomic이 리셋 안 됨
- 오디오 버퍼에 seek 이전 데이터가 남아있음
- 클럭이 wall clock으로 폴백하면서 시간이 즉시 흐름

**해결**:
1. seek 시 오디오를 즉시 pause + 버퍼 flush
2. 클럭을 "freeze" — `frozen_time`에 고정, 시간 진행 차단
3. 첫 비디오 프레임이 화면에 표시되면 `unfreeze()` + 오디오 resume
4. 비디오와 오디오가 동시에 시작

---

## 4. 4K 60fps에서 극심한 버벅임 (첫 번째)

**문제**: SW 디코딩 + `swscale`(YUV420P→RGBA) CPU 변환. 4K = 3840×2160 × 4바이트 = 33MB/frame. 60fps면 초당 2GB CPU 복사.

**해결**: 
- HW 디코딩 (VideoToolbox) 추가 — `ffmpeg::ffi::av_hwdevice_ctx_create` unsafe FFI
- swscale 완전 제거 — YUV 평면을 GPU 텍스처(R8)로 직접 업로드
- WGSL 프래그먼트 셰이더에서 BT.601 YUV→RGB 변환

**결과**: SW+swscale 559 드롭 → HW+YUV GPU 11 드롭 (초반만).

---

## 5. 4K 60fps에서 극심한 버벅임 (두 번째 — 진짜 원인)

**문제**: HW+GPU 최적화 후에도 "버벅임". 로그를 보면 render는 60fps인데 **프레임이 화면에 안 올라감**.

**프로파일링**: `update_frame`에 `show=false` 로그 → **매 렌더마다 비디오 프레임 표시 실패**. 큐에서 프레임을 가져오지만 sync 로직이 "미래 프레임"으로 판정하여 `pending_frame`에 넣음 → 다음 렌더에서도 같은 판정 → **영원히 표시 안 됨**.

**근본 원인**: 
- `SYNC_THRESHOLD_SECS = 40ms`인데 60fps 프레임 간격은 16.7ms
- 클럭이 약간만 뒤처져도 프레임이 항상 "미래"로 판정
- `pending_frame`에 갇힌 프레임이 큐를 블로킹 → 디코더의 새 프레임도 `try_send` Full → 전부 드롭

**해결**: sync 로직을 완전히 재작성.
- "미래 프레임 대기" 로직 완전 제거
- 큐에서 1개 가져와서 무조건 표시 (1:1 render-frame 매칭)
- 디코더의 backpressure는 `send_timeout`으로 자연스럽게 조절

---

## 6. drain-all 최적화가 역효과

**문제**: "큐를 전부 비우고 최신 프레임만 보여주자" → 60fps에서 매 render마다 큐에 2개 → 1개 드롭 + 1개 표시 = **50% 프레임 드롭**.

**교훈**: 비디오 플레이어에서 "최신 프레임만" 전략은 게임에는 맞지만, 영상 재생에는 맞지 않음. 영상은 **모든 프레임을 순서대로 1:1로 표시**해야 함.

**최종 해결**: `try_recv` 1개만 → 표시. 간단하지만 정답.

---

## 7. 한글 깨짐

**문제**: egui 기본 폰트에 한글 글리프 없음 → 메뉴/오버레이 텍스트가 □□□로 표시.

**해결**: macOS `AppleSDGothicNeo.ttc` / Windows `malgun.ttf` / Linux `NotoSansCJK` 시스템 폰트를 자동 탐색하여 egui에 등록.

**추가 문제**: 유니코드 이모지(⏸⏹⏪)도 시스템 폰트에서 렌더링 안 됨 → ASCII 텍스트 기호(`||`, `[]`, `<<`, `>>`)로 교체.

---

## 8. egui 메뉴 반응 느림

**문제**: 메뉴를 클릭해도 1~2초 후에 열림. 마우스 올려도 반응 없음.

**원인**: `ControlFlow::Wait` + 재생 안 할 때 `request_redraw()` 안 함 → egui가 repaint를 요청해도 winit이 무시.

**해결**: 
- `egui_state.on_window_event()`의 `response.repaint`를 체크하여 `request_redraw()` 호출
- 컨텍스트 메뉴 활성 시 `ctx.request_repaint()`
- `requested_repaint_last_pass()`로 egui 내부 repaint 요청도 반영

---

## 9. seekbar 범위 문제

**문제**: egui `Slider`가 전체 너비를 사용하지 않고 일부만 차지 → 긴 영상에서 seekbar 끝까지 드래그해도 영상 후반부에 못 감.

**원인**: egui `Slider`는 내부에 숫자 입력 필드를 포함하고, `ui.horizontal` 안에서 다른 위젯과 공간을 나눔.

**해결**: Slider 제거 → `ui.allocate_exact_size(available_width)` 커스텀 바 직접 그림. 클릭/드래그 비율로 seek 위치 계산.

---

## 10. 크로스 빌드 불가

**문제**: `scripts/cross-build.sh`로 macOS ARM64에서 x86_64 빌드 시도 → `pkg-config has not been configured to support cross-compilation`.

**원인**: FFmpeg은 `pkg-config`로 타겟 OS의 네이티브 `.dylib`를 링크. 다른 아키텍처/OS의 라이브러리가 로컬에 없으면 빌드 불가.

**해결**: 로컬 크로스 빌드 포기. GitHub Actions CI에서 각 OS별 네이티브 빌드 (macos-latest, windows-latest, ubuntu-latest).

---

## 11. 오디오 버퍼 drain 성능

**문제**: `Vec<f32>` + `drain(..n)` = O(n) 이동. 실시간 오디오 콜백에서 힙 재할당 가능.

**해결**: `VecDeque<f32>`로 교체. `drain(..n)`이 O(1) (head pointer만 이동).

---

## 성능 변천사 (4K H.264 60fps, M1 MacBook Air)

| 단계 | render fps | 프레임 드롭 (10초) | 비고 |
|------|-----------|-------------------|------|
| SW decode + swscale RGBA | ~10 | 559 | CPU 풀로드 |
| HW decode + swscale RGBA | 60 | 48 | swscale 병목 |
| HW decode + YUV GPU shader | 60 | 11 | 초반만 |
| + sync 로직 수정 | 60 | 0 (표시 실패 90%) | 화면에 안 올라감 |
| + 1:1 frame 매칭 | 60 | 0 | 최종 |

---

## 핵심 교훈

1. **비디오 플레이어의 sync는 단순할수록 좋다.** "미래 프레임 대기" 같은 정교한 로직보다 "1개 가져와서 바로 표시"가 60fps에서 훨씬 안정적.

2. **같은 스레드에서 비디오+오디오 처리하면 backpressure가 서로를 죽인다.** 비디오 큐가 차면 오디오도 멈춤.

3. **CPU 컬러 스페이스 변환은 4K에서 치명적.** GPU 셰이더로 옮기면 공짜.

4. **egui는 즉시 모드 UI라 매 프레임 repaint가 필요.** winit의 이벤트 드리븐 모델과 잘 맞추려면 repaint 요청을 명시적으로 전달해야 함.

5. **프로파일링 없이 추측으로 최적화하면 역효과.** "drain-all이 빠르겠지" → 50% 드롭. 항상 측정 먼저.

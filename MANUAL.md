# rPlayer 매뉴얼

Rust 크로스플랫폼 비디오 플레이어. FFmpeg 8.1 + wgpu + egui 기반.

## 빌드 요구사항

### 공통
- Rust 1.75 이상 (`rustup update stable`)
- Cargo (Rust 설치 시 포함)

### macOS
```bash
brew install ffmpeg pkgconf
```
- FFmpeg 8.1 (Homebrew)
- pkgconf (FFmpeg 링킹용)
- Xcode Command Line Tools (`xcode-select --install`)

### Windows
```powershell
# vcpkg 사용
vcpkg install ffmpeg:x64-windows
set VCPKG_ROOT=C:\vcpkg
```
또는 사전 빌드된 FFmpeg 바이너리를 사용:
```powershell
# FFmpeg dev 패키지를 다운로드하고 환경변수 설정
set FFMPEG_DIR=C:\ffmpeg
set PATH=%FFMPEG_DIR%\bin;%PATH%
```

### Linux
```bash
# Ubuntu/Debian
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev pkg-config

# Fedora
sudo dnf install ffmpeg-devel pkgconf
```

## 빌드

```bash
cd rPlayer

# 디버그 빌드
cargo build

# 릴리스 빌드 (최적화, 권장)
cargo build --release
```

릴리스 바이너리 위치:
- macOS/Linux: `target/release/rplayer`
- Windows: `target\release\rplayer.exe`

## 실행

```bash
# 바로 실행
cargo run --release

# 파일 지정
cargo run --release -- /path/to/video.mp4

# 로그 활성화
RUST_LOG=rplayer=info cargo run --release -- video.mp4

# 디버그 로그
RUST_LOG=rplayer=debug cargo run --release -- video.mp4
```

## 지원 포맷

| 컨테이너 | 비디오 코덱 | 오디오 코덱 |
|----------|-----------|-----------|
| MP4, MOV, M4V | H.264, H.265 | AAC, MP3 |
| MKV | H.264, H.265, VP9, AV1 | AAC, Opus, FLAC, Vorbis |
| WebM | VP9, AV1 | Opus, Vorbis |
| AVI | H.264, MPEG-4 | MP3, PCM |
| FLV | H.264 | AAC, MP3 |
| TS | H.264, H.265 | AAC, AC3 |
| WMV | WMV3, VC-1 | WMA |

FFmpeg이 지원하는 모든 코덱을 사용할 수 있다.

## 하드웨어 디코딩

기본적으로 하드웨어 디코딩을 시도하며, 실패 시 소프트웨어로 자동 전환된다.

| OS | HW 가속 | 지원 코덱 |
|----|---------|----------|
| macOS | VideoToolbox | H.264, H.265, VP9 |
| Windows | D3D11VA | H.264, H.265 |
| Linux | VAAPI | H.264, H.265, VP9 |

`R` 키로 재생 중 HW/SW 전환이 가능하다. 현재 모드는 하단 상태바에 `HW` 또는 `SW`로 표시된다.

## 단축키

| 키 | 기능 |
|----|------|
| `Space` | 재생 / 일시정지 |
| `Esc` | 정지 (처음으로) |
| `O` | 파일 열기 대화상자 |
| `R` | HW/SW 디코딩 전환 |
| `Tab` | 정보 오버레이 표시/숨김 |
| `M` | 음소거 토글 |
| `←` | 5초 뒤로 |
| `→` | 5초 앞으로 |
| `↑` | 볼륨 5% 증가 (최대 200%) |
| `↓` | 볼륨 5% 감소 |
| `]` | 배속 0.25x 증가 (최대 4.0x) |
| `[` | 배속 0.25x 감소 (최소 0.25x) |
| `+` | 자막 싱크 +0.5초 |
| `-` | 자막 싱크 -0.5초 |

## 자막

SRT와 SMI 형식을 지원한다. 비디오 파일과 같은 디렉토리에 같은 이름의 자막 파일이 있으면 자동으로 로드된다.

```
video.mp4
video.srt    <- 자동 감지
video.smi    <- 자동 감지
```

## 파일 열기

세 가지 방법이 있다:
1. **커맨드라인**: `rplayer video.mp4`
2. **O 키**: 파일 선택 대화상자
3. **드래그 앤 드롭**: 윈도우에 파일을 끌어다 놓기

## 정보 오버레이 (Tab)

`Tab` 키를 누르면 좌상단에 재생 정보가 표시된다:
- 코덱, 해상도, FPS
- 현재 시간 / 전체 길이
- 배속, 볼륨
- 디코더 모드 (HW/SW)

## 프로젝트 구조

```
rPlayer/
├── Cargo.toml              # 의존성 및 빌드 설정
├── CLAUDE.md               # AI 개발 명세
├── MANUAL.md               # 이 문서
└── src/
    ├── main.rs             # 진입점, winit 이벤트 루프
    ├── app.rs              # 앱 상태, wgpu/egui 렌더링
    ├── config.rs           # 상수 설정값
    ├── error.rs            # 에러 타입
    ├── decode/
    │   ├── demuxer.rs      # FFmpeg 컨테이너 파싱, seek
    │   ├── video_decoder.rs # 비디오 디코딩 (HW/SW)
    │   └── audio_decoder.rs # 오디오 디코딩 + 리샘플링
    ├── media/
    │   ├── pipeline.rs     # 디먹스+디코딩 스레드 관리
    │   ├── clock.rs        # A/V 동기화 마스터 클럭
    │   └── sync.rs         # 프레임 드롭/대기 판정
    ├── audio/
    │   └── output.rs       # cpal 오디오 출력
    ├── video/
    │   ├── renderer.rs     # wgpu 비디오 텍스처 렌더링
    │   └── shader.wgsl     # GPU 셰이더
    ├── subtitle/
    │   ├── parser_srt.rs   # SRT 파서
    │   └── parser_smi.rs   # SMI 파서
    ├── camera/
    │   └── mod.rs          # 카메라 캡처 인터페이스
    ├── db/
    │   └── clothing.rs     # SQLite 의류 DB
    ├── ai/
    │   └── mod.rs          # AI 분석 trait 정의 (미구현)
    └── ui/
        └── mod.rs          # UI 확장용
```

## 아키텍처

```
메인 스레드 (winit 이벤트 루프 + wgpu 렌더링 + egui)
    │
    │ crossbeam channel
    │
디먹스/디코딩 스레드
    ├── FFmpeg 패킷 읽기
    ├── 비디오 디코딩 (HW/SW) → 프레임 큐 → 메인 스레드
    └── 오디오 디코딩 → 오디오 큐 → 피드 스레드
                                        │
                                   오디오 피드 스레드
                                        │
                                   cpal 콜백 (실시간)
```

동기화: 오디오 콜백이 소비한 샘플 수 기반 마스터 클럭. 오디오 정지 시 벽시계 자동 전환.

## 의존성

| 크레이트 | 용도 | 크로스플랫폼 |
|---------|------|------------|
| ffmpeg-next 8.1 | 비디오/오디오 디코딩 | macOS, Windows, Linux |
| wgpu 24 | GPU 렌더링 | Metal, DX12, Vulkan |
| egui 0.31 | UI 오버레이 | 전체 |
| cpal 0.15 | 오디오 출력 | CoreAudio, WASAPI, ALSA |
| winit 0.30 | 윈도우 관리 | 전체 |
| rusqlite 0.32 | SQLite (bundled) | 전체 |
| rfd 0.15 | 파일 대화상자 | 전체 |
| crossbeam-channel | 스레드 간 통신 | 전체 |
| parking_lot | 뮤텍스 | 전체 |

## 알려진 제한사항

- 4K 60fps 소프트웨어 디코딩은 성능이 부족할 수 있음 (HW 디코딩 사용 권장)
- 배속 재생 시 오디오는 1x로 재생됨 (타임스트레치 미구현)
- 카메라 캡처는 인터페이스만 정의됨 (구현 예정)
- AI 분석 9종은 보류 상태 (trait 정의만 존재)

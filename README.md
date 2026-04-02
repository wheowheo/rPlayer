# rPlayer

Rust 크로스플랫폼 비디오 플레이어.  
FFmpeg 8.1 + wgpu + egui 기반. 하드웨어 가속 디코딩. 4K 60fps.

---

## 설치

모든 라이브러리(FFmpeg, OpenSSL, libvpx 등)가 앱에 내장되어 있다.  
**외부 의존성 설치 없이** 바로 실행 가능.

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

완료 후:
```bash
# 앱으로 실행
open ~/Applications/rPlayer.app

# 터미널에서 실행
~/Applications/rPlayer.app/Contents/MacOS/rplayer video.mp4

# PATH에 등록하면 어디서든 실행 가능
sudo ln -sf ~/Applications/rPlayer.app/Contents/MacOS/rplayer /usr/local/bin/rplayer
rplayer video.mp4
```

제거:
```bash
rm -rf ~/Applications/rPlayer.app
sudo rm -f /usr/local/bin/rplayer
```

### Windows

1. [Releases](https://github.com/wheowheo/rPlayer/releases)에서 `rplayer-windows-x64.zip` 다운로드
2. 압축 해제 (FFmpeg DLL 동봉)
3. `rplayer.exe video.mp4` 실행

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

완료 후:
```bash
rplayer video.mp4
```

> `~/.local/bin`이 PATH에 없으면: `export PATH=$HOME/.local/bin:$PATH`

제거:
```bash
rm -rf ~/.local/share/rplayer ~/.local/bin/rplayer
```

---

## 사용법

### 파일 열기

| 방법 | 조작 |
|------|------|
| 커맨드라인 | `rplayer video.mp4` |
| 키보드 | `O` 키 → 파일 선택 대화상자 |
| 드래그 앤 드롭 | 윈도우에 파일 끌어다 놓기 |
| 메뉴 | 파일 > 열기 |
| 우클릭 | 파일 열기 |

### 단축키

| 키 | 기능 |
|----|------|
| `Space` | 재생 / 일시정지 |
| `Esc` | 정지 (처음으로) |
| `F` | 1프레임 전진 |
| `←` `→` | 5초 뒤로 / 앞으로 |
| `↑` `↓` | 볼륨 5% 증가 / 감소 |
| `[` `]` | 배속 0.25x 감소 / 증가 (피치 보존) |
| `M` | 음소거 토글 |
| `R` | HW ↔ SW 디코더 전환 |
| `Tab` | 정보 오버레이 (FPS, 드롭률, 오디오 시각화) |
| `O` | 파일 열기 |
| `+` `-` | 자막 싱크 ±0.5초 |

### 메뉴

| 메뉴 | 항목 |
|------|------|
| **파일** | 열기 |
| **재생** | 재생/일시정지, 정지, 5초 탐색, 배속 조절 |
| **오디오** | 볼륨, 음소거, 이퀄라이저 (Bass/Mid/Treble), 컴프레서 |
| **보기** | 정보 오버레이, 디코더 전환, 라이브러리 정보 |

우클릭(트랙패드 두 손가락)으로 컨텍스트 메뉴.

### 컨트롤 바

하단에 위치. seekbar 클릭/드래그로 탐색. 재생/정지/되감기/빨리감기 벡터 아이콘 버튼, 볼륨 슬라이더, 배속 표시.

### 정보 오버레이 (Tab)

| 위치 | 표시 내용 |
|------|----------|
| 좌상단 | 코덱, 해상도, FPS, 렌더 FPS |
| 좌상단 | 시간, 배속, 볼륨, 디코더 모드 |
| 좌상단 | 프레임 드롭률 (색상 코딩) |
| 우상단 | L/R 오디오 레벨 미터 |
| 우상단 | PCM 파형 오실로스코프 |

---

## 기능

### 비디오 재생
- FFmpeg 8.1이 지원하는 모든 코덱/컨테이너
- H.264, H.265, VP9, AV1, MPEG-4, WMV, FLV 등
- MP4, MKV, AVI, MOV, WebM, TS, FLV, WMV 등

### 하드웨어 디코딩
- macOS: VideoToolbox (H.264/H.265/VP9)
- Windows: D3D11VA (H.264/H.265)
- Linux: VAAPI (H.264/H.265/VP9)
- `R` 키로 실시간 HW ↔ SW 전환
- 실패 시 자동 SW 폴백

### GPU 렌더링
- wgpu (Metal/DX12/Vulkan) 기반
- YUV420P/NV12 텍스처를 GPU에 직접 업로드
- WGSL 프래그먼트 셰이더에서 YUV→RGB 변환 (CPU 부하 0)
- 4K 60fps 안정 렌더링

### 오디오
- cpal 크로스플랫폼 출력 (CoreAudio/WASAPI/ALSA)
- 48kHz 스테레오 f32 리샘플링
- 볼륨 0~200%, 음소거

### 오디오 DSP
- **타임스트레치**: rubato FFT 리샘플러로 배속 재생 시 피치 보존 (0.25x~4.0x)
- **3밴드 이퀄라이저**: Biquad IIR 필터 — Bass 200Hz, Mid 1kHz, Treble 4kHz (-12~+12dB)
- **컴프레서**: 엔벨로프 팔로워 기반 다이나믹 레인지 압축

### A/V 동기화
- PTS 기반 오디오 마스터 클럭
- 오디오 스톨 시 벽시계 자동 전환
- seek 후 clock freeze → 첫 프레임 표시 후 resume

### 자막
- SRT, SMI 형식 지원
- 비디오와 같은 이름의 자막 파일 자동 감지
- `+`/`-`로 싱크 조절

---

## 소스에서 빌드

개발자이거나 직접 빌드하고 싶은 경우.

### 요구사항
- Rust 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- FFmpeg 8.x 개발 라이브러리
- pkg-config

### macOS
```bash
brew install ffmpeg pkgconf
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release

# 독립 실행 .app 번들 생성 (FFmpeg 내장, 배포용)
bash scripts/bundle.sh
```

### Windows (MSVC)
```powershell
vcpkg install ffmpeg:x64-windows
set VCPKG_ROOT=C:\vcpkg
choco install pkgconfiglite

git clone https://github.com/wheowheo/rPlayer.git
cd rPlayer
cargo build --release
```

### Ubuntu / Debian
```bash
sudo apt install -y libavcodec-dev libavformat-dev libavutil-dev \
    libavfilter-dev libavdevice-dev libswscale-dev libswresample-dev \
    pkg-config libasound2-dev libvulkan-dev
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
```

### Fedora
```bash
sudo dnf install -y ffmpeg-devel pkgconf alsa-lib-devel vulkan-loader-devel
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
```

### Arch Linux
```bash
sudo pacman -S ffmpeg pkgconf alsa-lib vulkan-icd-loader
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
```

---

## 스크립트

| 스크립트 | 용도 |
|---------|------|
| `scripts/install.sh` | 원라인 설치 (curl 파이프) |
| `scripts/build.sh release` | 릴리스 빌드 |
| `scripts/bundle.sh` | 독립 실행 .app 번들 (dylib 내장) |
| `scripts/package.sh` | tar.gz 아카이브 생성 |
| `scripts/test.sh` | 기능 검증 테스트 (28항목) |
| `scripts/version.sh` | 버전 관리 + 릴리스 |
| `scripts/clean.sh` | 빌드 정리 |

---

## 기술 스택

| 영역 | 라이브러리 | 역할 |
|------|----------|------|
| 디코딩 | ffmpeg-next 8.1 | 비디오/오디오 디코딩, HW 가속 |
| GPU | wgpu 24 | Metal / DX12 / Vulkan 렌더링 |
| 윈도우 | winit 0.30 | 크로스플랫폼 윈도우 + 이벤트 |
| UI | egui 0.31 | 즉시 모드 GUI |
| 오디오 출력 | cpal 0.15 | CoreAudio / WASAPI / ALSA |
| 배속 | rubato 0.16 | FFT 피치 보존 타임스트레치 |
| EQ/컴프레서 | 자체 구현 | Biquad IIR, 엔벨로프 팔로워 |
| DB | rusqlite 0.32 | SQLite (bundled) |

전체 라이브러리 상세: [LIBRARIES.md](LIBRARIES.md)

---

## 문서

| 파일 | 내용 |
|------|------|
| [MANUAL.md](MANUAL.md) | 사용 매뉴얼 |
| [LIBRARIES.md](LIBRARIES.md) | 라이브러리 20개 상세 설명 |
| [TESTPLAN.md](TESTPLAN.md) | 기능 검증 플랜 (자동 28 + 수동 21항목) |
| [VERSIONING.md](VERSIONING.md) | 버전 관리 가이드 |
| [TRYANDERROR.md](TRYANDERROR.md) | 개발 과정 삽질 기록 11건 |

---

## 라이선스

MIT

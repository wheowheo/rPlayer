# rPlayer

Rust 크로스플랫폼 비디오 플레이어.  
FFmpeg 8.1 + wgpu + egui 기반. 하드웨어 가속 디코딩. 4K 60fps.

[![CI](https://github.com/wheowheo/rPlayer/actions/workflows/ci.yml/badge.svg)](https://github.com/wheowheo/rPlayer/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wheowheo/rPlayer)](https://github.com/wheowheo/rPlayer/releases/latest)

---

## 설치

모든 라이브러리(FFmpeg, OpenSSL, libvpx 등)가 앱에 내장.  
**추가 설치 없이** 바로 실행 가능.

### macOS (Apple Silicon)

```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

설치 후:
```bash
open ~/Applications/rPlayer.app                              # GUI
~/Applications/rPlayer.app/Contents/MacOS/rplayer video.mp4  # CLI

# 터미널 어디서든 실행하려면
sudo ln -sf ~/Applications/rPlayer.app/Contents/MacOS/rplayer /usr/local/bin/rplayer
rplayer video.mp4
```

제거:
```bash
rm -rf ~/Applications/rPlayer.app /usr/local/bin/rplayer
```

### Windows (x64)

[최신 릴리스](https://github.com/wheowheo/rPlayer/releases/latest)에서 **rplayer-windows-x64.zip** 다운로드.

```powershell
# 1. 압축 해제 (FFmpeg DLL 포함)
Expand-Archive rplayer-windows-x64.zip -DestinationPath C:\rPlayer

# 2. 실행
C:\rPlayer\rplayer.exe video.mp4

# PATH에 추가 (선택)
setx PATH "%PATH%;C:\rPlayer"
rplayer video.mp4
```

제거: `C:\rPlayer` 폴더 삭제.

### Linux (x64)

```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

설치 후:
```bash
rplayer video.mp4
```

> FFmpeg 런타임 라이브러리 필요:  
> Ubuntu: `sudo apt install libavcodec60 libavformat60 libswscale7 libswresample4`  
> Fedora: `sudo dnf install ffmpeg-libs`

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
| 키보드 | `O` → 파일 선택 대화상자 |
| 드래그 앤 드롭 | 윈도우에 파일 끌어다 놓기 |
| 메뉴 | 파일 > 열기 |
| 우클릭 | 파일 열기 |

### 단축키

| 키 | 기능 |
|----|------|
| `Space` | 재생 / 일시정지 |
| `Esc` | 정지 (처음으로) |
| `F` | 1프레임 전진 |
| `←` `→` | 5초 탐색 |
| `↑` `↓` | 볼륨 ±5% |
| `[` `]` | 배속 ±0.25x (피치 보존) |
| `M` | 음소거 |
| `R` | HW ↔ SW 디코더 전환 |
| `Tab` | 정보 오버레이 |
| `O` | 파일 열기 |
| `+` `-` | 자막 싱크 ±0.5초 |

### 메뉴

| 메뉴 | 항목 |
|------|------|
| **파일** | 열기 |
| **재생** | 재생/정지, 탐색, 배속 |
| **오디오** | 볼륨, 음소거, EQ (Bass/Mid/Treble), 컴프레서 |
| **보기** | 정보 오버레이, 디코더 전환, 라이브러리 정보 |

우클릭(트랙패드 두 손가락)으로 컨텍스트 메뉴.

### 컨트롤 바

하단: seekbar + 재생/정지/탐색 아이콘 + 볼륨 슬라이더 + 배속 표시.

### 정보 오버레이 (Tab)

| 위치 | 내용 |
|------|------|
| 좌상단 | 코덱, 해상도, FPS, 렌더 FPS, 시간, 배속, 디코더 모드 |
| 좌상단 | 프레임 드롭률 (초록/노랑/빨강) |
| 우상단 | L/R 오디오 레벨 미터 + PCM 파형 |

---

## 기능

### 비디오
- H.264, H.265, VP9, AV1, MPEG-4, WMV, FLV 등
- MP4, MKV, AVI, MOV, WebM, TS, FLV, WMV 등
- 4K 60fps 안정 재생

### 하드웨어 디코딩
| OS | 가속 | 코덱 |
|----|------|------|
| macOS | VideoToolbox | H.264/H.265/VP9 |
| Windows | D3D11VA | H.264/H.265 |
| Linux | VAAPI | H.264/H.265/VP9 |

`R` 키로 실시간 전환. 실패 시 자동 SW 폴백.

### GPU 렌더링
- wgpu — Metal (macOS) / DX12 (Windows) / Vulkan (Linux)
- YUV420P/NV12 텍스처 GPU 업로드 + WGSL 셰이더 변환
- CPU 컬러 변환 부하 0

### 오디오 DSP
- **타임스트레치**: 배속 재생 시 피치 보존 (0.25x~4.0x, rubato FFT)
- **3밴드 EQ**: Bass 200Hz / Mid 1kHz / Treble 4kHz (±12dB, Biquad IIR)
- **컴프레서**: 다이나믹 레인지 압축

### A/V 동기화
- PTS 오디오 마스터 클럭
- seek 후 clock freeze → 비디오 도착 → 오디오 동시 시작

### 자막
- SRT / SMI 자동 감지
- `+`/`-`로 싱크 조절

---

## 소스에서 빌드

### macOS
```bash
brew install ffmpeg pkgconf
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
bash scripts/bundle.sh  # 독립 실행 .app (FFmpeg 내장)
```

### Windows (MSVC)
```powershell
vcpkg install ffmpeg:x64-windows
set VCPKG_ROOT=C:\vcpkg
choco install pkgconfiglite
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
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
| `scripts/install.sh` | 원라인 설치 (curl) |
| `scripts/build.sh release` | 릴리스 빌드 |
| `scripts/bundle.sh` | 독립 실행 .app (dylib 내장) |
| `scripts/package.sh` | tar.gz 아카이브 |
| `scripts/test.sh` | 기능 검증 (28항목) |
| `scripts/version.sh` | 버전 + 릴리스 자동화 |
| `scripts/clean.sh` | 빌드 정리 |

---

## 기술 스택

| 영역 | 라이브러리 | 역할 |
|------|----------|------|
| 디코딩 | ffmpeg-next 8.1 | 비디오/오디오 디코딩, HW 가속 |
| GPU | wgpu 24 | Metal / DX12 / Vulkan |
| 윈도우 | winit 0.30 | 크로스플랫폼 윈도우 |
| UI | egui 0.31 | 즉시 모드 GUI |
| 오디오 | cpal 0.15 | CoreAudio / WASAPI / ALSA |
| 배속 | rubato 0.16 | FFT 타임스트레치 |
| EQ/압축 | 자체 구현 | Biquad IIR, 엔벨로프 팔로워 |
| DB | rusqlite 0.32 | SQLite (bundled) |

상세: [LIBRARIES.md](LIBRARIES.md)

---

## 문서

| 문서 | 내용 |
|------|------|
| [MANUAL.md](MANUAL.md) | 사용 매뉴얼 |
| [LIBRARIES.md](LIBRARIES.md) | 라이브러리 20개 상세 |
| [TESTPLAN.md](TESTPLAN.md) | 검증 플랜 (자동 28 + 수동 21) |
| [VERSIONING.md](VERSIONING.md) | 버전 관리 |
| [TRYANDERROR.md](TRYANDERROR.md) | 개발 삽질 기록 |

---

## 라이선스

MIT

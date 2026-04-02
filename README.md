# rPlayer

Rust 크로스플랫폼 비디오 플레이어.  
FFmpeg 8.1 + wgpu + egui 기반, 하드웨어 가속 디코딩, 4K 60fps 재생.

## 기능

- H.264, H.265, VP9, AV1 등 모든 FFmpeg 지원 코덱 재생
- VideoToolbox (macOS) / D3D11VA (Windows) / VAAPI (Linux) 하드웨어 디코딩
- GPU YUV→RGB 셰이더 렌더링 (4K 60fps 무드롭)
- PTS 기반 A/V 동기화 (오디오 마스터 클럭)
- 오디오 DSP: 피치 보존 배속 재생, 3밴드 EQ, 컴프레서
- SRT/SMI 자막 (자동 감지, 싱크 조절)
- 메뉴바 + 컨트롤 바 + 우클릭 메뉴 + 벡터 아이콘
- 오디오 비주얼라이저 (L/R 레벨 미터 + PCM 파형)
- 드래그 앤 드롭 파일 열기

---

## 설치

### macOS (Apple Silicon / Intel)

#### Homebrew로 의존성 설치
```bash
brew install ffmpeg pkgconf
```

#### 사전 빌드 바이너리 (Apple Silicon)
[Releases](https://github.com/wheowheo/rPlayer/releases)에서 다운로드:
```bash
# tar.gz 다운로드 후
tar xzf rplayer-*-macos-arm64.tar.gz
./rplayer video.mp4
```

#### .app 번들
```bash
tar xzf rplayer-*.app.tar.gz
# rPlayer-0.2.0.app을 Applications 폴더로 이동
# 또는 더블클릭으로 실행
```

#### 소스에서 빌드
```bash
# Rust 설치 (없는 경우)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 의존성
brew install ffmpeg pkgconf

# 빌드
git clone https://github.com/wheowheo/rPlayer.git
cd rPlayer
cargo build --release

# 실행
./target/release/rplayer video.mp4
```

---

### Windows (x64)

#### 1. Rust 설치
https://rustup.rs 에서 `rustup-init.exe` 다운로드 후 실행.  
Visual Studio Build Tools (MSVC)가 필요하다.

#### 2. FFmpeg 설치

**방법 A — vcpkg (권장)**
```powershell
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install ffmpeg:x64-windows

# 환경변수 설정
set VCPKG_ROOT=C:\vcpkg
```

**방법 B — 사전 빌드 패키지**
1. https://github.com/GyanD/codexffmpeg/releases 에서 `ffmpeg-*-full_build-shared.zip` 다운로드
2. 압축 해제 후 환경변수 설정:
```powershell
set FFMPEG_DIR=C:\ffmpeg
set PATH=%FFMPEG_DIR%\bin;%PATH%
```

#### 3. pkgconf 설치
```powershell
choco install pkgconfiglite
```
또는 https://github.com/pkgconf/pkgconf/releases 에서 수동 설치.

#### 4. 빌드
```powershell
git clone https://github.com/wheowheo/rPlayer.git
cd rPlayer
cargo build --release
.\target\release\rplayer.exe video.mp4
```

#### 사전 빌드 바이너리
[Releases](https://github.com/wheowheo/rPlayer/releases)에서 `rplayer-windows-x64.zip` 다운로드.  
FFmpeg DLL이 동봉되어 있으므로 별도 설치 불필요.

---

### Linux (Ubuntu/Debian)

#### 의존성 설치
```bash
sudo apt update
sudo apt install -y \
    libavcodec-dev libavformat-dev libavutil-dev \
    libswscale-dev libswresample-dev \
    pkg-config libasound2-dev \
    libvulkan-dev
```

#### Fedora/RHEL
```bash
sudo dnf install -y \
    ffmpeg-devel pkgconf \
    alsa-lib-devel vulkan-loader-devel
```

#### Arch Linux
```bash
sudo pacman -S ffmpeg pkgconf alsa-lib vulkan-icd-loader
```

#### Rust 설치 + 빌드
```bash
# Rust (없는 경우)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 빌드
git clone https://github.com/wheowheo/rPlayer.git
cd rPlayer
cargo build --release

# 실행
./target/release/rplayer video.mp4
```

#### 사전 빌드 바이너리
[Releases](https://github.com/wheowheo/rPlayer/releases)에서 `rplayer-linux-x64.tar.gz` 다운로드.  
시스템에 FFmpeg 런타임 라이브러리가 필요:
```bash
sudo apt install libavcodec60 libavformat60 libswscale7 libswresample4
```

---

## 사용법

```bash
# 파일 재생
rplayer video.mp4

# 로그 출력
RUST_LOG=rplayer=info rplayer video.mp4

# 디버그 로그 (프레임 드롭, fps)
RUST_LOG=rplayer=debug rplayer video.mp4
```

### 단축키

| 키 | 기능 |
|----|------|
| `Space` | 재생 / 일시정지 |
| `Esc` | 정지 |
| `O` | 파일 열기 |
| `F` | 1프레임 전진 |
| `R` | HW/SW 디코더 전환 |
| `Tab` | 정보 오버레이 (FPS, 드롭률, 오디오 시각화) |
| `M` | 음소거 |
| `←` `→` | 5초 탐색 |
| `↑` `↓` | 볼륨 ±5% |
| `[` `]` | 배속 ±0.25x (피치 보존) |
| `+` `-` | 자막 싱크 ±0.5초 |

### 파일 열기
- `O` 키 → 파일 선택 대화상자
- 파일을 윈도우에 드래그 앤 드롭
- 커맨드라인 인자: `rplayer file.mp4`

### 메뉴
- **파일** > 열기
- **재생** > 재생/정지/탐색/배속
- **오디오** > 볼륨/음소거/이퀄라이저/컴프레서
- **보기** > 정보 오버레이/디코더 전환/라이브러리 정보
- **우클릭** > 컨텍스트 메뉴 (트랙패드 두 손가락 클릭)

---

## 빌드 스크립트

```bash
bash scripts/build.sh release    # 릴리스 빌드
bash scripts/package.sh          # 패키징 (tar.gz + .app)
bash scripts/test.sh             # 기능 검증 테스트 (28항목)
bash scripts/version.sh minor --release  # 버전 올리기 + 릴리스
bash scripts/clean.sh            # 빌드 정리
```

---

## 지원 포맷

| 컨테이너 | 비디오 코덱 | 오디오 코덱 |
|----------|-----------|-----------|
| MP4, MOV, M4V | H.264, H.265 | AAC, MP3 |
| MKV | H.264, H.265, VP9, AV1 | AAC, Opus, FLAC |
| WebM | VP9, AV1 | Opus, Vorbis |
| AVI | H.264, MPEG-4 | MP3, PCM |
| FLV, TS, WMV | H.264, H.265 | AAC, AC3, WMA |

---

## 기술 스택

| 영역 | 라이브러리 | 역할 |
|------|----------|------|
| 디코딩 | ffmpeg-next 8.1 | 비디오/오디오 디코딩 |
| GPU 렌더링 | wgpu 24 | Metal/DX12/Vulkan |
| UI | egui 0.31 | 메뉴, 컨트롤, 오버레이 |
| 오디오 | cpal 0.15 | CoreAudio/WASAPI/ALSA |
| 배속 | rubato 0.16 | FFT 피치 보존 타임스트레치 |
| DB | rusqlite 0.32 | SQLite (bundled) |

상세: [LIBRARIES.md](LIBRARIES.md)

---

## 문서

- [MANUAL.md](MANUAL.md) — 사용 매뉴얼
- [LIBRARIES.md](LIBRARIES.md) — 라이브러리 상세
- [TESTPLAN.md](TESTPLAN.md) — 기능 검증 플랜
- [VERSIONING.md](VERSIONING.md) — 버전 관리 가이드
- [TRYANDERROR.md](TRYANDERROR.md) — 개발 삽질 기록

---

## 라이선스

MIT

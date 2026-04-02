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

## 설치 (원라인)

외부 의존성 없이 바로 실행 가능. FFmpeg 등 모든 라이브러리가 앱에 내장.

### macOS
```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

설치 후:
```bash
open ~/Applications/rPlayer.app                              # GUI 실행
~/Applications/rPlayer.app/Contents/MacOS/rplayer video.mp4  # CLI 실행

# PATH에 추가 (선택)
sudo ln -sf ~/Applications/rPlayer.app/Contents/MacOS/rplayer /usr/local/bin/rplayer
rplayer video.mp4
```

### Windows
[Releases](https://github.com/wheowheo/rPlayer/releases)에서 `rplayer-windows-x64.zip` 다운로드.  
FFmpeg DLL 동봉 — 별도 설치 불필요.
```powershell
# 압축 해제 후
.\rplayer.exe video.mp4
```

### Linux
```bash
curl -fsSL https://raw.githubusercontent.com/wheowheo/rPlayer/main/scripts/install.sh | bash
```

설치 후:
```bash
rplayer video.mp4
```

---

## 소스에서 빌드 (개발자용)

### macOS
```bash
brew install ffmpeg pkgconf
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
bash scripts/bundle.sh  # FFmpeg 내장 .app 생성
```

### Windows
```powershell
# vcpkg로 FFmpeg 설치
vcpkg install ffmpeg:x64-windows
set VCPKG_ROOT=C:\vcpkg
choco install pkgconfiglite

git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
```

### Linux (Ubuntu/Debian)
```bash
sudo apt install -y libavcodec-dev libavformat-dev libavutil-dev \
    libswscale-dev libswresample-dev pkg-config libasound2-dev libvulkan-dev

git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
```

### Linux (Fedora)
```bash
sudo dnf install -y ffmpeg-devel pkgconf alsa-lib-devel vulkan-loader-devel
git clone https://github.com/wheowheo/rPlayer.git && cd rPlayer
cargo build --release
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

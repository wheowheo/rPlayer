# rPlayer 라이브러리 의존성 상세

rPlayer가 사용하는 모든 라이브러리와 각각이 프로젝트에서 수행하는 역할.

---

## 핵심 라이브러리

### ffmpeg-next 8.1.0
FFmpeg C 라이브러리의 Rust 바인딩. rPlayer의 미디어 엔진 전체를 담당한다.

**사용 위치**: `decode/demuxer.rs`, `decode/video_decoder.rs`, `decode/audio_decoder.rs`

**수행 역할**:
- **컨테이너 파싱** (`libavformat`): MP4, MKV, AVI, WebM, TS 등 컨테이너를 열고 비디오/오디오 스트림을 분리한다. `avformat_open_input`으로 파일을 열고, `av_read_frame`으로 패킷을 읽는다.
- **비디오 디코딩** (`libavcodec`): H.264, H.265, VP9, AV1 등 코덱을 디코딩한다. `avcodec_send_packet` / `avcodec_receive_frame` API를 사용한다.
- **하드웨어 가속** (`libavutil` hwcontext): VideoToolbox(macOS), D3D11VA(Windows), VAAPI(Linux) HW 디코딩을 `av_hwdevice_ctx_create`로 초기화하고, `av_hwframe_transfer_data`로 HW 프레임을 시스템 메모리로 전송한다.
- **오디오 리샘플링** (`libswresample`): 오디오를 f32 packed, 48kHz 스테레오로 변환한다. 원본 코덱의 샘플 포맷(fltp, s16 등)과 채널 레이아웃이 다양하기 때문에 통일이 필요하다.
- **픽셀 변환** (`libswscale`): YUV420P/NV12가 아닌 비표준 포맷의 프레임을 YUV420P로 변환하는 폴백 경로에서 사용한다. 일반적인 재생 경로에서는 GPU 셰이더가 대체하므로 거의 호출되지 않는다.
- **탐색** (`avformat_seek_file`): 키프레임 기반 정밀 탐색. AV_TIME_BASE 단위로 변환하여 호출한다.

**내부 구조**: `ffmpeg-sys-next` 크레이트가 `bindgen`으로 C 헤더에서 FFI 바인딩을 자동 생성하고, `ffmpeg-next`가 이를 안전한 Rust API로 감싼다. HW 가속 부분은 safe wrapper가 없어서 `unsafe` FFI를 직접 호출한다.

---

### wgpu 24.0.5
WebGPU 표준의 Rust 구현. GPU 렌더링 전체를 담당한다.

**사용 위치**: `app.rs`, `video/renderer.rs`, `video/shader.wgsl`

**수행 역할**:
- **GPU 디바이스 관리**: `Instance` → `Adapter` → `Device` + `Queue` 초기화. `HighPerformance` 어댑터를 선택하여 외장 GPU가 있으면 우선 사용한다.
- **비디오 텍스처 업로드**: 디코딩된 YUV420P 프레임의 Y/U/V 평면을 각각 `R8Unorm` 텍스처로 업로드한다. NV12는 Y(`R8Unorm`) + UV(`Rg8Unorm`)로 업로드한다. `queue.write_texture`로 CPU → GPU 전송.
- **렌더 파이프라인**: 화면 크기 사각형(quad)에 비디오 텍스처를 매핑하는 렌더 파이프라인. 버텍스 셰이더가 풀스크린 쿼드를 생성하고, 프래그먼트 셰이더가 YUV→RGB 변환을 수행한다.
- **서피스 관리**: `Surface`를 생성하고 `present()`로 렌더링 결과를 화면에 출력한다. `AutoVsync` 프레젠트 모드로 디스플레이 주사율에 동기화한다.

**백엔드**: macOS에서는 Metal, Windows에서는 DX12, Linux에서는 Vulkan을 자동 선택한다.

---

### winit 0.30.13
크로스플랫폼 윈도우 생성 및 이벤트 처리 라이브러리.

**사용 위치**: `main.rs`

**수행 역할**:
- **윈도우 생성**: 제목, 크기, DPI 스케일을 설정하여 OS 네이티브 윈도우를 생성한다.
- **이벤트 루프**: `EventLoop::run_app`으로 이벤트 기반 메인 루프를 실행한다. `ApplicationHandler` trait을 구현하여 키보드, 마우스, 리사이즈, 드래그앤드롭, 포커스 이벤트를 처리한다.
- **Redraw 관리**: `request_redraw()`로 렌더링을 요청한다. 재생 중에는 매 프레임마다, 일시정지 시에는 이벤트 발생 시에만 호출한다.
- **입력 이벤트**: `KeyboardInput`(단축키), `MouseInput`(우클릭 메뉴), `DroppedFile`(드래그앤드롭 파일 열기), `MouseWheel`(볼륨) 이벤트를 수신한다.

---

### egui 0.31.1
즉시 모드(immediate mode) GUI 라이브러리. 모든 UI 요소를 담당한다.

**사용 위치**: `app.rs` (`draw_ui` 함수)

**수행 역할**:
- **메뉴바**: 파일/재생/오디오/보기 드롭다운 메뉴.
- **컨트롤바**: 재생/정지/탐색 아이콘 버튼, seekbar, 볼륨 슬라이더, 배속 표시.
- **컨텍스트 메뉴**: 우클릭(트랙패드 두 손가락)으로 팝업되는 메뉴.
- **오버레이**: Tab 정보 오버레이(코덱, FPS, 드롭률), 오디오 비주얼라이저(레벨 미터 + PCM 파형), 자막 표시.
- **윈도우**: 라이브러리 정보 창 (`egui::Window`).
- **커스텀 드로잉**: `Painter` API로 재생/정지 벡터 아이콘, seekbar, 레벨 미터, 파형 오실로스코프를 직접 그린다.

**특징**: 매 프레임마다 전체 UI를 재선언하는 방식. 상태를 외부(`UiState`)에 유지하고, UI 코드는 순수 함수로 작성된다. `UiAction` enum으로 UI 이벤트를 앱 로직에 전달한다.

---

### egui-wgpu 0.31.1
egui와 wgpu를 연결하는 렌더러.

**사용 위치**: `app.rs` (`render` 메서드)

**수행 역할**:
- egui가 생성한 삼각형 메시(`ClippedPrimitive`)를 wgpu 렌더 패스에서 그린다.
- egui의 폰트 아틀라스 텍스처를 wgpu 텍스처로 관리한다.
- 비디오 렌더 패스 위에 egui 렌더 패스를 `LoadOp::Load`로 덮어 그려서 투명 오버레이를 구현한다.
- `forget_lifetime()`으로 렌더 패스의 `'static` 요구사항을 우회한다.

---

### egui-winit 0.31.1
egui와 winit을 연결하는 입력 어댑터.

**사용 위치**: `app.rs`, `main.rs`

**수행 역할**:
- winit의 `WindowEvent`를 egui의 `RawInput`으로 변환한다.
- 마우스 위치, 키보드 입력, DPI 스케일, 클립보드를 egui에 전달한다.
- egui의 `PlatformOutput`(커서 변경, 클립보드 쓰기)을 winit으로 돌려보낸다.
- `repaint` 플래그로 egui가 추가 렌더링을 요청하는지 감지한다.

---

### cpal 0.15.3
크로스플랫폼 오디오 출력 라이브러리.

**사용 위치**: `audio/output.rs`

**수행 역할**:
- **디바이스 선택**: `default_output_device()`로 시스템 기본 오디오 출력 장치를 선택한다.
- **스트림 생성**: `build_output_stream()`으로 48kHz 스테레오 f32 오디오 출력 스트림을 생성한다.
- **실시간 콜백**: 오디오 하드웨어가 데이터를 요청할 때 콜백 함수가 호출된다. 콜백에서 VecDeque 버퍼의 데이터를 출력 버퍼에 복사하고, 볼륨 게인을 적용하고, 피크 레벨과 파형 데이터를 수집한다.

**백엔드**: macOS에서는 CoreAudio, Windows에서는 WASAPI, Linux에서는 ALSA를 자동 선택한다.

**제약**: 콜백은 실시간 스레드에서 호출되므로 힙 할당, 뮤텍스 대기, 시스템 콜을 해서는 안 된다. VecDeque의 `drain`이 O(1)인 이유가 이것이다.

---

## 미디어 처리

### rubato 0.16.2
오디오 리샘플링 및 타임스트레칭 라이브러리.

**사용 위치**: 의존성으로 포함 (배속 재생 시 오디오 피치 보존용으로 예약)

**수행 역할**:
- FFT 기반 고품질 리샘플링을 제공한다.
- 배속 재생 시 오디오 피치를 보존하면서 재생 속도를 변경하는 데 사용될 예정이다.
- 현재는 배속 변경 시 비디오만 가속하고 오디오는 1x로 재생하는 구조이며, rubato 통합은 향후 과제이다.

**특징**: 순수 Rust 구현. C/C++ 의존성이 없어 크로스 컴파일이 용이하다. SoundTouch의 대안.

---

### rusqlite 0.32.1
SQLite 데이터베이스의 Rust 바인딩.

**사용 위치**: `db/clothing.rs`

**수행 역할**:
- 의류 데이터베이스(`clothing.db`)를 관리한다. 스키마: id, name, type, color_hex, opacity, model_file, notes, is_active.
- CRUD 연산: 의류 추가/삭제/조회, 착용 토글, 타입별 필터링.
- `bundled` 피처로 SQLite를 정적 링크하여 시스템 SQLite에 의존하지 않는다.

**DB 위치**: 
- macOS: `~/Library/Application Support/rPlayer/clothing.db`
- Windows: `%APPDATA%\rPlayer\clothing.db`
- Linux: `~/.local/share/rplayer/clothing.db`

---

### rfd 0.15.4
네이티브 파일 대화상자 라이브러리 (Rusty File Dialogs).

**사용 위치**: `app.rs` (`handle_action` → `OpenFile`)

**수행 역할**:
- `FileDialog::new().add_filter("Video", &["mp4","mkv",...]).pick_file()`로 OS 네이티브 파일 열기 대화상자를 표시한다.
- macOS에서는 NSOpenPanel, Windows에서는 IFileOpenDialog, Linux에서는 GTK/KDE 대화상자를 사용한다.

---

### sysinfo 0.33.1
시스템 정보 수집 라이브러리.

**사용 위치**: 의존성으로 포함 (리소스 모니터용으로 예약)

**수행 역할**:
- CPU 사용률, 메모리 사용량, 프로세스 정보를 수집한다.
- Tab 오버레이의 리소스 모니터 확장에 사용될 예정이다.

---

## 유틸리티

### crossbeam-channel 0.5.15
고성능 다중 생산자/소비자 채널.

**사용 위치**: `media/pipeline.rs`, `audio/output.rs`

**수행 역할**:
- **비디오 프레임 큐**: `bounded(3)` 채널로 디코드 스레드 → 메인 스레드 간 `RawFrame`을 전달한다. `send_timeout(50ms)`으로 큐가 가득 차면 짧게 대기하되 무한 블록은 방지한다.
- **오디오 데이터 큐**: `bounded(32)` 채널로 디코드 스레드 → 오디오 피드 스레드 간 `DecodedAudio`를 전달한다.
- **커맨드 채널**: `bounded(16)` 채널로 메인 스레드 → 디코드 스레드 간 재생 제어 명령(Play/Pause/Seek/Stop/SetDecodeMode)을 전달한다.

**선택 이유**: `std::sync::mpsc`와 달리 bounded 채널로 backpressure를 제공하고, `send_timeout` / `try_send` / `try_recv`를 지원한다.

---

### parking_lot 0.12.5
고성능 동기화 프리미티브 (Mutex, RwLock, Condvar).

**사용 위치**: `audio/output.rs`

**수행 역할**:
- 오디오 버퍼(`VecDeque<f32>`)를 `Mutex`로 보호한다. 오디오 피드 스레드(쓰기)와 cpal 콜백 스레드(읽기) 간 동기화.
- 오디오 비주얼라이저 데이터(`AudioVis`)를 `Mutex`로 보호한다. cpal 콜백(쓰기)과 메인 스레드(읽기) 간 동기화. `try_lock`으로 실시간 스레드 블로킹을 방지한다.

**선택 이유**: `std::sync::Mutex`보다 가볍고, `Poisoning`이 없어서 패닉 후에도 락을 사용할 수 있다. `try_lock`이 `Option`을 반환하여 실시간 스레드에서 안전하게 사용 가능하다.

---

### bytemuck 1.25.0
안전한 바이트 수준 타입 변환.

**사용 위치**: `video/renderer.rs`

**수행 역할**:
- 버텍스 데이터(`Vertex` 구조체)를 `&[u8]`로 변환하여 wgpu 버퍼에 업로드한다.
- `#[derive(Pod, Zeroable)]`로 구조체가 바이트 안전함을 컴파일 타임에 보장한다.
- `bytemuck::cast_slice(QUAD_VERTICES)`로 `&[Vertex]` → `&[u8]` 변환.

---

### anyhow 1.0.102
유연한 에러 처리 라이브러리.

**사용 위치**: 프로젝트 전반 (`anyhow::Result`)

**수행 역할**:
- `anyhow::Result<T>`로 다양한 에러 타입(FFmpeg, IO, wgpu, cpal)을 하나의 반환 타입으로 통합한다.
- `context()`로 에러에 설명을 추가한다.
- 주로 초기화 코드와 파이프라인 스레드에서 사용한다.

---

### thiserror 2.0.18
에러 타입 정의 매크로.

**사용 위치**: `error.rs`

**수행 역할**:
- `#[derive(Error)]`로 `PlayerError` enum의 `Display` + `From` 구현을 자동 생성한다.
- `Ffmpeg`, `Window`, `Gpu`, `Audio`, `Database`, `Io` 등 에러 변형을 정의한다.

---

### log 0.4.29
로깅 파사드 (인터페이스).

**사용 위치**: 프로젝트 전반

**수행 역할**:
- `log::info!`, `log::debug!`, `log::warn!`, `log::error!` 매크로로 로그를 기록한다.
- 실제 출력은 `env_logger`가 담당하고, `log`는 인터페이스만 제공한다.
- 주요 로그: GPU 어댑터 선택, HW 디코더 초기화, 파일 열기, 프레임 드롭, 오디오 스톨.

---

### env_logger 0.11.10
환경변수 기반 로그 출력 구현체.

**사용 위치**: `main.rs` (`env_logger::init()`)

**수행 역할**:
- `RUST_LOG` 환경변수로 로그 레벨을 제어한다.
  - `RUST_LOG=rplayer=info`: 일반 정보
  - `RUST_LOG=rplayer=debug`: 프레임 드롭, 렌더 FPS 포함
  - `RUST_LOG=rplayer=warn`: 경고만

---

### pollster 0.4.0
경량 async 블로킹 실행기.

**사용 위치**: `main.rs`

**수행 역할**:
- `pollster::block_on(App::new(window))` — wgpu의 `request_adapter`와 `request_device`가 async이므로, 이를 동기적으로 실행한다.
- tokio 같은 대형 런타임 없이 단일 future를 블로킹 실행하는 최소 구현이다.

---

### ringbuf 0.4.8
락프리 단일 생산자-단일 소비자 링 버퍼.

**사용 위치**: 의존성으로 포함 (향후 오디오 경로 최적화용)

**수행 역할**:
- 현재는 `VecDeque` + `Mutex`를 사용하지만, 오디오 콜백의 실시간 요구사항이 더 엄격해지면 락프리 링 버퍼로 교체할 예정이다.
- SPSC(단일 생산자-단일 소비자) 구조로 뮤텍스 없이 스레드 간 데이터를 전달할 수 있다.

---

## 시스템 프레임워크

FFmpeg과 cpal, wgpu가 내부적으로 링크하는 OS 네이티브 프레임워크.

### macOS
| 프레임워크 | 용도 |
|-----------|------|
| **VideoToolbox** | H.264/H.265/VP9 하드웨어 디코딩 |
| **CoreAudio** | 오디오 출력 (cpal 백엔드) |
| **Metal** | GPU 렌더링 (wgpu 백엔드) |
| **AppKit** | 윈도우 관리, 이벤트 처리 (winit 백엔드) |
| **CoreMedia** | 미디어 타이밍, 샘플 버퍼 |
| **CoreVideo** | 비디오 프레임 관리, IOSurface |
| **CoreFoundation** | 기본 데이터 타입, 메모리 관리 |
| **Security** | TLS/보안 (네트워크 미사용이지만 링크됨) |
| **QuartzCore** | CoreAnimation 레이어 |

### Windows
| 프레임워크 | 용도 |
|-----------|------|
| **D3D11VA** | H.264/H.265 하드웨어 디코딩 |
| **WASAPI** | 오디오 출력 (cpal 백엔드) |
| **Direct3D 12** | GPU 렌더링 (wgpu 백엔드) |
| **Win32** | 윈도우 관리 (winit 백엔드) |

### Linux
| 프레임워크 | 용도 |
|-----------|------|
| **VAAPI** | 하드웨어 디코딩 |
| **ALSA** | 오디오 출력 (cpal 백엔드) |
| **Vulkan** | GPU 렌더링 (wgpu 백엔드) |
| **X11 / Wayland** | 윈도우 관리 (winit 백엔드) |

---

## 빌드 전용 의존성

빌드 시에만 사용되고 런타임 바이너리에는 포함되지 않는 도구.

| 도구 | 용도 |
|------|------|
| **bindgen** | FFmpeg C 헤더 → Rust FFI 바인딩 자동 생성 |
| **cc** | C/C++ 소스 컴파일 (SQLite bundled 빌드) |
| **pkg-config** | FFmpeg 라이브러리 경로 탐색 |

---

## 의존성 트리 요약

```
rplayer
├── ffmpeg-next ─── ffmpeg-sys-next (bindgen → FFmpeg C)
├── wgpu ────────── wgpu-hal (Metal/DX12/Vulkan) + naga (셰이더 컴파일)
├── winit ───────── OS 네이티브 윈도우 API
├── egui ────────── epaint (래스터라이저) + emath
├── egui-wgpu ───── egui + wgpu 연결
├── egui-winit ──── egui + winit 연결
├── cpal ────────── OS 네이티브 오디오 API
├── rusqlite ────── libsqlite3-sys (bundled SQLite)
├── crossbeam-channel
├── parking_lot
└── (기타 유틸리티)
```

총 직접 의존성: 20개 크레이트  
총 간접 의존성: ~180개 크레이트 (Cargo.lock 기준)  
릴리스 바이너리 크기: ~8MB (strip + LTO)

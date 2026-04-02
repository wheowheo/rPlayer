# 버전 관리 가이드

## 버전 체계

[Semantic Versioning 2.0.0](https://semver.org) 준수.

```
MAJOR.MINOR.PATCH
```

| 구분 | 올리는 경우 | 예시 |
|------|-----------|------|
| **MAJOR** | 호환되지 않는 구조 변경 | 렌더링 엔진 교체, 설정 파일 포맷 변경 |
| **MINOR** | 기능 추가 (하위 호환) | AI 분석, 카메라, 새 코덱 지원 |
| **PATCH** | 버그 수정, 최적화 | sync 수정, 드롭 개선, UI 수정 |

## 버전 관리 스크립트

```bash
# 현재 버전 확인
bash scripts/version.sh

# 패치 버전 올리기 (0.2.0 → 0.2.1)
bash scripts/version.sh patch

# 마이너 버전 올리기 (0.2.1 → 0.3.0)
bash scripts/version.sh minor

# 메이저 버전 올리기 (0.3.0 → 1.0.0)
bash scripts/version.sh major

# 직접 지정
bash scripts/version.sh set 1.0.0

# 태그 + push
bash scripts/version.sh patch --tag

# 태그 + 빌드 + 패키징 + GitHub Release 한 번에
bash scripts/version.sh minor --release
```

## 릴리스 절차

### 자동 (권장)

```bash
# 1. 테스트 통과 확인
bash scripts/test.sh

# 2. 버전 올리기 + 빌드 + GitHub Release
bash scripts/version.sh minor --release
```

이 명령 하나로:
1. Cargo.toml 버전 업데이트
2. git commit + tag
3. git push origin main + tag
4. cargo build --release
5. scripts/package.sh (tar.gz + .app)
6. gh release create (GitHub Releases에 바이너리 업로드)

### 수동

```bash
# 1. Cargo.toml version 수정
# 2. 커밋
git add Cargo.toml Cargo.lock
git commit -m "v0.3.0 버전 업데이트"

# 3. 태그
git tag v0.3.0
git push origin main
git push origin v0.3.0

# 4. 빌드 + 패키징
bash scripts/build.sh release
bash scripts/package.sh

# 5. GitHub Release
gh release create v0.3.0 dist/*.tar.gz --title "v0.3.0" --generate-notes
```

## CI 자동 릴리스

`v*` 태그를 push하면 GitHub Actions가 자동으로:
1. macOS (ARM64 + x86_64) 빌드
2. Windows (x64) 빌드
3. Linux (x64) 빌드
4. GitHub Releases에 3개 OS 바이너리 업로드

```bash
# CI 릴리스 트리거
git tag v0.3.0
git push origin v0.3.0
# → Actions가 자동 빌드 + 릴리스
```

## 릴리스 이력

| 버전 | 날짜 | 주요 변경 |
|------|------|----------|
| v0.1.0 | 2026-03-31 | 초기 릴리스 — 기본 재생, HW 디코딩, UI |
| v0.2.0 | 2026-04-02 | DSP 체인, 코드 리뷰 수정, 테스트 자동화, seekbar 개선 |

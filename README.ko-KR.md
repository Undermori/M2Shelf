<p align="center">
  <img src="./src-tauri/icons/128x128.png" width="96" height="96" alt="M²Shelf Logo">
</p>

<h1 align="center">M²Shelf</h1>

<p align="center"><strong>MORI MEDIA SHELF</strong></p>

<p align="center"><a href="./README.md">简体中文</a> · <a href="./README.en-US.md">English</a> · <a href="./README.ja-JP.md">日本語</a></p>

<p align="center">전적으로 GPT로 제작된, Windows 로컬 미디어 컬렉션을 위한 로컬 우선·원본 미디어 읽기 전용 브라우저입니다.</p>

M²Shelf는 내장·외장 드라이브 또는 NAS 매핑 폴더에 저장된 애니메이션, 영화 및 관련 리소스를 별도 색인으로 관리하며 포스터 보기, Bangumi 메타데이터, 태그, 즐겨찾기, 시청 기록 및 외부 플레이어 실행 기능을 제공합니다.

간단히 말해, 파일 탐색기에서 언어, 인코딩, 자막 그룹의 차이와 지나치게 긴 파일 이름 때문에 구분하기 어려운 애니메이션 컬렉션을 아래와 같이 읽기 쉬운 포스터 보기로 한 번에 바꿀 수 있습니다.

<img width="1445" height="1226" alt="M²Shelf 포스터 보기" src="https://github.com/user-attachments/assets/4e1a235c-ea75-4996-b7a7-5c890e0b0803" />

애니메이션 '작품' 상세 페이지에서도 원래 파일 이름을 그대로 확인할 수 있으며, 클릭 한 번으로 Windows 파일 탐색기에서 해당 위치를 열 수 있습니다.

<img width="1445" height="1226" alt="M²Shelf 애니메이션 작품 상세 페이지" src="https://github.com/user-attachments/assets/9bac84e4-af4e-4b2a-8c5f-ecd05b06b34b" />

애니메이션 '시리즈' 상세 페이지:

<img width="1445" height="1226" alt="M²Shelf 애니메이션 시리즈 상세 페이지" src="https://github.com/user-attachments/assets/9bc04a4c-4184-4dd3-82dc-094ab32049f6" />

**원본 미디어 파일을 이동, 삭제, 이름 변경 또는 수정하지 않으며 기존 폴더 구조를 다시 정리하도록 요구하지 않습니다.**

## 주요 기능

- 여러 미디어 라이브러리를 관리하고 원하는 깊이까지 폴더를 재귀적으로 스캔합니다.
- 모든 리소스, 개별 라이브러리 및 실제 폴더 계층을 포스터 또는 목록 형태로 탐색합니다.
- 로컬 이름, 파일 이름, Bangumi 다국어 제목 및 사용자 태그를 검색합니다.
- 작품, 시리즈 및 기타 리소스를 자동 분류하고 수동 분류를 유지합니다.
- 신뢰도가 높은 Bangumi 항목을 자동 매칭하며 수동 검색, 수정 및 표지 재시도를 지원합니다.
- 동영상과 함께 자막, 이미지, 오디오, 문서, 압축 파일 등의 관련 리소스를 표시합니다.
- 설정한 외부 플레이어로 재생하고 Windows 파일 탐색기에서 파일 위치를 엽니다.
- 태그, 한 단계로 구성된 이름 지정 즐겨찾기 폴더 및 편집 모드로 일괄 정리합니다.
- M²Shelf가 재생을 성공적으로 시작한 작품을 최신순으로 표시합니다.
- 简体中文, English, 日本語, 한국어를 지원합니다.
- 시스템 설정, 라이트 및 다크 테마를 지원합니다.
- 창 크기, 정렬 선택 및 각 탐색 영역의 세션 내 위치를 기억합니다.
- 앱 전용 표지 캐시 위치 사용자 지정.

## 로컬 우선 설계와 개인정보 보호

미디어 폴더는 항상 읽기 전용으로 취급됩니다. M²Shelf의 색인, 표시 이름, Bangumi 연결 정보, 태그, 즐겨찾기, 시청 기록 및 설정은 앱 전용 SQLite 데이터베이스에 저장되며 표지는 앱 캐시에 저장됩니다.

Bangumi 검색과 표지 다운로드에는 인터넷 연결이 필요합니다. 로컬 색인 탐색과 로컬 파일 열기는 Bangumi에 의존하지 않습니다. 미디어 서버나 클라우드 계정이 필요하지 않으며 미디어 파일을 원격 서비스에 업로드하지 않습니다.

## 다운로드

현재 버전: **M²Shelf 0.5.11** (Windows x64)

- [Portable 버전 다운로드](https://github.com/Undermori/M2Shelf/releases/download/v0.5.11/M2Shelf-Portable-0.5.11-x64.zip)
- [최신 Release 보기](https://github.com/Undermori/M2Shelf/releases/latest)
- [모든 Release 보기](https://github.com/Undermori/M2Shelf/releases)

Portable 버전 사용 방법:

1. ZIP 전체를 압축 해제하고 압축 파일 내부에서 직접 실행하지 마세요.
2. `M2Shelf.exe`를 두 번 클릭하세요.
3. 미디어 폴더를 추가하고 스캔하세요.
4. 필요한 경우 외부 플레이어 경로를 설정하세요.

Portable은 앱 자체를 설치할 필요가 없다는 뜻입니다. 데이터베이스, 설정 및 기본 표지 캐시는 Windows 앱 데이터 폴더에 저장됩니다. 현재 빌드는 코드 서명되지 않았으므로 Windows SmartScreen에서 ‘알 수 없는 게시자’ 경고가 표시될 수 있습니다. 화면을 표시하려면 Microsoft Edge WebView2 Runtime이 필요합니다.

## 현재 범위

M²Shelf는 현재 내장 플레이어, 온라인 스트리밍, 트랜스코딩, 미디어 서버, 계정 동기화, 자동 자막 또는 이어보기 진행률을 제공하지 않습니다. 미디어 파일을 자동으로 이동하거나 이름을 변경하지도 않습니다.

## 개발

기술 스택: Tauri 2, Rust, React 19, TypeScript, Vite 및 SQLite.

```powershell
npm install
npm run tauri dev
```

커밋 전 확인:

```powershell
npm run typecheck
npm run build
npm run validate
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

Windows용 공식 결과물은 `scripts/build_windows_release.ps1`로 빌드합니다.

## 프로젝트 문서

- [개발 규칙](./AGENTS.md)
- [제품 사양](./docs/PRODUCT_SPEC.md)
- [현재 구현](./docs/PROJECT_CONTEXT.md)
- [장기 결정 사항](./docs/DECISIONS.md)
- [빠른 프로젝트 안내](./PROJECT_DOCUMENTATION.md)

## 제작자

- [森下Undermori · Bilibili](https://space.bilibili.com/2903441)

## 작품 라이브러리와 시작 시 업데이트

기본 작품 라이브러리는 동영상이 있는 작품을 폴더 전체에서 펼쳐 표시하고 동일한 Bangumi 항목의 소스를 묶습니다. 시리즈는 폴더 정리를 담당하며 폴더 보기로 전환할 수 있습니다. FOLDER 모드의 CD1/CD2, Disc1/Disc2 등은 구조와 제목 근거가 일치할 때만 상위 작품의 분권으로 처리합니다. 다른 연결과 수동 분류는 독립 경계를 유지합니다. 작품 상세의 “작품의 기타 리소스”에서는 소속 하위 폴더의 동영상을 재귀적으로 펼쳐 표시하며 원래 경로, 재생, 정확한 파일 위치를 유지합니다.

Bangumi 연결, 변경, 해제 및 자동 표지 재시도는 전체 소스 묶음에 적용됩니다. 분류, 표시 이름, 숨기기, 태그 및 즐겨찾기는 대상 소스를 선택합니다. 숨긴 항목은 설정에서 검색하고 복원할 수 있습니다. 시작 시 업데이트는 새 데이터베이스에서 켜지고 저장된 설정이 없는 기존 데이터베이스에서는 꺼지며, 기존 선택은 유지됩니다. 처음에는 전체 폴더 열거가 필요할 수 있습니다. 라이브러리 관리에 결과와 마지막 성공 시간이 표시됩니다. 실패한 라이브러리의 읽지 못한 인덱스와 이전 기준은 유지됩니다. 과거 별칭 보완은 앱 실행당 서로 다른 항목을 최대 32개 시도하며 실패도 예산에 포함하고 다음 실행에서 순환하여 이어갑니다. 원본 미디어는 항상 읽기 전용입니다.

`npm run check`로 프런트엔드와 프로젝트를 검사합니다. 독립 Windows PR workflow는 Rust 포맷, 테스트 및 엄격한 Clippy도 검사합니다. [Windows 로컬 개발](docs/WINDOWS_DEVELOPMENT.md)을 참조하세요。

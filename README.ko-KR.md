# M²Shelf

**MORI MEDIA SHELF**

[简体中文](./README.md) · [English](./README.en-US.md) · [日本語](./README.ja-JP.md)

M²Shelf는 Windows용 로컬 미디어 관리 앱입니다. 애니메이션, 영화·드라마, 만화, 전자책, 동인지와 설정집을 지원합니다. 로컬·외장 드라이브나 매핑된 NAS 폴더를 추가해 탐색, 검색, 재생, 독서를 할 수 있습니다.

## 기능

- 여러 라이브러리의 재귀·증분 스캔. 폴더 또는 개별 파일 인식, 책의 스마트 혼합 인식과 시리즈·독립 책·분류 정리. 폴더를 끌어다 놓아 추가하거나 옮긴 라이브러리의 경로를 변경할 수 있습니다.
- 포스터·목록 보기, 라이브러리 통합 탐색, 미디어 종류·태그 필터와 정렬. 이름, 파일명, Bangumi 다국어 제목과 메모를 검색하고 라이브러리, 미디어 종류, 연결 여부, 시청·독서 상태, 파일 형식으로 좁힐 수 있습니다.
- 종류별 Bangumi 자동·수동 연결. 실사 영화·드라마는 TMDb 기본 서비스 또는 개인 토큰을 지원하며 이중 언어 제목, 연도, 시즌 정보, 일반적인 배포 파일명을 분석하고 원어 포스터를 우선 사용. 연결·표시 이름·표지 수동 수정.
- 사용자 태그, 이름 있는 즐겨찾기, 시청·독서 상태, 로컬 메모와 일괄 편집. 항목 숨기기·복원, 전체 리소스에서 라이브러리 제외, 연결·표지 문제 목록.
- 시스템 기본 또는 지정 플레이어로 재생. 애니메이션의 본편·특전·기타 영상 구분과 회차별 진행 기록. mpv를 설정하면 회차 변경과 재생 위치가 동기화됩니다. 첨부 파일과 Windows 탐색기에서 파일 위치도 확인할 수 있습니다.
- 만화·텍스트 리더, 읽기 진행률, 책갈피와 다음 권 이어 읽기. 최근 연 항목은 최대 30개까지 표시하며 기록을 지울 수 있습니다.
- 로컬 표지 캐시와 백그라운드 썸네일 생성. 설정에서 진행률 확인·재시도. 표지가 없는 PDF에는 첫 페이지를 사용하고 로컬 이미지, 사용 가능한 온라인 후보 또는 책 페이지에서 표지를 선택할 수 있습니다.
- 밝게·어둡게·시스템 테마, 중국어·영어·일본어·한국어. 창 크기, 정렬, 탐색 위치 기억.

## 독서

이미지 폴더, CBZ, PDF, EPUB, TXT, MOBI, AZW3를 지원합니다. 이미지 형식은 JPG, JPEG, PNG, WebP, GIF, AVIF, BMP입니다.

- 만화: 한 페이지·두 페이지, 가로 넘김, 연속 스크롤, Webtoon, 읽기 방향, 확대, 맞춤, 배경색, 전체 화면.
- 텍스트: 페이지·스크롤, 목차, 글꼴·크기·굵기, 줄·문단 간격, 본문 너비, 여백, 색상.
- 만화·PDF 페이지 썸네일, 텍스트 책과 텍스트 레이어가 있는 PDF의 본문 검색.

MOBI/AZW3는 암호화되지 않은 MOBI6/KF8을 지원합니다. DRM, 일반 ZIP, RAR, 7Z는 내장 독서 대상이 아닙니다.

## 다운로드

**M²Shelf 0.5.15 · Windows x64**

- [설치 프로그램](https://github.com/Undermori/M2Shelf/releases/download/v0.5.15/M2Shelf-Setup-0.5.15-x64.exe)
- [Portable ZIP](https://github.com/Undermori/M2Shelf/releases/download/v0.5.15/M2Shelf-Portable-0.5.15-x64.zip)
- [최신 Release](https://github.com/Undermori/M2Shelf/releases/latest) · [모든 버전](https://github.com/Undermori/M2Shelf/releases)

Portable은 전체 압축을 푼 다음 `M2Shelf.exe`를 실행하세요. 데이터베이스·설정·기본 캐시는 Windows 앱 데이터에 저장됩니다. 이전 5개 파일 Portable도 새 ZIP 전체로 업데이트하세요.

설정에서 업데이트를 확인하거나 설치 프로그램 또는 Portable ZIP을 직접 다운로드하세요.

Microsoft Edge WebView2 Runtime이 필요합니다. SmartScreen 경고가 표시될 수 있습니다.

## 로컬 데이터

원본 미디어는 읽기 전용입니다. 색인, 표시 이름, 연결, 태그, 즐겨찾기, 기록, 설정은 앱의 SQLite에, 표지는 캐시에 저장합니다. 설정에서 컬렉션 데이터와 직접 지정한 표지를 백업하고 복원할 수 있습니다.

Bangumi/TMDb 검색과 표지 다운로드에는 인터넷이 필요하지만 로컬 탐색·독서에는 필요하지 않습니다. 미디어를 업로드하지 않고 서버나 클라우드 계정도 요구하지 않습니다. 영상은 외부 플레이어를 사용하며 온라인 스트리밍, 변환, 기기 동기화는 제공하지 않습니다.

## 개발·문서

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

`npm run check`와 [AGENTS.md](./AGENTS.md)의 Rust 검증을 실행합니다. Windows 빌드는 `scripts/build_windows_release.ps1`을 사용합니다.

- [Windows 개발·빌드](./docs/WINDOWS_DEVELOPMENT.md)
- [릴리스 절차](./docs/UPDATE_RELEASE_PROCESS.md)

## 작성자

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)

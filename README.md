# 클립 번역 (clip-translator)

Windows 10/11 트레이에서만 동작하는 클립보드 번역 프로그램입니다. 작업 표시줄에는 창을 두지 않고, 단축키를 누르면 클립보드에 복사해 둔 글을 DeepL 또는 Azure Translator로 번역해 활성 모니터 가운데에 띄웁니다.

화면은 Slint Fluent 스타일입니다. 앱의 메뉴·설정·오류 문구는 영어이고, 번역문 자체는 원문 언어 그대로 표시됩니다. 라이트 테마가 기본이고, Windows 앱 테마가 다크이면 다크 팔레트를 따릅니다.

## 하는 일

- 트레이 메뉴: 현재 단축키(선택 불가, `Hotkey`), `Settings`, `Open translation`, `Quit`
- 전역 단축키 기본값은 `Ctrl+Alt+T`입니다. 설정에서 칸을 클릭한 뒤 새 조합을 누르면 바뀝니다.
- 단축키를 누르면 클립보드 텍스트를 읽어 번역 창을 띄웁니다. 글 선택은 사용자가 직접 복사합니다.
- 언어 1(기본 한국어)이 번역 대상입니다. 원문이 언어 1이면 언어 2(기본 일본어)로 번역합니다. 창 안의 언어 목록으로 다른 언어에 다시 번역할 수 있습니다.
- DeepL API Developer(무료 키는 `:fx`로 끝남)와 Azure Translator F0를 씁니다. DeepL이 한도(HTTP 456)나 오류를 내면 Azure로 넘기는 옵션이 있습니다.
- 같은 Windows 사용자 세션에서는 한 개만 실행됩니다. 다시 실행하면 이미 떠 있는 쪽의 설정 창을 엽니다.

## 키 준비

API 키는 설정 창에만 입력합니다. 설정 파일에는 적히지 않고, 현재 Windows 사용자 자격으로 DPAPI(`CryptProtectData`) 암호화되어 `%APPDATA%\clip-translator\secrets.bin`에 저장됩니다. 다른 PC나 다른 계정으로 복사하면 풀 수 없습니다.

### DeepL

1. [DeepL API](https://www.deepl.com/pro-api)에서 계정을 만듭니다.
2. API Developer(무료) 키를 발급합니다. 무료 키는 `:fx`로 끝납니다.
3. 무료 한도는 일회 100만 자입니다. `:fx` 키는 `api-free.deepl.com`, 그 외 키는 `api.deepl.com`으로 호출합니다.
4. 설정에 키를 넣고 **Test connection**을 누르면 `/v2/usage`의 남은 글자 수를 보여 줍니다.

### Azure Translator

1. [Azure Portal](https://portal.azure.com)에서 Translator 리소스를 만듭니다.
2. 가격 책정 계층은 **F0**(무료, 월 200만 자)를 고릅니다.
3. 리소스의 키와 지역(예: `japaneast`)을 설정에 넣습니다. 기본 지역은 Japan East입니다.
4. 호출 주소는 `api.cognitive.microsofttranslator.com`의 Translator v3입니다. 지역은 `Ocp-Apim-Subscription-Region` 헤더로 보냅니다.
5. 이 키만으로는 계정 전체 사용량을 조회할 수 없습니다. 화면에 나오는 “Connected · characters left this month”는 이 PC에서 번역에 성공한 글자 수를 월마다 더한 값입니다. 달력의 달이 바뀌면 다시 200만 자부터 셉니다.

## 사용 방법

1. `clip-translator.exe`를 실행합니다. 콘솔 창과 작업 표시줄 단추는 없고, 알림 영역(트레이)에 파란 **가** 아이콘이 생깁니다.
2. 아이콘을 오른클릭해 **Settings**를 엽니다. 키, 기본 서비스, Azure 지역, Language 1·2, 단축키, “Fall back to Azure when DeepL is exhausted”, “Start automatically with Windows”를 고르고 **Save**합니다.
3. 번역할 글을 복사한 뒤 `Ctrl+Alt+T`(또는 저장한 단축키)를 누릅니다. 번역이 끝나기 전에는 창이 뜨지 않고, 결과가 준비되면 한 번만 열립니다.
4. 번역 창에서 **Copy**로 번역문을 클립보드에 넣거나, 언어 목록으로 다시 번역합니다. `Esc`로 닫습니다.
5. 키가 없거나, 한도를 넘겼거나, 클립보드가 비어 있거나, 네트워크가 실패하면 같은 창에 이유를 보여 줍니다.

설정 파일은 `%APPDATA%\clip-translator\config.toml`입니다. 오류 기록은 같은 폴더의 `app.log`입니다. 자동 실행은 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`의 `clip-translator` 값이며, 관리자 권한이 필요 없습니다. exe를 옮기면 다음 실행 때 등록 경로를 현재 exe로 다시 씁니다.

## 빌드

호스트는 Linux여도 됩니다. Rust 1.92 이상, `cargo-xwin`, Windows 리소스 컴파일러 `llvm-rc`가 필요합니다.

```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
# Ubuntu 예: sudo apt-get install llvm-18
# llvm-rc가 PATH에 없으면:
# sudo ln -sf /usr/lib/llvm-18/bin/llvm-rc /usr/local/bin/llvm-rc

cargo test
cargo xwin build --target x86_64-pc-windows-msvc --release -j 2
```

실행 파일은 여기에 나옵니다.

```text
target/x86_64-pc-windows-msvc/release/clip-translator.exe
```

추가 DLL 없이 exe 하나만 복사하면 됩니다. 코드 서명은 없습니다. SmartScreen이 막으면 “추가 정보 → 실행”으로 열 수 있습니다.

`cargo test`는 언어 선택, DeepL/Azure 요청·응답 파싱, 한도 초과 시 Azure 전환을 Linux에서 HTTP 목으로 검사합니다. 번역 로직은 플랫폼과 분리되어 있습니다.

## Windows에서 직접 확인할 것

이 저장소를 만든 환경에는 Windows 머신이 없어서, 아래는 exe를 만든 뒤 실제 PC에서 봐야 합니다.

- 트레이 아이콘과 메뉴(`Hotkey`, `Settings`, `Open translation`, `Quit`)가 뜨는지
- 작업 표시줄 단추와 콘솔 창이 없는지
- `Ctrl+Alt+T`가 다른 프로그램 위에서도 동작하고, 창이 커서가 있는 모니터 가운데에 포커스와 함께 뜨는지
- `Esc`로 번역 창이 닫히는지, `Copy`가 클립보드에 번역문을 넣는지
- 단축키를 누른 뒤 로딩 창이나 크기·위치 깜빡임 없이 결과 창이 한 번만 뜨는지
- 설정에서 단축키를 녹음해 저장하면 트레이 메뉴와 실제 단축키가 같이 바뀌는지
- DeepL `:fx` 키와 Azure 키·지역의 연결 테스트, 사용량 표시
- DeepL 한도(456) 또는 오류 때 Azure 자동 전환 문구
- 키 없음, 한도 초과, 빈 클립보드, 텍스트가 아닌 클립보드, 네트워크 오류 화면
- 라이트/다크 앱 테마
- 두 번째 실행이 기존 인스턴스의 설정만 여는지
- 자동 실행 체크 후 로그오프·로그인
- `%APPDATA%\clip-translator\config.toml`에 키가 없고 `secrets.bin`만 있는지
- 한글·일본어 글리프가 깨지지 않는지

## 라이선스

MIT. `LICENSE`를 참고하세요.

# Build, Distribuição & Checksums — Jarvis para Windows

Este documento reúne instruções de build local, detalhes do empacotamento **Tauri 2 + NSIS (x64)** e os hashes **SHA-256** dos binários publicados no GitHub Releases.

---

## 1. Instalação no Windows 10 / 11 (x64)

- **Instalador (`JARVIS_8.0.0_x64-setup.exe`)**: instala o aplicativo para o usuário atual (`installMode: currentUser`) sem exigir privilégios de administrador, registra o atalho no Menu Iniciar e adiciona desinstalador em *Configurações do Windows → Aplicativos → Aplicativos instalados*.
- **Executável portátil (`JARVIS.exe`)**: roda diretamente sem instalação.
- **Runtime WebView2**: usa o *Evergreen WebView2 Runtime* (já presente no Windows 11 e baixado via bootstrapper caso falte no Windows 10).
- **Assinatura Authenticode**: os binários desta release open-source não possuem certificado comercial Authenticode configurado; você pode conferir a integridade usando os hashes SHA-256 abaixo.

---

## 2. Como Compilar Localmente (Windows x64)

Pré-requisitos: Node.js 22+, Rust stable (`x86_64-pc-windows-msvc`), Visual Studio C++ Build Tools (x64) e Windows SDK.

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustc -vV
npm.cmd ci
npm.cmd test
npm.cmd run tauri -- build
```

O comando `tauri build` executa `npm run build` (`tsc --noEmit && vite build`), embute `dist/` no binário Rust e gera:
- `src-tauri/target/release/JARVIS.exe`
- `src-tauri/target/release/bundle/nsis/JARVIS_8.0.0_x64-setup.exe`

---

## 3. Verificação da Release Atual (`v8.0.0` — 2026-11-21)

### Artefatos e Checksums SHA-256 (unsigned)

| Artefato | Tamanho (bytes) | Tamanho (MiB) | SHA-256 |
| :--- | ---: | ---: | :--- |
| `JARVIS.exe` | 24.504.320 | 23,37 MiB | `4C3DC3F4BAA69529B9084C5CCDB2CDD7B61A59766916696394A8AB0F4639DC9D` |
| `JARVIS_8.0.0_x64-setup.exe` | 8.333.767 | 7,95 MiB | `E2FD652BFF8A47EADAB549D7007D5415D867B0F59A1E31D39C3489F6CF4E5B08` |

Para validar no PowerShell após baixar do GitHub Releases:

```powershell
Get-FileHash .\JARVIS_8.0.0_x64-setup.exe -Algorithm SHA256
```

### Validações Executadas nesta Build
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` — passou sem pendências.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` — **0 warnings, 0 errors**.
- `npm test` (`tsc --noEmit && vite build` + `cargo test`) — **34 passed; 0 failed**.
- **Smoke test real (`JARVIS.exe`)**: processo iniciou e permaneceu ativo (`SMOKE_TEST_ALIVE=True`), com `GET http://127.0.0.1:47821/api/status` respondendo `HTTP 503` (Companion desativado por padrão) e cabeçalhos `X-Frame-Options: DENY` e `Referrer-Policy: no-referrer`.

---

## 4. Evidência Histórica (Builds Anteriores)

- Em uma build inicial de referência (`v8.0.0` anterior), o instalador NSIS foi validado em instalação/desinstalação silenciosa com registro em *Aplicativos instalados*, e o teste E2E `node tests/desktop.mjs` capturou os screenshots de referência em [assistant-idle-desktop.png](assistant-idle-desktop.png) e [assistant-desktop.png](assistant-desktop.png).
- A instalação do WebView2 em uma máquina totalmente limpa sem WebView2 pré-instalado permanece dependente do bootstrapper online da Microsoft.

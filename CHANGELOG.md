# Changelog

Todas as mudanças notáveis do **Jarvis para Windows** estão documentadas neste arquivo.

---

## [8.0.0] — 2026-11-21

### Novos Recursos
- **Base de Conhecimento Local — RAG em Rust (`src-tauri/src/knowledge.rs`)**: indexação recursiva de pastas de código e documentos em blocos com numeração exata de linhas (`start_line..=end_line`), busca híbrida **BM25 + correspondência exata/nome de arquivo**, 4 novas ferramentas para a IA (`kb_index_path`, `kb_search`, `kb_list`, `kb_remove`) e nova aba **Base RAG** na gaveta lateral.
- **Auto-Updater Nativo 1-Clique (`src-tauri/src/updater.rs`)**: consulta `GitHub Releases`, compara versões SemVer, valida origem HTTPS em todos os redirecionamentos, verifica assinatura PE (`MZ`), calcula SHA-256 e baixa/inicia o instalador `.exe` direto em **Ajustes → Sistema** com progresso ao vivo.
- **Jarvis Companion Mobile + Tailscale 4G/5G & Túnel HTTPS (`src-tauri/src/companion.rs`, `companion_ui.html`)**: servidor HTTP/SSE embutido com PWA mobile redesenhado (Chat, Deck de Skills, Telemetria CPU/RAM em SVG, Tela do PC e Aprovação 2FA), suporte a **Wi-Fi Local**, detecção automática de **Tailscale VPN (`100.64.0.0/10`)** e **Túnel HTTPS** com cookie `Secure` automático e alertas/vibração para 2FA.
- **Terminal ao Vivo & Git Nativo (`terminal.rs`, `TerminalView.tsx`, `DiffView.tsx`)**: sessões PowerShell em streaming com cancelamento, barra de repositório Git e ferramentas `git_status`, `git_diff`, `git_log`, `git_commit` e `run_terminal`.
- **Skills (`/comando`) & Rotinas Proativas (`routines.rs`, `RoutinesView.tsx`)**: macros reutilizáveis (`/briefing`, `/saude-pc`, `/foco`, `/git-review`) e agendador em background (diário `HH:MM` ou por intervalo).

### Segurança e Estabilidade (Auditoria P0 / P1 / P2)
- **Companion endurecido**: inicia desligado (`503`) e restrito a `127.0.0.1`; pareamento de uso único trocado por cookie `HttpOnly; SameSite=Strict` (TTL de 8h); rotação automática do código após 5 tentativas inválidas (`429`); cabeçalhos `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff` e `Cache-Control: no-store`; timeouts de leitura (`5s`/`10s`) e limites de concorrência (`32` HTTP / `6` SSE).
- **Credenciais cifradas (Windows DPAPI)**: fallback seguro com `CryptProtectData` (`DPAPI1:`) em `config.rs`, `oauth.rs` e `google_oauth.rs` — nunca salva segredos em texto puro.
- **SSRF Guard & Filtros RAG**: bloqueio de IPs privados/loopback/link-local/CGNAT em ferramentas web e bloqueio de `.ssh`, `.aws`, `.gnupg`, `.env`, `credentials.json`, chaves privadas e pastas do Windows no indexador RAG.
- **Concorrência determinística**: cancelamento de chat por geração (`cancel_gen`), fila única de aprovações 2FA, `ScreenCaptureGuard` (Mutex + RAII) para capturas simultâneas, encerramento determinístico do microfone via `JoinHandle` e limpeza segura de listeners assíncronos no React.

### Qualidade de Código e Documentação
- **Zero warnings no Clippy (`-- -D warnings`)** enforced no CI (`.github/workflows/ci.yml`) e **34+ testes automatizados** via `npm test`.
- **Documentação reorganizada**: cabeçalhos `//!` em todos os 16 módulos Rust, JSDoc nos componentes React, guia completo para desenvolvedores ([CONTRIBUTING.md](CONTRIBUTING.md)), central de docs ([docs/README.md](docs/README.md)) e guia dedicado do Companion ([docs/COMPANION.md](docs/COMPANION.md)).

---

## [8.0.0-base] — 2026-11-20

- **Núcleo 3D (Three.js + GLSL)** e **Mascote Global Always-On-Top (`?window=mascot`)** com 4 temas visuais.
- **Voz Nativa em Rust (`mic.rs`, `tts.rs`)**: captura Win32 `waveIn` 16 kHz com VAD local, Wake Word `"Ei, Jarvis"`, transcrição ElevenLabs Scribe / Gemini e síntese ElevenLabs Flash v2.5 com cache SHA-256.
- **Visão de Jogos e Janelas (`tools.rs`)**: `PrintWindow(PW_RENDERFULLCONTENT)` + fallback DXGI Desktop Duplication + `WDA_EXCLUDEFROMCAPTURE`.
- **Navegador Embutido WebView2 (`browser.rs`)**, **Subagentes paralelos**, **Projetos + Diff com Undo em 1 clique** e **Paleta de Comandos (`Ctrl+K`)**.

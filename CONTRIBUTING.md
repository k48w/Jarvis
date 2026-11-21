# Guia do Código para Desenvolvedores — Jarvis

Este guia foi escrito para quem abriu o repositório no GitHub e quer **entender rapidamente onde cada coisa fica**, como os módulos conversam entre si e onde mexer para adicionar novas funcionalidades.

---

## 1. Visão Geral da Estrutura de Pastas

```text
Jarvis/
├── src/                         # Frontend desktop & prévia web (React 19 + TypeScript + Three.js)
│   ├── main.tsx                 # Cockpit principal, roteamento de abas, dock de voz/texto e listeners de eventos
│   ├── bridge.ts                # Ponte IPC única: todos os invoke() e listen() do Tauri + fallback web
│   ├── Drawers.tsx              # Gavetas laterais: Conversas, Projetos, Memória, Base RAG e Ajustes
│   ├── Stage.tsx                # Núcleo 3D interativo em WebGL2 (Three.js)
│   ├── AgentsView.tsx           # Aba "Agentes": painel Mission Control de subagentes em background
│   ├── BrowserView.tsx          # Aba "Navegador": WebView2 acoplado lado a lado com controle CDP
│   ├── DiffView.tsx             # Aba "Alterações": diff de código, undo em 1 clique e barra Git
│   ├── TerminalView.tsx         # Aba "Terminal": sessões PowerShell com streaming ao vivo
│   ├── RoutinesView.tsx         # Aba "Rotinas": Skills (/comandos) e agendador recorrente
│   ├── CommandPalette.tsx       # Paleta de comandos rápidos (Ctrl + K)
│   ├── MascotBuddy.tsx          # Janela flutuante do mascote (?window=mascot)
│   └── LandingShowcase.tsx      # Página oficial de apresentação e download (deploy no Vercel)
│
└── src-tauri/                   # Backend nativo do Windows (Rust + Tauri 2 + Win32)
    ├── Cargo.toml               # Dependências Rust e features da crate `windows`
    └── src/
        ├── main.rs              # Ponto de entrada, System Tray, atalho global e todos os #[tauri::command]
        ├── state.rs             # AppState concorrente (cancelamento por geração, fila 2FA, timers e agentes)
        ├── config.rs            # Preferências (config.json) e cofre de segredos (Keyring + Windows DPAPI)
        ├── llm.rs               # Cliente de streaming ChatGPT OAuth & Gemini + loop multi-turno de ferramentas
        ├── tools.rs             # Catálogo ToolSpec, permissões, proteção SSRF e automação Win32/DXGI
        ├── knowledge.rs         # Base de Conhecimento Local (RAG em Rust puro com chunking e busca BM25)
        ├── updater.rs           # Auto-Updater 1-clique integrado ao GitHub Releases (validação HTTPS + PE "MZ")
        ├── companion.rs         # Servidor HTTP/SSE mobile (LAN, Tailscale VPN e Túnel HTTPS)
        ├── companion_ui.html    # App mobile PWA single-file servido pelo companion.rs
        ├── routines.rs          # Persistência de Skills (/slug) e agendador de rotinas em background
        ├── terminal.rs          # Executor PowerShell assíncrono com streaming de stdout/stderr
        ├── browser.rs           # Controle da janela filha WebView2 e injeção DOM/CDP
        ├── mic.rs               # Captura de microfone Win32 waveIn (16 kHz) + VAD local + Wake Word
        ├── tts.rs               # Síntese de voz ElevenLabs com cache local SHA-256
        ├── oauth.rs             # Login OAuth 2.0 + PKCE da OpenAI (ChatGPT Plus/Pro)
        ├── google_oauth.rs      # Login OAuth 2.0 + PKCE do Google Cloud / Gemini
        └── storage.rs           # Persistência atômica de conversas, projetos e memórias em JSON
```

---

## 2. Receitas Rápidas ("Onde mexer para...")

### A. Como adicionar uma nova ferramenta para a IA (`tools.rs`)
Todas as ferramentas do ChatGPT e do Gemini são geradas automaticamente a partir de uma única tabela em `src-tauri/src/tools.rs`:
1. **`tool_catalog()` (`src-tauri/src/tools.rs`)**: adicione um novo `ToolSpec` com `name`, `title`, `description`, `risk` (`ToolRisk::Read` para leitura ou `ToolRisk::Action` para ações que exigem aprovação no modo Revisão) e o schema JSON em `parameters`.
2. **`summarize_tool_args()` (`src-tauri/src/tools.rs`)**: adicione uma linha no `match name` para exibir um resumo amigável no chip da ferramenta e no pedido de aprovação 2FA.
3. **`execute_tool()` (`src-tauri/src/tools.rs`)**: implemente o braço `match name` que executa a lógica em Rust e retorna `Result<String, String>`.

### B. Como criar um comando entre React e Rust (`main.rs` ↔ `bridge.ts`)
1. No Rust (`src-tauri/src/main.rs`), crie uma função anotada com `#[tauri::command]` e registre o nome dela dentro de `tauri::generate_handler![...]` no final do arquivo.
2. No TypeScript (`src/bridge.ts`), exporte uma função `async` que chama `invoke<TipoRetorno>("nome_do_comando", { ... })` quando `isTauri` for `true` e retorne um valor simulado quando estiver rodando no navegador web.

### C. Como funciona a Base de Conhecimento Local (RAG — `knowledge.rs`)
- Os arquivos e pastas indexados pelo usuário (na gaveta **Conversas → Base RAG** ou pela ferramenta `kb_index_path`) são divididos em blocos de ~36 linhas com sobreposição de 6 linhas, preservando `start_line` e `end_line`.
- Arquivos sensíveis (`.env`, `secrets.json`, `.pem`, `.key`) e pastas pesadas (`node_modules`, `target`, `.git`, `dist`) são ignorados automaticamente.
- A busca (`search_knowledge` / ferramenta `kb_search`) pontua cada bloco combinando **BM25** com bônus para frase exata e nome de arquivo.

### D. Como funciona o Companion Mobile + Tailscale / Túnel HTTPS (`companion.rs`)
- Por segurança, o servidor inicia **desativado** (`503`) e restrito a `127.0.0.1`.
- Ao ativar em **Ajustes → Celular**, você pode escolher entre 3 rotas de QR Code:
  1. **Wi-Fi Local (LAN)**: usa o IP da rede local (`192.168.x.x`).
  2. **Tailscale 4G/5G**: detecta automaticamente o IP da interface Tailscale (`100.64.0.0/10`) para conectar fora de casa sem abrir portas no roteador.
  3. **Túnel HTTPS**: aceita uma URL externa `https://...` (Tailscale Serve/Funnel ou Cloudflare Tunnel) e ativa automaticamente o atributo `Secure` no cookie de sessão `HttpOnly`.

---

## 3. Comandos de Verificação e Qualidade

Antes de abrir um Pull Request ou gerar um novo instalador `.exe`, execute:

```powershell
# 1. Verifica formatação do código Rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# 2. Roda o linter Clippy sem permitir warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings

# 3. Compila o frontend TypeScript/Vite e roda todos os testes unitários e de integração em Rust
npm test

# 4. Gera o executável JARVIS.exe e o instalador NSIS em src-tauri/target/release/
npm run tauri -- build
```

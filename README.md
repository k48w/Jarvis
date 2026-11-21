<div align="center">

<img src="docs/app-icon.jpg" width="76" alt="Jarvis" />

# Jarvis

Assistente desktop nativo para Windows construído em **Tauri 2, Rust (Win32 + Tokio) e React 19**.

[Baixar para Windows (.exe)](https://github.com/k48w/Jarvis/releases/latest) · [Site & Prévia Web](https://jarvis-cinematic.vercel.app/) · [Documentação](docs/README.md) · [Segurança](SECURITY.md) · [Guia do Código](CONTRIBUTING.md)

<img src="docs/assistant-desktop.png" width="100%" alt="Interface principal do Jarvis no Windows" />

</div>

---

## Principais Recursos

### Produtividade, Código, IA Offline & Conhecimento
- **IA 100% Local / Offline (Ollama & LM Studio)** — além de **ChatGPT (OAuth)** e **Google Gemini**, alterne para modelos locais (`qwen2.5`, `llama3.2`, `deepseek-r1`) com detecção automática de modelos em **Ajustes → IA**, custo zero e privacidade total.
- **Base de Conhecimento Local (RAG em Rust)** — indexa pastas de projetos, código-fonte, Markdown, **PDF**, **DOCX** e planilhas **XLSX / CSV** com sincronização automática de projetos, numeração exata de linhas (`start_line..=end_line`) e busca híbrida **BM25 + correspondência exata** (`kb_index_path`, `kb_search`, `kb_list`, `kb_remove` + aba **Base RAG**), bloqueando automaticamente `.env`, `.ssh`, chaves privadas e pastas de sistema.
- **Cliente Nativo MCP (Model Context Protocol)** — conecte servidores externos via **HTTP/Streamable HTTP** ou **Stdio local** (`npx`, `python`, `uvx`) em **Ajustes → MCP**; as ferramentas descobertas entram automaticamente no catálogo sob aprovação 2FA.
- **Histórico Inteligente de Clipboard (`Ctrl+C`)** — guarda os últimos 20 trechos copiados apenas em RAM (ignorando senhas, JWTs e chaves de API automaticamente) com ações 1-clique (**Explicar**, **Traduzir**, **Resumir**) na aba **Clipboard**.
- **Terminal ao vivo e Git nativo** — terminal PowerShell com saída em streaming e cancelamento em tempo real (`run_terminal`), barra Git integrada (`git_status`, `git_diff`, `git_log`, `git_commit`) e diff colorido com desfazer em 1 clique.
- **Agentes em paralelo, Skills (`/comando`) e rotinas agendadas** — até 3 subagentes em segundo plano, macros reutilizáveis (`/briefing`, `/saude-pc`, `/foco`, `/git-review`) e agendador em background.

### Voz, Visão & Co-Pilot de Tela
- **Voz nativa sem desperdício de créditos** — captura via Win32 `waveIn` (16 kHz) com detecção de silêncio local (VAD), Wake Word `"Ei, Jarvis"`, modo mãos-livres, transcrição (ElevenLabs Scribe / Gemini) e fala ElevenLabs com cache SHA-256 em disco.
- **Visão de janelas, jogos e Modo Co-Pilot de Tela** — lista janelas abertas e captura alvos específicos com `PrintWindow` e fallback **DXGI Desktop Duplication** (`ScreenCaptureGuard`). No **Modo Co-Pilot de Tela**, calcula a diferença visual localmente em Rust (`Δ pixel` — 0 créditos na tela parada) e envia dicas táticas curtas quando a cena ou logs mudam.
- **Navegador integrado (WebView2 + CDP)** — painel acoplado lado a lado quando você pede para abrir ou interagir com uma página; buscas comuns acontecem silenciosamente em segundo plano.
- **Atalhos e Mascote** — chamada instantânea por `Alt + Space`, paleta de comandos `Ctrl + K` e mascote flutuante sempre no topo.

### Celular Conectado & Atualizações
- **Jarvis Companion (Wi-Fi, Tailscale 4G/5G & Túnel HTTPS)** — PWA mobile servido diretamente em Rust com Chat, Deck de Skills, telemetria CPU/RAM, tela do PC e aprovação 2FA. Suporta **Wi-Fi Local**, detecção automática de **Tailscale VPN (`100.x.y.z`)** para uso no 4G/5G fora de casa e **Túnel HTTPS** (Tailscale Serve/Funnel ou Cloudflare Tunnel). Veja o guia em [docs/COMPANION.md](docs/COMPANION.md).
- **Auto-Updater 1-Clique via GitHub Releases** — verifica novas versões em **Ajustes → Sistema**, valida a origem HTTPS em cada redirecionamento, checa a assinatura PE (`MZ`) + SHA-256 e inicia o instalador `.exe` atualizado automaticamente.

### Segurança por Padrão
- **Segredos protegidos**: chaves e sessões OAuth usam o Gerenciador de Credenciais do Windows (`keyring`) com fallback cifrado por usuário via **Windows DPAPI (`DPAPI1:`)** — nunca texto puro.
- **Companion endurecido**: desativado (`503`) e restrito a `127.0.0.1` por padrão; pareamento de uso único trocado por cookie `HttpOnly; SameSite=Strict` (TTL 8h, `Secure` automático em HTTPS), rotação automática após 5 tentativas inválidas (`429`), cabeçalhos defensivos e limites contra Slowloris/flood.
- **SSRF Guard**: ferramentas web resolvem DNS e bloqueiam IPs privados, loopback, link-local, CGNAT e redirecionamentos internos.

---

## Instalação (Windows 10 / 11 x64)

1. Baixe a versão mais recente em [GitHub Releases](https://github.com/k48w/Jarvis/releases/latest):
   - **Instalador (~7,9 MB):** [JARVIS_8.0.0_x64-setup.exe](https://github.com/k48w/Jarvis/releases/download/v8.0.0/JARVIS_8.0.0_x64-setup.exe)
   - **Executável portátil:** [JARVIS.exe](https://github.com/k48w/Jarvis/releases/download/v8.0.0/JARVIS.exe)
2. Abra **Ajustes → IA** e conecte sua conta **ChatGPT (OAuth 2.0 + PKCE)** ou **Google Gemini (API Key / OAuth)**.
3. *(Opcional)* Em **Ajustes → Voz**, configure sua chave **ElevenLabs** e escolha o microfone.
4. Para futuras atualizações, use **Ajustes → Sistema → Verificar Atualização**.

> Projeto open-source sob licença MIT. As contas e chaves de IA/voz são configuradas pelo próprio usuário e não acompanham o binário.

---

## Modos de Permissão

| Modo | Comportamento |
| :--- | :--- |
| **Somente Conversa (`safe`)** | Desativa ações locais no PC, leitura de hardware e escrita em disco. |
| **Revisão Automática (`review`)** | Permite leitura e buscas; pede aprovação 2FA (desktop ou celular) antes de rodar comandos ou alterar arquivos. |
| **Permissão Total (`auto`)** | Executa ferramentas automaticamente, mantendo bloqueios contra pastas críticas do Windows e comandos destrutivos. |

---

## Atalhos de Teclado

| Atalho | Ação |
| :--- | :--- |
| `Alt + Space` | Mostrar ou ocultar o Jarvis sobre qualquer app ou jogo (configurável) |
| `Ctrl + K` | Abrir Paleta de Comandos Rápidos |
| `F11` / `Alt + Enter` | Alternar modo tela cheia / imersivo |
| `Ctrl + V` | Colar print como anexo na conversa |
| `Esc` | Fechar painéis ou sair do modo tela cheia |

---

## Desenvolvimento Local & Qualidade

Requisitos: **Windows 10/11 x64**, **Node.js 22+**, **Rust stable (MSVC)**, Visual Studio C++ Build Tools e WebView2 Runtime.

```powershell
git clone https://github.com/k48w/Jarvis.git
cd Jarvis
npm ci
npm run tauri -- dev
```

Para rodar formatação, linter estrito (zero warnings), todos os testes e gerar os binários de produção:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm test
npm run tauri -- build
```

---

## Documentação Organizada

| Guia | Conteúdo |
| :--- | :--- |
| [Central de Docs](docs/README.md) | Índice completo de toda a documentação do repositório |
| [Guia do Código](CONTRIBUTING.md) | Mapa de todos os arquivos (`src/` e `src-tauri/src/`) e receitas *"Onde mexer para..."* |
| [Arquitetura](docs/ARCHITECTURE.md) | Diagramas Mermaid, pipeline Win32/DXGI, áudio `waveIn`, RAG e Auto-Updater |
| [Jarvis Companion](docs/COMPANION.md) | Pareamento via Wi-Fi Local, Tailscale 4G/5G e Túnel HTTPS |
| [Segurança](SECURITY.md) | Política de segurança, DPAPI, SSRF, proteção do RAG e do Updater |
| [Build & Checksums](docs/native.md) | Empacotamento NSIS, WebView2 e hashes SHA-256 dos binários |
| [Changelog](CHANGELOG.md) | Histórico de versões e auditorias |

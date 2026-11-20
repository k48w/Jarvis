# J.A.R.V.I.S. — The Singularity Edition

![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-Native%20Win32-DEA584?logo=rust&logoColor=black)
![React 19](https://img.shields.io/badge/React-19%20%2B%20Three.js-61DAFB?logo=react&logoColor=black)
![Platform](https://img.shields.io/badge/Platform-Windows%20x64-0078D6?logo=windows&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-green)

Assistente pessoal cinematográfico de desktop para **Windows**, construído em **Tauri 2 + Rust nativo + React 19 + Three.js**, com núcleo holográfico de plasma 3D reativo à voz, mascote animado global na área de trabalho, Wake Word nativo (`"Ei, Jarvis"`), visão inteligente de jogos/janelas, navegador embutido 60fps controlado por IA, subagentes paralelos, projetos estilo Codex e painel de Diff com desfazer em 1 clique.

![Prévia do J.A.R.V.I.S. Desktop](docs/assistant-desktop.png)

---

## Recursos Principais

### 🌌 Núcleo Holográfico 3D & Temas
- **Esfera de Plasma Three.js + GLSL** com paralaxe suave do mouse, 3 anéis giroscópicos, retícula orbital HUD e espectro de voz 60fps.
- **4 temas visuais ao vivo**: *Stark Cyan*, *Arc Gold*, *Quantum Violet* e *Crimson Core*.
- **Tela cheia real (`F11` / `Alt+Enter`)** e **Modo Imersivo**.

### 🤖 Mascote Vivo por Todo o PC
- Janela flutuante **transparente e sempre no topo** arrastável para qualquer ponto da tela (funciona sobre outros apps e jogos).
- **Balões de fala ao vivo** com o resumo das respostas, avisos de agentes concluídos e timers, além de animações de reação (aceno, comemoração, alerta, cochilo ocioso).
- **Menu rápido no botão direito**: *Ver tela agora*, *Wake Word ON/OFF*, *Silenciar*, *Trocar cor do núcleo*, *Abrir J.A.R.V.I.S.* e *Ocultar*.

### 🎙️ Voz Nativa em Rust, `"Ei, Jarvis"` & Modo Mãos-Livres
- **Captura nativa Win32 (`waveIn` 16 kHz PCM)**: acessa seu microfone diretamente pelo Windows sem depender de permissões do WebView2 — funciona inclusive minimizado na bandeja ou atrás de jogos.
- **Wake Word `"Ei, Jarvis"`** com VAD local (**zero envio/custo durante o silêncio**) + botão de ativação rápida no topo com indicador ao vivo de captação.
- **Modo Mãos-Livres**: conversa contínua automática após cada resposta (diga *"parar"* ou *"tchau"* para encerrar).
- **STT (Fala → Texto)**: **ElevenLabs Scribe** (`scribe_v2` / `scribe_v1`) ou **Gemini Audio**, com seletor de microfone e teste ao vivo (barra VU) em **Ajustes → Voz**.
- **TTS (Texto → Fala)**: **ElevenLabs** (`eleven_flash_v2_5`, `eleven_turbo_v2_5`, `eleven_multilingual_v2`) com **cache local em disco** e modos de economia de créditos (*Econômico*, *Só no Microfone* e *Completo*).

### 👁️ Visão Inteligente de Tela, Jogos e Janelas (100% Rust Win32)
- Descobre automaticamente as janelas abertas (`list_open_windows`) e captura exatamente o app ou jogo pedido (`see_screen(target_window)`), mesmo atrás do navegador ou minimizado.
- Usa **`PrintWindow(..., PW_RENDERFULLCONTENT)`** em segundo plano (sem roubar foco), com fallback automático para **DXGI Desktop Duplication / GDI** em jogos DirectX/Vulkan/OpenGL, mantendo o próprio J.A.R.V.I.S. invisível na captura (`WDA_EXCLUDEFROMCAPTURE`).
- Controle nativo de mouse e teclado via Win32 `SendInput` (`screen_click`, `screen_type`, `screen_hotkey`), sem spawnar PowerShell.

### 🌐 Navegador Embutido 60fps & Subagentes Contextuais
- **Dock contextual inteligente**: as abas **Agentes**, **Navegador** e **Código (Diff)** ficam ocultas quando ociosas e aparecem automaticamente quando o J.A.R.V.I.S. entra em ação.
- **Navegador dividido estilo Codex**: WebView2 nativo 60fps controlado via CDP (`browser_navigate`, `browser_inspect`, `browser_click`, `browser_type`, `browser_scroll`, `browser_press_key`, `browser_back`, `browser_close`).
- **Subagentes paralelos (`spawn_agent`)**: executa pesquisas e tarefas em segundo plano com acompanhamento ao vivo.

### 💻 Projetos, Arquivos, Diff com Desfazer & `Ctrl+C`
- **Conversas & Projetos estilo Codex**: vincule pastas reais do seu PC com seletor nativo do Windows (`IFileOpenDialog`), organize conversas por projeto e dê contexto automático dos arquivos ao J.A.R.V.I.S.
- **Edição real de arquivos** (`list_files`, `read_file`, `write_file`, `edit_file`, `search_files`) com proteção contra pastas críticas do Windows.
- **Painel de Código / Diff**: mostra cada alteração linha a linha em verde/vermelho com botão **Desfazer alteração** em 1 clique.
- **Anexos & Área de Transferência (`Ctrl+C`)**: arraste e solte imagens, códigos ou PDFs na janela, cole prints com `Ctrl+V` ou peça *"Jarvis, analisa o que eu copiei"* (leitura nativa Win32 de texto `CF_UNICODETEXT` e prints `CF_DIB`).

### 🧠 Provedores de IA & Privacidade
- **ChatGPT Plus / Pro (OAuth 2.0 + PKCE)** — login oficial sem precisar de chave separada.
- **Google Gemini** — via API Key gratuita do Google AI Studio ou **Google OAuth 2.0 + PKCE**.
- **Chaves seguras**: todas as credenciais são salvas exclusivamente no **Gerenciador de Credenciais do Windows** e dados locais em `%APPDATA%\studio.jarvis.cinematic\`.

---

## Atalhos Úteis

| Atalho | Ação |
| --- | --- |
| `Alt + Space` | Mostrar / ocultar J.A.R.V.I.S. de qualquer lugar (configurável) |
| `F11` ou `Alt + Enter` | Alternar tela cheia |
| `Enter` / `Shift + Enter` | Enviar mensagem / quebrar linha |
| `Ctrl + V` | Colar print diretamente como anexo visual |
| `Esc` | Fechar painéis ou sair do modo imersivo |

---

## Como Compilar Localmente (Windows x64)

### Pré-requisitos
- **Windows 10/11 x64** com WebView2 Runtime
- **Node.js 20+** e `npm`
- **Rust stable (MSVC)** + Visual Studio C++ Build Tools / Windows SDK

### Comandos

```powershell
# 1. Instalar dependências
npm ci

# 2. Rodar todos os testes unitários em Rust
cargo test --manifest-path src-tauri/Cargo.toml

# 3. Compilar frontend (prévia web / Vercel)
npm run build

# 4. Compilar executável desktop e instalador NSIS
npm run tauri -- build
```

Os binários gerados ficam em:
- Executável portátil: `src-tauri/target/release/JARVIS.exe`
- Instalador Windows: `src-tauri/target/release/bundle/nsis/JARVIS_8.0.0_x64-setup.exe`

### Testes Automatizados E2E

```powershell
node tests/scene.mjs
node tests/desktop.mjs
```

---

## Publicação Automática (GitHub Releases & Vercel)

- **GitHub Actions (`.github/workflows/release.yml`)**: roda `cargo test` + `npm run build` em todo push/PR; ao criar uma tag `v*` (ex.: `git tag v8.0.0 && git push --tags`), compila e publica automaticamente o `JARVIS.exe` e o instalador `JARVIS_8.0.0_x64-setup.exe` na aba **Releases**.
- **Vercel (`vercel.json`)**: basta importar o repositório na Vercel para publicar a prévia web interativa do núcleo 3D automaticamente.

---

## Licença

Distribuído sob a licença **MIT**. Veja [LICENSE](LICENSE) para mais detalhes.

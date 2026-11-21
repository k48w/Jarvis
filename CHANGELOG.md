# Changelog

Todas as mudanças notáveis do **J.A.R.V.I.S. — The Singularity Edition** estão documentadas neste arquivo.

## [8.0.0] — 2026-11-20

### Adicionado
- **Núcleo Holográfico 3D (Three.js + GLSL)**: esfera de plasma reativa à voz, 3 anéis giroscópicos, retícula orbital SVG, espectro de voz 60fps e 4 temas visuais ao vivo (*Stark Cyan*, *Arc Gold*, *Quantum Violet*, *Crimson Core*).
- **Mascote Global Always-On-Top (`?window=mascot`)**: janela transparente arrastável por todo o Windows com sprite sheet 6×10 a 60fps, balões de fala ao vivo, reações automáticas e menu rápido no botão direito.
- **Voz Nativa em Rust (`src-tauri/src/mic.rs`)**: captura direta pelo Windows Win32 (`waveIn` 16 kHz PCM), Wake Word `"Ei, Jarvis"` com VAD local (0% rede no silêncio), Modo Mãos-Livres contínuo, transcrição via **ElevenLabs Scribe** (`scribe_v2` / `scribe_v1`) ou **Gemini Audio**, e fala via **ElevenLabs Flash v2.5** com cache SHA-256 em disco.
- **Visão Inteligente de Jogos e Janelas (`src-tauri/src/tools.rs`)**: descoberta de janelas abertas (`list_open_windows`) e captura direcionada (`see_screen(target_window)`) via `PrintWindow(PW_RENDERFULLCONTENT)` sem roubar foco, fallback automático **DXGI Desktop Duplication / GDI** para jogos DirectX/Vulkan e exclusão da própria janela via `WDA_EXCLUDEFROMCAPTURE`.
- **Navegador Embutido 60fps & Subagentes (`src-tauri/src/browser.rs`)**: tela dividida estilo Codex com Child WebView2 nativo controlado via Chrome DevTools Protocol (8 ferramentas autônomas) e aba dinâmica de subagentes paralelos (`spawn_agent`).
- **Projetos Estilo Codex, Diff Colorido & `Ctrl+C`**: vinculação de pastas reais via `IFileOpenDialog`, edição cirúrgica de arquivos com painel de Diff colorido e botão **Desfazer alteração** (`revert_file_change`), leitura nativa da área de transferência (`CF_UNICODETEXT` e `CF_DIB`), Drag & Drop de arquivos e exportação de conversas em Markdown (`.md`).
- **Paleta de Comandos Rápidos (`Ctrl+K`)** e **Site Oficial Interativo (`LandingShowcase.tsx`)** com simulador 3D integrado.

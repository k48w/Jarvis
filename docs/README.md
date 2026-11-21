# Documentação — Jarvis

Bem-vindo à central de documentação do **Jarvis** para Windows. Os guias abaixo estão organizados por objetivo para facilitar tanto o uso diário quanto a contribuição no código.

---

## Mapa da Documentação

| Documento | Para quem é | O que cobre |
| :--- | :--- | :--- |
| [README.md](../README.md) | Todos | Visão geral do projeto, instalação rápida no Windows, atalhos e recursos principais |
| [COMPANION.md](COMPANION.md) | Usuários / Mobile | Como parear o celular via Wi-Fi Local, **Tailscale VPN (4G/5G)** ou **Túnel HTTPS**, aprovação 2FA e troubleshooting |
| [SECURITY.md](../SECURITY.md) | Segurança / Auditoria | Modelo de ameaça, DPAPI/Keyring, proteção contra SSRF, brute-force no pareamento, filtros do RAG e validação do Auto-Updater |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Desenvolvedores | Diagramas Mermaid da arquitetura multi-janela (Tauri 2 + Rust + React), visão DXGI/PrintWindow, áudio `waveIn`, Companion, RAG e Updater |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Contribuidores | Mapa completo de arquivos em `src/` e `src-tauri/src/`, receitas (*"Onde mexer para..."*) e comandos de CI/Clippy |
| [native.md](native.md) | Build & Release | Detalhes do instalador NSIS x64, runtime WebView2, smoke tests e checksums SHA-256 dos binários publicados |
| [audio.md](audio.md) | Áudio / UI | Especificação técnica dos sinais sonoros sintetizados em Web Audio (`src/audio.ts`) |
| [CHANGELOG.md](../CHANGELOG.md) | Histórico | Registro cronológico de recursos adicionados, melhorias de segurança e correções |
| [direcao.md](direcao.md) | Arte / Visual | Estudo de direção de arte que originou o núcleo 3D e os estados visuais |

---

## Links Rápidos

- **Download do instalador Windows (`.exe`)**: [GitHub Releases](https://github.com/k48w/Jarvis/releases/latest)
- **Site oficial e prévia web**: [jarvis-cinematic.vercel.app](https://jarvis-cinematic.vercel.app/)
- **Código-fonte**: [github.com/k48w/Jarvis](https://github.com/k48w/Jarvis)

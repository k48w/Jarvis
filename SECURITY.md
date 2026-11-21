# Política de Segurança — Jarvis

Este documento detalha os controles de segurança implementados no **Jarvis** para Windows e o canal para reporte de vulnerabilidades.

---

## 1. Armazenamento de Segredos e Credenciais (`config.rs`, `oauth.rs`, `google_oauth.rs`)

- **Windows Credential Manager (`keyring`)**: chaves de API (Gemini, ElevenLabs, Google Client Secret) e tokens OAuth 2.0 + PKCE (ChatGPT e Google) são gravados prioritariamente no cofre nativo do Windows (`studio.jarvis.cinematic`).
- **Fallback criptografado com Windows DPAPI (`DPAPI1:`)**: caso o Credential Manager esteja indisponível, os segredos são cifrados por usuário com `CryptProtectData` (`CRYPTPROTECT_UI_FORBIDDEN`) antes de tocar o disco. Nenhum segredo é salvo em texto puro.
- **Logs limpos**: tokens de acesso, refresh tokens e chaves nunca são impressos em logs ou respostas de erro.

---

## 2. Permissões de Janela e Modos de Autonomia (`tauri.conf.json`, `tools.rs`)

- **Princípio do menor privilégio (Tauri v2)**: a capability `main-window.json` concede apenas `core:default`, `core:event:default`, `core:window:default` e `core:window:allow-start-dragging` às janelas `main` e `mascot`. Toda interação com sistema de arquivos, processos e rede passa exclusivamente por comandos Rust validados.
- **Modos de Permissão**:
  - **Somente Conversa (`safe`)**: desativa ferramentas locais de sistema, leitura de hardware e escrita em disco.
  - **Revisão Automática (`review`)**: permite leitura e buscas, exigindo aprovação 2FA explícita (desktop ou celular) antes de executar comandos ou modificar arquivos.
  - **Permissão Total (`auto`)**: executa ferramentas automaticamente, mantendo bloqueios rígidos contra pastas críticas do Windows (`C:\Windows`, `System32`, `Program Files`) e comandos destrutivos de disco/boot.

---

## 3. Segurança do Jarvis Companion (`companion.rs`)

- **Desativado e local por padrão**: inicia com `enabled = false` e `lan_enabled = false` (bind em `127.0.0.1`). Enquanto desligado, todas as rotas retornam `503 Service Unavailable`.
- **Pareamento de uso único + anti força bruta**: o código de pareamento é invalidado no primeiro uso e **rotacionado automaticamente após 5 tentativas inválidas** (`429 Too Many Requests`).
- **Cookies de sessão endurecidos**: sessões usam cookie `HttpOnly; SameSite=Strict; Path=/; Max-Age=28800` (TTL de 8 horas, limite de 16 sessões), recebendo automaticamente `; Secure` quando acessadas via túnel HTTPS. Nenhum token é aceito via query string (`?token=`).
- **Cabeçalhos defensivos e limites HTTP**: todas as respostas incluem `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` e `Referrer-Policy: no-referrer`. Timeouts de leitura (`5s` headers / `10s` body) e semáforos (`32` conexões HTTP / `6` streams SSE) mitigam Slowloris e exaustão de recursos.

---

## 4. Proteção contra SSRF, RAG Seguro e Auto-Updater

- **SSRF Guard (`tools.rs`)**: `fetch_webpage`, `fetch_browser_page` e `open_url` resolvem o DNS e bloqueiam endereços de loopback (`127.0.0.0/8`, `::1`), privados RFC1918, link-local/metadata (`169.254.0.0/16`), CGNAT (`100.64.0.0/10`), multicast e redirecionamentos para redes internas.
- **Filtros de privacidade na Base RAG (`knowledge.rs`)**: a indexação bloqueia diretórios de sistema (`C:\Windows`, `Program Files`, `Microsoft\Protect`) e pastas/arquivos de credenciais (`.ssh`, `.aws`, `.gnupg`, `.azure`, `.kube`, `.env*`, `secrets.json`, `credentials.json`, `*oauth*.json`, `id_rsa*`, `id_ed25519*`, `.npmrc`, `.pem`, `.key`, `.pfx`, `.p12`, `.kdbx`).
- **Auto-Updater verificado (`updater.rs`)**: aceita exclusivamente downloads HTTPS de `github.com/k48w/Jarvis/releases/download/...` e CDNs oficiais do GitHub (`objects.githubusercontent.com` / `release-assets.githubusercontent.com`), validando **cada salto de redirecionamento**, verificando a assinatura binária PE (`MZ`) e calculando o hash **SHA-256** antes de executar o instalador.

---

## 5. Reportando Vulnerabilidades

Caso encontre qualquer vulnerabilidade de segurança, abra uma issue marcada com a label `security` ou entre em contato diretamente pelo GitHub [@k48w](https://github.com/k48w).

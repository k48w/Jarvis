# Jarvis Companion — Controle pelo Celular (Wi-Fi, Tailscale 4G/5G & Túnel HTTPS)

O **Jarvis Companion** é um servidor HTTP/SSE leve embutido diretamente no binário Rust (`src-tauri/src/companion.rs`) que serve uma interface PWA responsiva (`companion_ui.html`) para controlar o PC pelo celular sem precisar instalar aplicativos de terceiros na loja.

---

## 1. O que você pode fazer pelo celular

- **Chat sincronizado com o PC**: envie comandos por texto ou voz e acompanhe respostas em tempo real via SSE.
- **Aprovação de Segurança (2FA)**: quando o modo **Revisão Automática** estiver ativo, pedidos de ações sensíveis aparecem no celular com botões **Aprovar / Recusar**, vibração e notificação do navegador.
- **Deck de Skills (`/comandos`)**: dispare macros como `/briefing`, `/saude-pc`, `/foco` e `/git-review` com 1 toque.
- **Telemetria & Mídia**: monitore CPU/RAM em tempo real, ajuste volume, mute e controle reprodução de mídia.
- **Visão de Tela Remota**: veja a lista de janelas abertas e solicite uma captura da tela ou de uma janela específica (protegida por `ScreenCaptureGuard`).

---

## 2. Rotas de Conexão

Em **Ajustes → Celular** no Jarvis desktop, o servidor começa **desativado por padrão** (`HTTP 503`). Depois de ativá-lo, escolha a rota do QR Code:

### A. Wi-Fi Local (LAN)
1. Marque **Ativar Jarvis Companion** e **Expor na rede Wi-Fi (Modo LAN)**.
2. No primeiro uso, clique em **Liberar Firewall** para criar a regra TCP na porta `47821` (redes privadas).
3. Escaneie o QR Code no celular conectado ao mesmo Wi-Fi.

> **Aviso de segurança:** na rede local o tráfego usa HTTP sem TLS. Ative o Modo LAN apenas em redes Wi-Fi domésticas/privadas confiáveis.

### B. Tailscale VPN (Acesso pelo 4G/5G fora de casa)
1. Instale e conecte o **Tailscale** no seu PC Windows e no celular usando a mesma conta.
2. Ative o **Modo LAN** no Jarvis. O backend detecta automaticamente o IPv4 da interface Tailscale (`100.64.0.0/10`) sem precisar abrir portas no roteador.
3. Na aba **Celular**, selecione a rota **Tailscale 4G/5G (`100.x.y.z`)** e escaneie o QR Code.

### C. Túnel HTTPS (Tailscale Serve / Cloudflare Tunnel)
Ideal se você deseja **HTTPS ponta a ponta**, cookie `Secure` automático e suporte completo a microfone/notificações no navegador mobile:
1. Exponha a porta local `127.0.0.1:47821` via `tailscale serve https / http://127.0.0.1:47821` ou `cloudflared tunnel --url http://127.0.0.1:47821`.
2. Em **Ajustes → Celular**, selecione **Túnel HTTPS**, cole a URL `https://...` e clique em **Salvar URL**.
3. Escaneie o QR Code atualizado. O backend detecta HTTPS automaticamente e envia `Set-Cookie: ...; HttpOnly; SameSite=Strict; Secure`.

---

## 3. Segurança do Protocolo

- **Pareamento descartável**: o código de 8 caracteres exibido no QR Code (`?pair=`) é de uso único e rotacionado imediatamente após o pareamento bem-sucedido em `POST /api/pair`.
- **Proteção contra força bruta**: após **5 tentativas inválidas** em `POST /api/pair`, o servidor rotaciona o código automaticamente e responde `429 Too Many Requests`.
- **Sessão em cookie `HttpOnly`**: nenhum token trafega em query strings de API ou SSE nem fica em `localStorage`. Sessões expiram automaticamente em **8 horas** (`Max-Age=28800`), com limite máximo de **16 sessões ativas**.
- **Revogação instantânea**: clique em **Revogar Sessões / Novo Código** em **Ajustes → Celular** para encerrar imediatamente todas as sessões e streams SSE conectados.
- **Limites contra Slowloris / Flood**: timeouts de `5s` para cabeçalhos e `10s` para corpo, máximo de `32` conexões HTTP simultâneas e `6` clientes SSE (`429` acima do limite), além de cabeçalhos `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` e `Referrer-Policy: no-referrer`.

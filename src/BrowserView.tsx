/**
 * # Painel do Navegador Integrado WebView2 (`src/BrowserView.tsx`)
 *
 * Controla a janela WebView2 acoplada lado a lado com o chat, sincronizando
 * coordenadas (`browserSyncBounds`), inspeção DOM ao vivo e comandos assistidos.
 */
import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ExternalLink,
  Globe,
  Lock,
  Maximize2,
  Mic,
  RotateCw,
  ScanEye,
  Square,
  X,
} from "lucide-react";
import {
  browserBack,
  browserForward,
  browserInspectLive,
  browserNavigate,
  browserReload,
  browserSetVisible,
  browserStatus,
  browserSyncBounds,
  isTauri,
  onBrowserUpdated,
  openNativeBrowserWindow,
  type StoredMessage,
  type ToolTrace,
} from "./bridge";
import { MarkdownBlocks } from "./ResponseCard";
import type { CoreState } from "./rhythm";

const BOOKMARKS = [
  { label: "WhatsApp Web", url: "https://web.whatsapp.com/" },
  { label: "Google", url: "https://www.google.com/" },
  { label: "YouTube", url: "https://www.youtube.com/" },
  { label: "GitHub", url: "https://github.com/trending" },
  { label: "Mercado Livre", url: "https://www.mercadolivre.com.br/" },
  { label: "Wikipedia", url: "https://pt.wikipedia.org/wiki/Intelig%C3%AAncia_artificial" },
];

function normalizeAddress(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return isTauri ? "https://www.google.com/" : BOOKMARKS[5].url;
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  if (/^(localhost|127\.0\.0\.1)(:\d+)?(\/.*)?$/i.test(trimmed)) {
    return `http://${trimmed}`;
  }
  if (!trimmed.includes(" ") && trimmed.includes(".")) {
    return `https://${trimmed}`;
  }
  return isTauri
    ? `https://www.google.com/search?q=${encodeURIComponent(trimmed)}`
    : `https://html.duckduckgo.com/html/?q=${encodeURIComponent(trimmed)}`;
}

export function BrowserView({
  initialUrl,
  initialTitle,
  obscured,
  mode,
  statusText,
  messages,
  streamingText,
  liveTools,
  streaming,
  busy,
  draft,
  onDraftChange,
  onSend,
  onStop,
  onToggleMic,
  onBackToCore,
  onCloseBrowser,
}: {
  initialUrl?: string;
  initialTitle?: string;
  obscured: boolean;
  mode: CoreState;
  statusText: string;
  messages: StoredMessage[];
  streamingText: string;
  liveTools: ToolTrace[];
  streaming: boolean;
  busy: boolean;
  draft: string;
  onDraftChange: (v: string) => void;
  onSend: () => Promise<void>;
  onStop: () => Promise<void>;
  onToggleMic: () => Promise<void>;
  onBackToCore: () => void;
  onCloseBrowser: () => void;
}) {
  const defaultUrl =
    initialUrl && initialUrl.trim()
      ? normalizeAddress(initialUrl)
      : isTauri
        ? "https://www.google.com/"
        : BOOKMARKS[5].url;

  const [addressInput, setAddressInput] = useState(defaultUrl);
  const [pageTitle, setPageTitle] = useState(initialTitle || "Navegador J.A.R.V.I.S.");
  const [webFallbackUrl, setWebFallbackUrl] = useState(defaultUrl);
  const [webFallbackNonce, setWebFallbackNonce] = useState(0);
  const [ready, setReady] = useState(!isTauri);
  const [loading, setLoading] = useState(false);
  const [showOverlay, setShowOverlay] = useState(false);
  const [statusError, setStatusError] = useState<string | null>(null);

  const hostRef = useRef<HTMLDivElement | null>(null);
  const chatScrollRef = useRef<HTMLDivElement | null>(null);
  const addressFocusedRef = useRef(false);
  const overlayRef = useRef(showOverlay);
  const obscuredRef = useRef(obscured);

  overlayRef.current = showOverlay;
  obscuredRef.current = obscured;

  // Sync external URL updates emitted when J.A.R.V.I.S. opens a specific site
  useEffect(() => {
    if (!initialUrl) return;
    const norm = normalizeAddress(initialUrl);
    if (!addressFocusedRef.current) setAddressInput(norm);
    if (!isTauri) setWebFallbackUrl(norm);
  }, [initialUrl]);

  useEffect(() => {
    if (initialTitle) setPageTitle(initialTitle);
  }, [initialTitle]);

  // Auto-scroll left chat column smoothly on new messages / streaming tokens
  useEffect(() => {
    const el = chatScrollRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages.length, streamingText, liveTools.length]);

  async function syncHostBounds(visibleOverride?: boolean) {
    if (!isTauri) return;
    const vis = visibleOverride ?? !obscuredRef.current;
    if (!vis) {
      await browserSetVisible(false).catch(() => {});
      return;
    }
    const el = hostRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width < 60 || r.height < 60) return;

    try {
      const st = await browserSyncBounds(
        Math.round(r.left),
        Math.round(r.top),
        Math.round(r.width),
        Math.round(r.height),
        true,
        addressInput,
      );
      setReady(true);
      if (st.url && st.url !== "about:blank" && !addressFocusedRef.current) {
        setAddressInput(st.url);
      }
      if (st.title) setPageTitle(st.title);
      setStatusError(null);
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    }
  }

  // Mount & sync native 60fps WebView2 bounds with ResizeObserver
  useEffect(() => {
    if (!isTauri) return;
    let active = true;
    let raf = 0;

    const scheduleSync = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        if (active) void syncHostBounds();
      });
    };

    scheduleSync();
    const t1 = window.setTimeout(() => {
      if (active) void syncHostBounds();
    }, 160);
    const t2 = window.setTimeout(() => {
      if (active) void syncHostBounds();
    }, 420);

    const observer = new ResizeObserver(() => scheduleSync());
    if (hostRef.current) observer.observe(hostRef.current);
    window.addEventListener("resize", scheduleSync);

    let unlisten: (() => void) | undefined;
    void onBrowserUpdated((ev) => {
      if (!active) return;
      if (ev.url && !addressFocusedRef.current) setAddressInput(ev.url);
      if (ev.title) setPageTitle(ev.title);
      if (overlayRef.current) {
        void browserInspectLive(true).catch(() => {});
      }
    }).then((fn) => {
      if (!active) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      active = false;
      clearTimeout(t1);
      clearTimeout(t2);
      cancelAnimationFrame(raf);
      observer.disconnect();
      window.removeEventListener("resize", scheduleSync);
      unlisten?.();
      void browserSetVisible(false).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    void syncHostBounds(!obscured);
  }, [obscured]);

  useEffect(() => {
    if (!isTauri || obscured) return;
    const id = window.setInterval(() => {
      if (addressFocusedRef.current) return;
      void browserStatus()
        .then((st) => {
          if (st.url && st.url !== "about:blank" && !addressFocusedRef.current) {
            setAddressInput(st.url);
          }
          if (st.title) setPageTitle(st.title);
        })
        .catch(() => {});
    }, 1500);
    return () => clearInterval(id);
  }, [obscured]);

  async function handleNavigate(raw: string) {
    const nextUrl = normalizeAddress(raw);
    setAddressInput(nextUrl);
    setStatusError(null);

    if (!isTauri) {
      setWebFallbackUrl(nextUrl);
      return;
    }

    setLoading(true);
    try {
      const info = await browserNavigate(nextUrl);
      setAddressInput(info.url);
      setPageTitle(info.title);
      if (overlayRef.current) {
        await browserInspectLive(true).catch(() => {});
      }
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleBack() {
    if (!isTauri) return;
    setLoading(true);
    try {
      const st = await browserBack();
      if (st.url) setAddressInput(st.url);
      if (st.title) setPageTitle(st.title);
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleForward() {
    if (!isTauri) return;
    setLoading(true);
    try {
      const st = await browserForward();
      if (st.url) setAddressInput(st.url);
      if (st.title) setPageTitle(st.title);
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleReload() {
    if (!isTauri) {
      setWebFallbackNonce((n) => n + 1);
      return;
    }
    setLoading(true);
    try {
      const st = await browserReload();
      if (st.url) setAddressInput(st.url);
      if (st.title) setPageTitle(st.title);
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleToggleOverlay() {
    const next = !showOverlay;
    setShowOverlay(next);
    overlayRef.current = next;
    if (!isTauri) return;
    setLoading(true);
    try {
      const info = await browserInspectLive(next);
      if (info.url) setAddressInput(info.url);
      if (info.title) setPageTitle(info.title);
    } catch (e) {
      setStatusError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section
      className="split-browser-shell"
      aria-label="Conversa e Navegador Embutido"
    >
      {/* Left column: Codex-style live J.A.R.V.I.S. conversation */}
      <aside className="split-chat-col" aria-label="Conversa J.A.R.V.I.S.">
        <div className="split-chat-head">
          <div className="split-orb-status">
            <span className={`mini-orb state-${mode}`} />
            <div>
              <strong>J.A.R.V.I.S.</strong>
              <small>{statusText}</small>
            </div>
          </div>
          <button
            type="button"
            className="icon-tiny"
            title="Voltar ao Núcleo 3D (mantém navegador aberto)"
            aria-label="Expandir Núcleo"
            onClick={onBackToCore}
          >
            <Maximize2 />
          </button>
        </div>

        <div ref={chatScrollRef} className="split-chat-log">
          {messages.length === 0 && !streamingText && (
            <div className="split-chat-empty">
              <p>
                Converse normalmente com o J.A.R.V.I.S. enquanto ele navega, lê e age ao lado em
                tempo real.
              </p>
              <small>
                Ex.: &ldquo;Olha no WhatsApp Web a resposta do meu cliente&rdquo; ou &ldquo;Pesquise
                lofi no YouTube e abra o primeiro vídeo&rdquo;
              </small>
            </div>
          )}

          {messages.map((m) => (
            <div key={m.id} className={`split-msg ${m.role}`}>
              {m.tool_events.length > 0 && (
                <div className="split-msg-tools">
                  {m.tool_events.map((t) => (
                    <span key={t.id} className={`tool-chip ${t.status}`}>
                      {t.title}
                    </span>
                  ))}
                </div>
              )}
              <div className="split-msg-bubble">
                {m.role === "assistant" ? (
                  <MarkdownBlocks text={m.text} />
                ) : (
                  <p>{m.text}</p>
                )}
              </div>
            </div>
          ))}

          {(streaming || streamingText || liveTools.length > 0) && (
            <div className="split-msg assistant streaming">
              {liveTools.length > 0 && (
                <div className="split-msg-tools">
                  {liveTools.map((t) => (
                    <span key={t.id} className={`tool-chip ${t.status}`}>
                      {t.title}
                      {t.summary ? ` · ${t.summary.slice(0, 26)}` : ""}
                    </span>
                  ))}
                </div>
              )}
              <div className="split-msg-bubble">
                {streamingText ? (
                  <MarkdownBlocks text={streamingText} />
                ) : (
                  <p className="agent-thinking-text">Operando navegador e processando...</p>
                )}
              </div>
            </div>
          )}
        </div>

        <form
          className="split-chat-composer"
          onSubmit={(e) => {
            e.preventDefault();
            void onSend();
          }}
        >
          <textarea
            aria-label="Mensagem no navegador"
            value={draft}
            placeholder="Peça ao J.A.R.V.I.S. para ver, clicar, escrever ou responder..."
            rows={2}
            maxLength={4000}
            onChange={(e) => onDraftChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void onSend();
              }
            }}
          />
          <div className="form-actions">
            <button
              type="button"
              className="mic"
              aria-label="Falar pelo microfone"
              aria-pressed={mode === "listening"}
              disabled={streaming || mode === "thinking"}
              onClick={() => void onToggleMic()}
            >
              <Mic />
            </button>
            {busy ? (
              <button
                type="button"
                className="send stop"
                aria-label="Interromper"
                onClick={() => void onStop()}
              >
                <Square />
              </button>
            ) : (
              <button
                type="submit"
                className="send"
                aria-label="Enviar"
                disabled={!draft.trim()}
              >
                <ArrowUp />
              </button>
            )}
          </div>
        </form>
      </aside>

      {/* Right column: Full 60fps Embedded Chromium WebView2 Browser */}
      <div className="split-browser-col">
        <div className="split-browser-topbar">
          <div className="browser-nav-btns">
            <button
              type="button"
              className="br-icon-btn"
              aria-label="Voltar"
              title="Voltar"
              disabled={!isTauri || loading}
              onClick={() => void handleBack()}
            >
              <ArrowLeft size={14} />
            </button>
            <button
              type="button"
              className="br-icon-btn"
              aria-label="Avançar"
              title="Avançar"
              disabled={!isTauri || loading}
              onClick={() => void handleForward()}
            >
              <ArrowRight size={14} />
            </button>
            <button
              type="button"
              className="br-icon-btn"
              aria-label="Recarregar"
              title="Recarregar"
              disabled={loading}
              onClick={() => void handleReload()}
            >
              <RotateCw size={13} className={loading ? "spin" : ""} />
            </button>
          </div>

          <form
            className="browser-omnibox"
            onSubmit={(e) => {
              e.preventDefault();
              addressFocusedRef.current = false;
              void handleNavigate(addressInput);
            }}
          >
            <span className="br-omni-icon" aria-hidden="true">
              {addressInput.startsWith("https://") ? (
                <Lock size={12} />
              ) : (
                <Globe size={12} />
              )}
            </span>
            <input
              type="text"
              value={addressInput}
              placeholder="Pesquise ou digite um endereço..."
              aria-label="Barra de endereço"
              onFocus={() => {
                addressFocusedRef.current = true;
              }}
              onBlur={() => {
                addressFocusedRef.current = false;
              }}
              onChange={(e) => setAddressInput(e.target.value)}
            />
          </form>

          <div className="browser-mode-switch">
            {isTauri && (
              <button
                type="button"
                className={showOverlay ? "br-pill-btn active" : "br-pill-btn"}
                title="Mostrar números [1..N] sobre os elementos que a IA enxerga"
                onClick={() => void handleToggleOverlay()}
              >
                <ScanEye size={13} />
                <span>Raio-X</span>
              </button>
            )}
            <button
              type="button"
              className="br-icon-btn"
              title="Abrir no navegador externo"
              aria-label="Abrir no navegador externo"
              onClick={() => void openNativeBrowserWindow(addressInput, pageTitle)}
            >
              <ExternalLink size={14} />
            </button>
            <button
              type="button"
              className="br-icon-btn danger"
              title="Fechar navegador embutido"
              aria-label="Fechar navegador"
              onClick={onCloseBrowser}
            >
              <X size={14} />
            </button>
          </div>
        </div>

        <div className="browser-subbar">
          <div className="browser-bookmarks">
            {BOOKMARKS.map((bm) => (
              <button
                key={bm.label}
                type="button"
                className="br-bm-chip"
                onClick={() => void handleNavigate(bm.url)}
              >
                {bm.label}
              </button>
            ))}
          </div>
          <div className="br-page-title" title={pageTitle}>
            <Globe size={11} />
            <span>{pageTitle}</span>
          </div>
        </div>

        {loading && <div className="br-loading-bar" aria-hidden="true" />}

        {statusError && (
          <div className="module-error-banner" role="alert">
            {statusError}
          </div>
        )}

        <div className="browser-stage">
          {isTauri ? (
            <div
              ref={hostRef}
              className="browser-native-host"
              data-ready={ready ? "true" : "false"}
              aria-label="Navegador Chromium 60fps embutido"
            >
              {!ready && (
                <div className="screen-placeholder">
                  <Globe />
                  <strong>Carregando Chromium 60fps...</strong>
                </div>
              )}
            </div>
          ) : (
            <div className="browser-viewport">
              <iframe
                key={`${webFallbackUrl}-${webFallbackNonce}`}
                src={webFallbackUrl}
                title={pageTitle}
                className="browser-iframe"
                sandbox="allow-same-origin allow-scripts allow-forms allow-popups"
                referrerPolicy="no-referrer"
              />
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

/**
 * # Página Oficial & Vitrine Premium (`src/LandingShowcase.tsx`)
 *
 * Renderizada quando o projeto é aberto no navegador (`https://jarvis-cinematic.vercel.app`),
 * com design editorial escuro, núcleo 3D interativo em WebGL2, Bento Grid de recursos
 * (Base RAG PDF/DOCX/XLSX, IA Local Ollama/LM Studio, MCP, Co-Pilot de Tela, Companion
 * Wi-Fi/Tailscale/HTTPS e Auto-Updater), galeria real e download direto do `.exe`.
 */
import { useEffect, useRef, useState } from "react";
import {
  ArrowRight,
  Blocks,
  Check,
  ClipboardList,
  Copy,
  Cpu,
  Database,
  Download,
  Eye,
  Github,
  Play,
  ShieldCheck,
  Smartphone,
  Sparkles,
  Terminal,
} from "lucide-react";
import Stage, { type VisualTheme } from "./Stage";
import { HudOverlay, VoiceSpectrum } from "./HudOverlay";
import { AudioEngine } from "./audio";
import type { CoreState } from "./rhythm";
import "./landing.css";

const GITHUB_REPO_URL = "https://github.com/k48w/Jarvis";
const GITHUB_RELEASE_URL =
  "https://github.com/k48w/Jarvis/releases/download/v8.0.0/JARVIS_8.0.0_x64-setup.exe";
const GITHUB_RELEASES_PAGE = "https://github.com/k48w/Jarvis/releases/tag/v8.0.0";

const THEME_OPTIONS: Array<{ id: VisualTheme; label: string; color: string }> = [
  { id: "cyan", label: "Stark Cyan", color: "#6ce8ff" },
  { id: "gold", label: "Arc Gold", color: "#ffc95c" },
  { id: "violet", label: "Quantum Violet", color: "#b68cff" },
  { id: "crimson", label: "Crimson Core", color: "#ff5c72" },
];

const STATE_DEMOS: Array<{ id: CoreState; label: string; caption: string }> = [
  {
    id: "idle",
    label: "Pronto",
    caption: "VAD local em Rust ativo com 0 bytes de rede durante o silêncio.",
  },
  {
    id: "listening",
    label: "Ouvindo",
    caption: "Captura nativa Win32 waveIn 16 kHz aguardando o comando de voz.",
  },
  {
    id: "thinking",
    label: "Pensando",
    caption: "Consultando Base RAG BM25, ferramentas MCP e contexto do projeto.",
  },
  {
    id: "responding",
    label: "Falando",
    caption: "Streaming em tempo real com resposta falada via ElevenLabs Flash.",
  },
  {
    id: "alert",
    label: "Alerta 2FA",
    caption: "Aprovação de segurança sincronizada entre desktop e celular Companion.",
  },
];

const GALLERY_SLIDES = [
  {
    id: "cockpit",
    label: "Interface 3D",
    title: "Cockpit principal com núcleo WebGL2",
    subtitle: "Esfera reativa à voz, respostas em Markdown ao vivo e troca instantânea de temas.",
    img: "/og-cover.png",
  },
  {
    id: "browser",
    label: "Navegador WebView2",
    title: "Navegador integrado lado a lado",
    subtitle: "WebView2 nativo controlado via Chrome DevTools Protocol quando você pedir.",
    img: "/gallery-browser.png",
  },
  {
    id: "agents",
    label: "Agentes Paralelos",
    title: "Mission Control de subagentes em background",
    subtitle: "Até 3 agentes pesquisando, programando ou automatizando em paralelo com resumo falado.",
    img: "/gallery-agents.png",
  },
  {
    id: "awakening",
    label: "Tela Cheia (F11)",
    title: "Modo imersivo sem distrações",
    subtitle: "Experiência limpa em tela cheia com ativação por voz e atalho global Alt + Space.",
    img: "/gallery-awakening.png",
  },
];

export function LandingShowcase({
  theme,
  onThemeChange,
  onEnterSimulator,
}: {
  theme: VisualTheme;
  onThemeChange: (t: VisualTheme) => void;
  onEnterSimulator: () => void;
}) {
  const [demoState, setDemoState] = useState<CoreState>("responding");
  const [heroVisualMode, setHeroVisualMode] = useState<"studio" | "live3d">("studio");
  const [activeSlideId, setActiveSlideId] = useState<string>("cockpit");
  const [copiedCmd, setCopiedCmd] = useState(false);
  const demoAudioRef = useRef<number>(0.42);
  const sfxRef = useRef<AudioEngine | null>(null);

  useEffect(() => {
    return () => {
      sfxRef.current?.dispose();
    };
  }, []);

  useEffect(() => {
    let raf = 0;
    const loop = (now: number) => {
      if (demoState === "listening" || demoState === "responding") {
        demoAudioRef.current =
          0.32 +
          Math.abs(Math.sin(now * 0.0055)) * 0.45 +
          Math.abs(Math.cos(now * 0.013)) * 0.2;
      } else if (demoState === "thinking") {
        demoAudioRef.current = 0.22 + Math.abs(Math.sin(now * 0.004)) * 0.18;
      } else {
        demoAudioRef.current = 0.06;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [demoState]);

  function selectDemoState(next: CoreState) {
    setDemoState(next);
    setHeroVisualMode("live3d");
    try {
      sfxRef.current ??= new AudioEngine();
      sfxRef.current.setLevel(0.36, false);
      void sfxRef.current.cue(next, { suppressLoop: true }).catch(() => {});
    } catch {
      /* ignore autoplay block */
    }
  }

  async function handleCopyBuildCmd() {
    const cmd = `git clone https://github.com/k48w/Jarvis.git\ncd Jarvis\nnpm ci\nnpm test\nnpm run tauri -- build`;
    try {
      await navigator.clipboard.writeText(cmd);
      setCopiedCmd(true);
      setTimeout(() => setCopiedCmd(false), 1800);
    } catch {
      /* ignore */
    }
  }

  const currentDemo =
    STATE_DEMOS.find((item) => item.id === demoState) ?? STATE_DEMOS[0];
  const currentSlide =
    GALLERY_SLIDES.find((s) => s.id === activeSlideId) ?? GALLERY_SLIDES[0];

  return (
    <div className="lp2-root" data-theme={theme}>
      <div className="lp2-bg-grid" aria-hidden="true" />
      <div className="lp2-bg-aura" aria-hidden="true" />

      {/* FLOATING FROSTED NAVBAR */}
      <div className="lp2-nav-wrap">
        <header className="lp2-nav">
          <a href="#topo" className="lp2-brand">
            <img src="/app-icon.jpg" alt="Jarvis" />
            <span>Jarvis</span>
            <span className="lp2-brand-tag">Windows x64</span>
          </a>

          <nav className="lp2-nav-links" aria-label="Navegação principal">
            <a href="#recursos">Recursos</a>
            <a href="#galeria">Interface</a>
            <a href="#comparativo">Comparativo</a>
            <a href="#faq">Segurança &amp; FAQ</a>
          </nav>

          <div className="lp2-nav-actions">
            <div className="lp2-theme-dots" role="group" aria-label="Tema visual">
              {THEME_OPTIONS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  title={t.label}
                  aria-label={`Tema ${t.label}`}
                  className={theme === t.id ? "lp2-theme-dot active" : "lp2-theme-dot"}
                  onClick={() => onThemeChange(t.id)}
                >
                  <span style={{ background: t.color }} />
                </button>
              ))}
            </div>
            <a
              href={GITHUB_REPO_URL}
              target="_blank"
              rel="noreferrer"
              className="lp2-btn lp2-btn-ghost lp2-btn-sm"
              aria-label="Abrir repositório no GitHub"
            >
              <Github size={15} />
              <span>GitHub</span>
            </a>
            <a
              href={GITHUB_RELEASE_URL}
              target="_blank"
              rel="noreferrer"
              className="lp2-btn lp2-btn-primary lp2-btn-sm"
            >
              <Download size={14} />
              <span>Baixar .exe</span>
            </a>
          </div>
        </header>
      </div>

      <main id="topo" className="lp2-container">
        {/* HERO SECTION */}
        <section className="lp2-hero">
          <div>
            <div className="lp2-eyebrow-pill">
              <span className="lp2-eyebrow-dot" />
              <span>Rust + Tauri 2 · ChatGPT, Gemini &amp; Ollama Offline · MIT</span>
            </div>

            <h1>
              O assistente nativo do Windows para{" "}
              <span>programar, jogar e automatizar.</span>
            </h1>

            <p className="lp2-hero-lead">
              Fale <code>&ldquo;Ei, Jarvis&rdquo;</code>, consulte PDFs, planilhas e repositórios
              na <strong>Base RAG local</strong>, conecte servidores <strong>MCP</strong>, rode
              modelos <strong>100% offline</strong> com Ollama/LM Studio e aprove ações pelo celular
              via Wi-Fi ou <strong>Tailscale 4G/5G</strong>.
            </p>

            <div className="lp2-cta-row">
              <a
                href={GITHUB_RELEASE_URL}
                target="_blank"
                rel="noreferrer"
                className="lp2-btn lp2-btn-primary lp2-btn-lg"
              >
                <Download size={18} />
                <div>
                  <strong>Baixar para Windows x64</strong>
                  <small>Instalador NSIS .exe · ~8.3 MB</small>
                </div>
              </a>

              <button
                type="button"
                className="lp2-btn lp2-btn-secondary lp2-btn-lg"
                onClick={onEnterSimulator}
              >
                <Sparkles size={18} />
                <div>
                  <strong>Abrir prévia interativa</strong>
                  <small>Explorar a interface no navegador</small>
                </div>
                <ArrowRight size={16} />
              </button>
            </div>

            <div className="lp2-hero-footnote">
              <span>Windows 10 / 11 (x64)</span>
              <span>·</span>
              <span>Auto-Updater 1-clique integrado</span>
              <span>·</span>
              <a href={GITHUB_RELEASES_PAGE} target="_blank" rel="noreferrer">
                Ver checksums SHA-256 no GitHub Releases
              </a>
            </div>
          </div>

          {/* INTERACTIVE STAGE SHOWCASE */}
          <div className="lp2-stage-card">
            <div className="lp2-stage-head">
              <span className="lp2-stage-status">
                <i />
                {heroVisualMode === "live3d"
                  ? `Núcleo WebGL2 · ${currentDemo.label}`
                  : "Núcleo Interativo · Clique em um estado"}
              </span>
              <div className="lp2-seg-control" role="group" aria-label="Modo do núcleo">
                <button
                  type="button"
                  className={heroVisualMode === "studio" ? "active" : ""}
                  onClick={() => setHeroVisualMode("studio")}
                >
                  Estúdio
                </button>
                <button
                  type="button"
                  className={heroVisualMode === "live3d" ? "active" : ""}
                  onClick={() => setHeroVisualMode("live3d")}
                >
                  WebGL 3D
                </button>
              </div>
            </div>

            <div className="lp2-stage-canvas">
              {heroVisualMode === "live3d" ? (
                <>
                  <Stage
                    state={demoState}
                    quality="High"
                    reduced={false}
                    theme={theme}
                    audioLevel={demoAudioRef}
                  />
                  <HudOverlay state={demoState} immersive={false} />
                </>
              ) : (
                <img
                  src="/hero-orb.jpg"
                  alt="Núcleo 3D do Jarvis"
                  className="lp2-studio-orb"
                />
              )}

              <div className="lp2-stage-caption">
                <p>{currentDemo.caption}</p>
                <VoiceSpectrum
                  state={demoState}
                  active={demoState === "listening" || demoState === "responding"}
                  audioLevel={demoAudioRef}
                />
              </div>
            </div>

            <div className="lp2-stage-states" role="group" aria-label="Testar estados do núcleo">
              {STATE_DEMOS.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  className={demoState === s.id ? "lp2-state-pill active" : "lp2-state-pill"}
                  onClick={() => selectDemoState(s.id)}
                >
                  {s.label}
                </button>
              ))}
            </div>
          </div>
        </section>

        {/* ENGINEERING TELEMETRY STRIP */}
        <section className="lp2-metrics-strip" aria-label="Métricas técnicas">
          <div className="lp2-metric-card">
            <strong>~8.3 MB</strong>
            <span>Binário Tauri 2 + Rust nativo (até 25× menor que apps Electron)</span>
          </div>
          <div className="lp2-metric-card">
            <strong>100% Offline</strong>
            <span>Ollama / LM Studio + Base RAG BM25 rodando direto no seu PC</span>
          </div>
          <div className="lp2-metric-card">
            <strong>DPAPI + SSRF</strong>
            <span>Segredos cifrados no Windows, anti-bruteforce 5× e zero texto puro</span>
          </div>
          <div className="lp2-metric-card">
            <strong>38 Testes</strong>
            <span>Clippy estrito (0 warnings), testes de protocolo Companion e CI ativo</span>
          </div>
        </section>

        {/* BENTO FEATURE GRID */}
        <section id="recursos" className="lp2-section">
          <div className="lp2-sec-head">
            <span className="lp2-kicker">ARQUITETURA NATIVA</span>
            <h2>Tudo o que você faz no PC, unificado em um único núcleo em Rust.</h2>
            <p>
              Sem depender de abas lentas ou scripts frágeis. Cada recurso foi escrito em módulos
              dedicados para responder em milissegundos com privacidade real.
            </p>
          </div>

          <div className="lp2-bento">
            {/* Card 1: Local RAG */}
            <article className="lp2-bento-card lp2-span-7">
              <div>
                <div className="lp2-bento-icon">
                  <Database size={18} />
                </div>
                <h3>Base de Conhecimento Local (RAG BM25)</h3>
                <p>
                  Indexe pastas de código, Markdown, <strong>PDF</strong>, <strong>DOCX</strong> e
                  planilhas <strong>XLSX/CSV</strong> com sincronização automática de projetos.
                  Ignora <code>.env</code>, <code>.ssh</code> e chaves privadas automaticamente.
                </p>
              </div>
              <div className="lp2-mini-panel">
                <div className="lp2-rag-hit">
                  <code>src-tauri/src/knowledge.rs · L182–L218</code>
                  <span className="lp2-score-pill">Score 9.42</span>
                </div>
                <div className="lp2-rag-hit">
                  <code>relatorio-q4.pdf · Página 4</code>
                  <span className="lp2-score-pill">Score 8.15</span>
                </div>
                <div className="lp2-rag-hit">
                  <code>financeiro.xlsx · Planilha 1</code>
                  <span className="lp2-score-pill">Score 7.80</span>
                </div>
              </div>
            </article>

            {/* Card 2: Companion Mobile */}
            <article className="lp2-bento-card lp2-span-5">
              <div>
                <div className="lp2-bento-icon">
                  <Smartphone size={18} />
                </div>
                <h3>Companion no Celular (Wi-Fi &amp; 4G/5G)</h3>
                <p>
                  Controle o PC pelo celular via Wi-Fi Local, <strong>Tailscale VPN</strong> ou{" "}
                  <strong>Túnel HTTPS</strong> com pareamento único, cookie <code>HttpOnly</code> e
                  aprovação 2FA.
                </p>
              </div>
              <div className="lp2-phone-mock">
                <div className="lp2-phone-bar">
                  <span>Jarvis Companion · Tailscale</span>
                  <span className="lp2-phone-badge">
                    <i /> Conectado
                  </span>
                </div>
                <div className="lp2-2fa-box">
                  <strong>Aprovação 2FA Pendente</strong>
                  <span>git_commit(&quot;Release v8.0&quot;)</span>
                  <div className="lp2-2fa-btns">
                    <em className="ok">Aprovar</em>
                    <em className="no">Recusar</em>
                  </div>
                </div>
              </div>
            </article>

            {/* Card 3: Local AI Offline */}
            <article className="lp2-bento-card lp2-span-4">
              <div>
                <div className="lp2-bento-icon">
                  <Cpu size={18} />
                </div>
                <h3>IA Local Offline + Nuvem</h3>
                <p>
                  Alterne entre <strong>ChatGPT OAuth</strong>, <strong>Google Gemini</strong> e
                  modelos 100% locais com <strong>Ollama</strong> ou <strong>LM Studio</strong>{" "}
                  (Qwen, Llama, DeepSeek) com detecção em 1 clique.
                </p>
              </div>
              <div className="lp2-chip-row">
                <span className="lp2-chip accent">Ollama :11434</span>
                <span className="lp2-chip">LM Studio :1234</span>
                <span className="lp2-chip">ChatGPT OAuth</span>
                <span className="lp2-chip">Gemini</span>
              </div>
            </article>

            {/* Card 4: MCP Native Client */}
            <article className="lp2-bento-card lp2-span-4">
              <div>
                <div className="lp2-bento-icon">
                  <Blocks size={18} />
                </div>
                <h3>Cliente Nativo MCP</h3>
                <p>
                  Conecte servidores <strong>Model Context Protocol</strong> via{" "}
                  <code>Stdio</code> ou <code>HTTP</code>. Ferramentas externas entram no catálogo
                  da IA sob aprovação 2FA automática.
                </p>
              </div>
              <div className="lp2-chip-row">
                <span className="lp2-chip accent">JSON-RPC 2.0</span>
                <span className="lp2-chip">GitHub</span>
                <span className="lp2-chip">Notion</span>
                <span className="lp2-chip">Postgres</span>
              </div>
            </article>

            {/* Card 5: Screen Co-Pilot */}
            <article className="lp2-bento-card lp2-span-4">
              <div>
                <div className="lp2-bento-icon">
                  <Eye size={18} />
                </div>
                <h3>Co-Pilot de Tela &amp; DXGI</h3>
                <p>
                  Captura janelas específicas com <code>PrintWindow</code> + <code>DXGI</code> e
                  monitora mudanças visuais localmente em Rust — gastando 0 créditos quando a tela
                  não muda.
                </p>
              </div>
              <div className="lp2-chip-row">
                <span className="lp2-chip accent">Δ Pixel Local</span>
                <span className="lp2-chip">Jogos DirectX</span>
                <span className="lp2-chip">Debug ao vivo</span>
              </div>
            </article>

            {/* Card 6: Terminal, Git & Clipboard History */}
            <article className="lp2-bento-card lp2-span-6">
              <div>
                <div className="lp2-bento-icon">
                  <Terminal size={18} />
                </div>
                <h3>Terminal ao Vivo &amp; Git Nativo</h3>
                <p>
                  Execute comandos PowerShell em streaming direto no app, revise diffs linha a
                  linha com <strong>desfazer em 1 clique</strong> e faça commits sem sair da
                  conversa.
                </p>
              </div>
              <div className="lp2-chip-row">
                <span className="lp2-chip">git_status</span>
                <span className="lp2-chip">git_diff</span>
                <span className="lp2-chip">git_commit</span>
                <span className="lp2-chip accent">run_terminal</span>
              </div>
            </article>

            {/* Card 7: Smart Clipboard History & Auto-Updater */}
            <article className="lp2-bento-card lp2-span-6">
              <div>
                <div className="lp2-bento-icon">
                  <ClipboardList size={18} />
                </div>
                <h3>Histórico Inteligente de Clipboard &amp; Auto-Updater</h3>
                <p>
                  Guarda os últimos 20 trechos copiados apenas em RAM (ignorando senhas e tokens)
                  com botões <strong>Explicar / Traduzir / Resumir</strong>, além de atualização
                  1-clique verificada por SHA-256.
                </p>
              </div>
              <div className="lp2-chip-row">
                <span className="lp2-chip accent">Filtro Anti-Senha</span>
                <span className="lp2-chip">Apenas RAM</span>
                <span className="lp2-chip">Updater PE + SHA-256</span>
              </div>
            </article>
          </div>
        </section>

        {/* GALLERY SECTION */}
        <section id="galeria" className="lp2-section">
          <div className="lp2-sec-head">
            <span className="lp2-kicker">CAPTURAS REAIS DO DESKTOP</span>
            <h2>Construído para rodar leve sobre jogos, IDEs e múltiplos monitores.</h2>
          </div>

          <div className="lp2-gallery-card">
            <div className="lp2-gallery-tabs" role="tablist">
              {GALLERY_SLIDES.map((slide) => (
                <button
                  key={slide.id}
                  type="button"
                  role="tab"
                  aria-selected={activeSlideId === slide.id}
                  className={
                    activeSlideId === slide.id
                      ? "lp2-gallery-tab active"
                      : "lp2-gallery-tab"
                  }
                  onClick={() => setActiveSlideId(slide.id)}
                >
                  {slide.label}
                </button>
              ))}
            </div>

            <div className="lp2-gallery-frame">
              <img src={currentSlide.img} alt={currentSlide.title} />
              <div className="lp2-gallery-caption">
                <div>
                  <strong>{currentSlide.title}</strong>
                  <div>
                    <span>{currentSlide.subtitle}</span>
                  </div>
                </div>
                <button
                  type="button"
                  className="lp2-btn lp2-btn-secondary lp2-btn-sm"
                  onClick={onEnterSimulator}
                >
                  <Play size={13} /> Abrir prévia web
                </button>
              </div>
            </div>
          </div>
        </section>

        {/* COMPARISON TABLE */}
        <section id="comparativo" className="lp2-section">
          <div className="lp2-sec-head">
            <span className="lp2-kicker">COMPARATIVO TÉCNICO</span>
            <h2>Por que nativo em Rust faz diferença no dia a dia.</h2>
          </div>

          <div className="lp2-table-wrap">
            <table className="lp2-table">
              <thead>
                <tr>
                  <th>Recurso</th>
                  <th>Jarvis (Tauri 2 + Rust)</th>
                  <th>Assistentes Web / Electron</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td>Provedores de IA</td>
                  <td className="hl">
                    <strong>ChatGPT OAuth, Gemini e Ollama / LM Studio offline</strong>
                  </td>
                  <td>Preso a um único provedor na nuvem</td>
                </tr>
                <tr>
                  <td>Base de Conhecimento (RAG)</td>
                  <td className="hl">
                    <strong>BM25 local em Rust para código, PDF, DOCX e XLSX</strong> com auto-sync
                  </td>
                  <td>Upload manual a cada conversa</td>
                </tr>
                <tr>
                  <td>Integrações Externas</td>
                  <td className="hl">
                    <strong>Cliente nativo MCP (Stdio &amp; HTTP)</strong> com aprovação 2FA
                  </td>
                  <td>Sem acesso a ferramentas locais do PC</td>
                </tr>
                <tr>
                  <td>Visão de Tela &amp; Co-Pilot</td>
                  <td className="hl">
                    <strong>PrintWindow + DXGI + diff visual local</strong> (0 créditos na tela parada)
                  </td>
                  <td>Print manual sem observação inteligente</td>
                </tr>
                <tr>
                  <td>Celular ↔ PC (Companion)</td>
                  <td className="hl">
                    <strong>Wi-Fi Local, Tailscale 4G/5G e Túnel HTTPS</strong> com cookie HttpOnly e 2FA
                  </td>
                  <td>Sem controle remoto nativo do desktop</td>
                </tr>
                <tr>
                  <td>Segurança &amp; Atualização</td>
                  <td className="hl">
                    <strong>Windows DPAPI + SSRF Guard + Auto-Updater 1-clique</strong> (SHA-256)
                  </td>
                  <td>Tokens expostos ou atualização manual</td>
                </tr>
                <tr>
                  <td>Tamanho do Instalador</td>
                  <td className="hl">
                    <strong>~8.3 MB</strong> (instalador NSIS x64 sem admin)
                  </td>
                  <td>150 MB – 300 MB</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        {/* SHORTCUTS & FAQ */}
        <section id="faq" className="lp2-section">
          <div className="lp2-dual">
            <div className="lp2-panel">
              <span className="lp2-kicker">ATALHOS &amp; REQUISITOS</span>
              <h3>Controle Instantâneo</h3>
              <div className="lp2-sc-list">
                <div className="lp2-sc-row">
                  <kbd>Alt + Space</kbd>
                  <span>Chamar ou ocultar o Jarvis sobre qualquer app ou jogo</span>
                </div>
                <div className="lp2-sc-row">
                  <kbd>Ctrl + K</kbd>
                  <span>Abrir Paleta de Comandos Rápidos</span>
                </div>
                <div className="lp2-sc-row">
                  <kbd>F11</kbd>
                  <span>Alternar modo tela cheia imersivo</span>
                </div>
                <div className="lp2-sc-row">
                  <kbd>Ctrl + V</kbd>
                  <span>Colar print ou texto direto na conversa</span>
                </div>
                <div className="lp2-sc-row">
                  <kbd>&ldquo;Ei, Jarvis&rdquo;</kbd>
                  <span>Wake Word nativo com VAD local (zero envio no silêncio)</span>
                </div>
              </div>
              <p style={{ marginTop: "16px", fontSize: "12.5px", color: "var(--text-muted)" }}>
                <strong>Requisitos:</strong> Windows 10 ou 11 (x64), WebView2 Runtime e GPU
                compatível com WebGL2 / DirectX 11.
              </p>
            </div>

            <div className="lp2-panel">
              <span className="lp2-kicker">SEGURANÇA &amp; DÚVIDAS</span>
              <h3>Perguntas Frequentes</h3>

              <details className="lp2-faq-item" open>
                <summary>Posso usar o Jarvis 100% gratuito e offline sem API paga?</summary>
                <p>
                  Sim. Selecione <strong>IA Local Offline (Ollama / LM Studio)</strong> em{" "}
                  <strong>Ajustes → IA</strong> para rodar modelos como Qwen, Llama ou DeepSeek
                  direto na sua máquina, combinados com a Base RAG local. Você também pode usar
                  sua assinatura <strong>ChatGPT Plus/Pro (OAuth)</strong> ou{" "}
                  <strong>Google Gemini</strong>.
                </p>
              </details>

              <details className="lp2-faq-item">
                <summary>Como minhas chaves, arquivos e acesso pelo celular são protegidos?</summary>
                <p>
                  Chaves e sessões OAuth ficam no <strong>Gerenciador de Credenciais do Windows</strong>{" "}
                  com fallback criptografado via <strong>Windows DPAPI</strong> (nunca texto puro).
                  O Companion começa desligado por padrão (<code>503</code>), rotaciona o código
                  após 5 tentativas inválidas e envia cookie <code>HttpOnly; SameSite=Strict</code>{" "}
                  (com <code>Secure</code> em túneis HTTPS). A Base RAG bloqueia{" "}
                  <code>.env</code>, <code>.ssh</code> e pastas do Windows.
                </p>
              </details>

              <details className="lp2-faq-item">
                <summary>O Modo Co-Pilot de Tela e o Wake Word gastam créditos parados?</summary>
                <p>
                  Não. Tanto o VAD de microfone quanto o detector de mudança de tela (<code>Δ pixel</code>)
                  rodam localmente em Rust. O Jarvis só chama o modelo de IA quando detecta fala ou
                  mudança visual real acima do limiar escolhido.
                </p>
              </details>

              <details className="lp2-faq-item">
                <summary>Como recebo novas versões do aplicativo?</summary>
                <p>
                  Em <strong>Ajustes → Sistema</strong>, clique em{" "}
                  <strong>Verificar Atualização</strong>. O Auto-Updater valida exclusivamente
                  downloads HTTPS do GitHub Releases, checa a assinatura binária PE (<code>MZ</code>)
                  e exibe o hash SHA-256 antes de iniciar o instalador.
                </p>
              </details>
            </div>
          </div>
        </section>

        {/* FINAL DOWNLOAD & BUILD CTA */}
        <section id="download" className="lp2-section">
          <div className="lp2-download-card">
            <div>
              <span className="lp2-kicker">DOWNLOAD &amp; OPEN-SOURCE</span>
              <h2>Instale o Jarvis no seu Windows</h2>
              <p>
                Baixe o instalador x64 pronto para uso (~8 MB), verifique o hash SHA-256 ou compile
                diretamente do código-fonte modular em Rust.
              </p>
              <div className="lp2-cta-row">
                <a
                  href={GITHUB_RELEASE_URL}
                  target="_blank"
                  rel="noreferrer"
                  className="lp2-btn lp2-btn-primary lp2-btn-lg"
                >
                  <Download size={18} />
                  <div>
                    <strong>Baixar Instalador (.exe)</strong>
                    <small>GitHub Releases · Windows x64</small>
                  </div>
                </a>
                <a
                  href={GITHUB_REPO_URL}
                  target="_blank"
                  rel="noreferrer"
                  className="lp2-btn lp2-btn-secondary lp2-btn-lg"
                >
                  <ShieldCheck size={18} />
                  <div>
                    <strong>Código &amp; Documentação</strong>
                    <small>github.com/k48w/Jarvis</small>
                  </div>
                </a>
              </div>
            </div>

            <div className="lp2-code-box">
              <div className="lp2-code-head">
                <span>
                  <Terminal size={14} /> Build Local (PowerShell · Windows x64)
                </span>
                <button
                  type="button"
                  className="lp2-copy-btn"
                  onClick={() => void handleCopyBuildCmd()}
                >
                  {copiedCmd ? <Check size={13} /> : <Copy size={13} />}
                  {copiedCmd ? "Copiado" : "Copiar"}
                </button>
              </div>
              <pre>
                <code>{`git clone https://github.com/k48w/Jarvis.git
cd Jarvis
npm ci
npm test
npm run tauri -- build`}</code>
              </pre>
            </div>
          </div>
        </section>

        {/* FOOTER */}
        <footer className="lp2-footer">
          <div className="lp2-brand">
            <img src="/app-icon.jpg" alt="" />
            <span>Jarvis</span>
          </div>
          <div className="lp2-footer-links">
            <a href={GITHUB_REPO_URL} target="_blank" rel="noreferrer">
              GitHub
            </a>
            <a href={GITHUB_RELEASES_PAGE} target="_blank" rel="noreferrer">
              Releases &amp; SHA-256
            </a>
            <a href={`${GITHUB_REPO_URL}/blob/main/SECURITY.md`} target="_blank" rel="noreferrer">
              Segurança
            </a>
            <a href={`${GITHUB_REPO_URL}/blob/main/CONTRIBUTING.md`} target="_blank" rel="noreferrer">
              Guia do Código
            </a>
            <button type="button" onClick={onEnterSimulator}>
              Prévia Web
            </button>
          </div>
          <span>MIT · Tauri 2, Rust, React 19 e Three.js</span>
        </footer>
      </main>
    </div>
  );
}

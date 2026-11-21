import { useEffect, useRef, useState } from "react";
import {
  Aperture,
  ArrowRight,
  Bot,
  Check,
  ClipboardPaste,
  Code2,
  Copy,
  Cpu,
  Download,
  Eye,
  FileCode2,
  FolderGit2,
  Github,
  Globe,
  Layers,
  Mic,
  Palette,
  Play,
  Radio,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Terminal,
  Volume2,
  Zap,
} from "lucide-react";
import Stage, { type VisualTheme } from "./Stage";
import { HudOverlay, VoiceSpectrum } from "./HudOverlay";
import { AudioEngine } from "./audio";
import mascotSheetUrl from "./assets/mascot-sheet.png";
import type { CoreState } from "./rhythm";

const GALLERY_SLIDES = [
  {
    id: "cockpit",
    title: "Cockpit Principal 3D & HUD Orbital",
    subtitle: "Esfera de plasma GLSL reativa à voz, retícula giroscópica e resposta Markdown ao vivo.",
    img: "/og-cover.png",
  },
  {
    id: "browser",
    title: "Navegador Chromium 60fps Dividido (Estilo Codex)",
    subtitle: "O próprio J.A.R.V.I.S. abre sites ao lado da conversa, inspeciona elementos e clica/digita via CDP.",
    img: "/gallery-browser.png",
  },
  {
    id: "agents",
    title: "Central de Subagentes Paralelos",
    subtitle: "Aba contextual dinâmica que surge quando o J.A.R.V.I.S. delega pesquisas em segundo plano.",
    img: "/gallery-agents.png",
  },
  {
    id: "awakening",
    title: "Modo Imersivo & Despertar do Reator",
    subtitle: "Visualização pura em tela cheia (F11) do núcleo de plasma em 60 quadros por segundo.",
    img: "/gallery-awakening.png",
  },
];

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

const STATE_DEMOS: Array<{
  id: CoreState;
  label: string;
  caption: string;
  bubble: string;
}> = [
  {
    id: "idle",
    label: "Em Repouso",
    caption: "Núcleo estabilizado · Wake Word 'Ei, Jarvis' monitorando localmente (0% rede)",
    bubble: "Pronto às ordens, senhor!",
  },
  {
    id: "listening",
    label: "Ouvindo Voz",
    caption: "Win32 waveIn 16kHz PCM ativo · VAD detectando fala em tempo real",
    bubble: 'Ouvindo: "Ei, Jarvis, olha meu jogo..."',
  },
  {
    id: "thinking",
    label: "Processando",
    caption: "Executando list_open_windows → see_screen(target_window: 'Minecraft')",
    bubble: "Capturando janela em 2º plano...",
  },
  {
    id: "responding",
    label: "Respondendo",
    caption: "Síntese neural ElevenLabs Flash v2.5 + cache local SHA-256 em disco",
    bubble: "Seu inventário está cheio; recomendo guardar os minérios, senhor.",
  },
  {
    id: "alert",
    label: "Alerta Âmbar",
    caption: "Pulso prioritário de timer, aprovação de segurança ou aviso crítico",
    bubble: "⏰ Timer concluído: Deploy em Produção!",
  },
];

type PillarId = "vision" | "browser" | "diff" | "voice";

function MiniMascotPreview({ mode, bubble }: { mode: CoreState; bubble: string }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const img = new Image();
    img.src = mascotSheetUrl;
    let raf = 0;
    let col = 0;
    let lastStep = performance.now();

    img.onload = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = 76 * dpr;
      canvas.height = 76 * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      const cellW = img.width / 6;
      const cellH = img.height / 10;

      const tick = (now: number) => {
        const row =
          mode === "listening"
            ? 6
            : mode === "thinking"
              ? 4
              : mode === "responding"
                ? 2
                : mode === "alert"
                  ? 5
                  : 0;
        if (now - lastStep >= 115) {
          lastStep = now;
          col = (col + 1) % 6;
        }
        ctx.clearRect(0, 0, 76, 76);
        const bob = Math.sin(now * 0.0035) * 2.5;
        ctx.drawImage(img, col * cellW, row * cellH, cellW, cellH, 4, 4 + bob, 68, 68);
        raf = requestAnimationFrame(tick);
      };
      raf = requestAnimationFrame(tick);
    };

    return () => cancelAnimationFrame(raf);
  }, [mode]);

  return (
    <div className="lp-mascot-card">
      <div className="lp-mascot-bubble">{bubble}</div>
      <div className="lp-mascot-row">
        <canvas ref={canvasRef} width={76} height={76} className="lp-mascot-canvas" />
        <div className="lp-mascot-info">
          <strong>Mascote Global Always-On-Top</strong>
          <span>Flutua por todo o Windows · Menu rápido no botão direito</span>
        </div>
      </div>
    </div>
  );
}

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
  const [activePillar, setActivePillar] = useState<PillarId>("vision");
  const [activeSlide, setActiveSlide] = useState<string>("cockpit");
  const [copiedCmd, setCopiedCmd] = useState(false);
  const demoAudioRef = useRef<number>(0.42);
  const sfxRef = useRef<AudioEngine | null>(null);

  useEffect(() => {
    return () => {
      sfxRef.current?.dispose();
    };
  }, []);

  function selectDemoState(next: CoreState) {
    setDemoState(next);
    setHeroVisualMode("live3d");
    try {
      sfxRef.current ??= new AudioEngine();
      sfxRef.current.setLevel(0.38, false);
      void sfxRef.current.cue(next, { suppressLoop: true }).catch(() => {});
    } catch {
      /* ignore autoplay block */
    }
  }

  // Animate synthetic audio level when demo state is listening or responding
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

  const currentDemo =
    STATE_DEMOS.find((d) => d.id === demoState) ?? STATE_DEMOS[0];

  async function handleCopyInstall() {
    const cmd = `git clone https://github.com/k48w/Jarvis.git\ncd Jarvis\nnpm ci\nnpm run tauri -- build`;
    try {
      await navigator.clipboard.writeText(cmd);
      setCopiedCmd(true);
      window.setTimeout(() => setCopiedCmd(false), 1800);
    } catch {
      /* ignore */
    }
  }

  return (
    <div className="lp-root" data-theme={theme} data-state={demoState}>
      {/* Ambient background glow */}
      <div className="lp-ambient-grid" aria-hidden="true" />
      <div className="lp-ambient-radial" aria-hidden="true" />

      {/* Sticky Glass Navbar */}
      <header className="lp-navbar">
        <div className="lp-nav-brand">
          <img src="/app-icon.jpg" alt="" className="lp-app-emblem" />
          <strong>J.A.R.V.I.S.</strong>
          <span className="lp-version-tag">V8.0</span>
        </div>

        <nav className="lp-nav-links" aria-label="Navegação do site">
          <a href="#pilares">Engenharia</a>
          <a href="#recursos">Recursos</a>
          <a href="#comparativo">Arquitetura</a>
          <a href="#download">Download</a>
        </nav>

        <div className="lp-nav-actions">
          <div className="lp-theme-dots" role="group" aria-label="Cor do núcleo">
            {THEME_OPTIONS.map((t) => (
              <button
                key={t.id}
                type="button"
                className={theme === t.id ? "lp-theme-dot active" : "lp-theme-dot"}
                title={`Tema ${t.label}`}
                aria-label={`Tema ${t.label}`}
                onClick={() => onThemeChange(t.id)}
              >
                <span style={{ background: t.color, boxShadow: `0 0 10px ${t.color}` }} />
              </button>
            ))}
          </div>

          <button
            type="button"
            className="lp-btn-ghost"
            onClick={onEnterSimulator}
          >
            <Play /> Simulador 3D
          </button>

          <a
            href={GITHUB_RELEASE_URL}
            target="_blank"
            rel="noreferrer"
            className="lp-btn-primary"
          >
            <Download /> Baixar Windows
          </a>
        </div>
      </header>

      {/* HERO SECTION */}
      <section className="lp-hero">
        <div className="lp-hero-copy">
          <div className="lp-kicker-pill">
            <span className="lp-live-dot" />
            <span>TAURI 2 · RUST WIN32 NATIVO · REACT 19 · THREE.JS GLSL</span>
          </div>

          <h1>
            Não é só um chat.
            <br />
            <span>É um copiloto vivo no seu Windows.</span>
          </h1>

          <p className="lp-hero-lead">
            O <strong>J.A.R.V.I.S. V8.0 — The Singularity Edition</strong> une um núcleo de
            plasma 3D reativo à voz, mascote animado em toda a área de trabalho, comando de
            voz <code>&ldquo;Ei, Jarvis&rdquo;</code> nativo em Rust, visão inteligente que
            enxerga seu jogo mesmo atrás do navegador, Chromium 60fps embutido e edição de
            código com diff visual.
          </p>

          <div className="lp-hero-cta-row">
            <a
              href={GITHUB_RELEASE_URL}
              target="_blank"
              rel="noreferrer"
              className="lp-btn-primary lp-btn-lg"
            >
              <Download />
              <div>
                <strong>Baixar para Windows x64</strong>
                <small>JARVIS_8.0.0_x64-setup.exe · ~5 MB</small>
              </div>
            </a>

            <button
              type="button"
              className="lp-btn-secondary lp-btn-lg"
              onClick={onEnterSimulator}
            >
              <Sparkles />
              <div>
                <strong>Abrir Simulador 3D ao Vivo</strong>
                <small>Teste o cockpit direto no navegador</small>
              </div>
              <ArrowRight />
            </button>
          </div>

          <div className="lp-hero-metrics">
            <div className="lp-metric">
              <strong>100% Rust</strong>
              <span>DXGI, PrintWindow, waveIn &amp; SendInput sem PowerShell</span>
            </div>
            <div className="lp-metric">
              <strong>0% Rede</strong>
              <span>Durante o silêncio com VAD local no Wake Word</span>
            </div>
            <div className="lp-metric">
              <strong>60 FPS</strong>
              <span>Shaders GLSL + Child WebView2 acelerado por GPU</span>
            </div>
          </div>
        </div>

        {/* Minimalist Studio Orb + Interactive 3D Cockpit Stage Preview */}
        <div className="lp-hero-stage-card">
          <div className="lp-stage-topbar">
            <div className="lp-visual-mode-pills">
              <button
                type="button"
                className={heroVisualMode === "studio" ? "lp-vmode-btn active" : "lp-vmode-btn"}
                onClick={() => setHeroVisualMode("studio")}
              >
                Esfera Minimalista
              </button>
              <button
                type="button"
                className={heroVisualMode === "live3d" ? "lp-vmode-btn active" : "lp-vmode-btn"}
                onClick={() => setHeroVisualMode("live3d")}
              >
                WebGL 3D Interativo
              </button>
            </div>
            <button
              type="button"
              className="lp-stage-expand"
              onClick={onEnterSimulator}
            >
              Simulador Completo <ArrowRight />
            </button>
          </div>

          <div className="lp-stage-viewport">
            {heroVisualMode === "studio" ? (
              <div
                className="lp-studio-orb-wrap"
                onClick={() => setHeroVisualMode("live3d")}
                title="Clique para interagir com o shader 3D em tempo real"
              >
                <img
                  src="/hero-orb.jpg"
                  alt="Esfera minimalista de vidro fumê do J.A.R.V.I.S."
                  className="lp-studio-orb-img"
                />
              </div>
            ) : (
              <>
                <Stage
                  state={demoState}
                  quality="High"
                  theme={theme}
                  reduced={false}
                  audioLevel={demoAudioRef}
                />
                <HudOverlay state={demoState} immersive={false} />
              </>
            )}

            <div className="lp-stage-hud-caption">
              <span className="lp-state-badge">
                <i /> {currentDemo.label}
              </span>
              <p>{currentDemo.caption}</p>
            </div>

            {heroVisualMode === "live3d" && (
              <div className="lp-stage-spectrum">
                <VoiceSpectrum
                  state={demoState}
                  audioLevel={demoAudioRef}
                  active={demoState !== "idle"}
                />
              </div>
            )}

            <MiniMascotPreview mode={demoState} bubble={currentDemo.bubble} />
          </div>

          <div className="lp-state-switcher" role="tablist" aria-label="Testar estados do núcleo">
            {STATE_DEMOS.map((st) => (
              <button
                key={st.id}
                type="button"
                role="tab"
                aria-selected={demoState === st.id}
                className={demoState === st.id ? "lp-state-btn active" : "lp-state-btn"}
                onClick={() => selectDemoState(st.id)}
              >
                {st.label}
              </button>
            ))}
          </div>
        </div>
      </section>

      {/* MINIMALIST VOICE FREQUENCY RIBBON DIVIDER */}
      <div className="lp-voice-ribbon" aria-hidden="true">
        <img src="/voice-wave.jpg" alt="" />
      </div>

      {/* 4 INTERACTIVE ENGINEERING PILLARS */}
      <section id="pilares" className="lp-section">
        <div className="lp-section-head">
          <span className="lp-eyebrow">ENGENHARIA DE PONTA A PONTA</span>
          <h2>Quatro sistemas nativos que nenhum assistente web tem</h2>
          <p>
            Projetado em Rust puro diretamente sobre as APIs Win32 do Windows 10/11 para
            enxergar, ouvir, navegar e programar sem bloqueios de antivírus nem travamentos.
          </p>
        </div>

        <div className="lp-pillar-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activePillar === "vision"}
            className={activePillar === "vision" ? "lp-pillar-tab active" : "lp-pillar-tab"}
            onClick={() => setActivePillar("vision")}
          >
            <Eye />
            <div>
              <strong>1. Visão Inteligente de Jogos &amp; Janelas</strong>
              <span>PrintWindow em 2º plano + fallback DXGI</span>
            </div>
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={activePillar === "voice"}
            className={activePillar === "voice" ? "lp-pillar-tab active" : "lp-pillar-tab"}
            onClick={() => setActivePillar("voice")}
          >
            <Radio />
            <div>
              <strong>2. Wake Word &ldquo;Ei, Jarvis&rdquo; &amp; Voz Neural</strong>
              <span>Win32 waveIn + ElevenLabs Scribe &amp; Flash v2.5</span>
            </div>
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={activePillar === "browser"}
            className={activePillar === "browser" ? "lp-pillar-tab active" : "lp-pillar-tab"}
            onClick={() => setActivePillar("browser")}
          >
            <Globe />
            <div>
              <strong>3. Navegador 60fps &amp; Subagentes</strong>
              <span>Split-screen estilo Codex controlado via CDP</span>
            </div>
          </button>

          <button
            type="button"
            role="tab"
            aria-selected={activePillar === "diff"}
            className={activePillar === "diff" ? "lp-pillar-tab active" : "lp-pillar-tab"}
            onClick={() => setActivePillar("diff")}
          >
            <FileCode2 />
            <div>
              <strong>4. Projetos, Diff Colorido &amp; Ctrl+C</strong>
              <span>Edição cirúrgica com Desfazer em 1 clique</span>
            </div>
          </button>
        </div>

        <div className="lp-pillar-showcase">
          {activePillar === "vision" && (
            <div className="lp-pillar-grid">
              <div className="lp-pillar-info">
                <span className="lp-badge">ZERO ROUBO DE FOCO · ZERO POWERSHELL</span>
                <h3>Peça para ele olhar seu jogo mesmo enquanto você está no navegador</h3>
                <p>
                  Assistentes comuns tiram um print cego do monitor e acabam lendo a própria
                  janela. O J.A.R.V.I.S. enumera todas as janelas reais do Windows
                  (<code>EnumWindows</code> + <code>sysinfo</code>), entende qual app ou jogo
                  você pediu e captura a janela alvo diretamente.
                </p>
                <ul className="lp-check-list">
                  <li>
                    <Check /> <strong>Captura em 2º plano via <code>PrintWindow(PW_RENDERFULLCONTENT)</code></strong>: enxerga janelas cobertas pelo Firefox ou VS Code sem trocar seu foco.
                  </li>
                  <li>
                    <Check /> <strong>Fallback automático para jogos DirectX / Vulkan</strong>: detecta quadros pretos de GPU exclusiva, restaura a janela e captura via <strong>DXGI Desktop Duplication</strong>.
                  </li>
                  <li>
                    <Check /> <strong>Invisível para si mesmo</strong>: aplica <code>SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)</code> no app e no mascote.
                  </li>
                </ul>
              </div>

              <div className="lp-terminal-card">
                <div className="lp-terminal-head">
                  <span>jarvis::tools::see_screen</span>
                  <code>Win32 Native Pipeline</code>
                </div>
                <div className="lp-terminal-body">
                  <div className="lp-term-line user">
                    <span>Você:</span> &ldquo;Jarvis, olha meu jogo e vê o que falta pra craftar&rdquo;
                  </div>
                  <div className="lp-term-line tool">
                    <span>[tool]</span> list_open_windows() → 3 janelas detectadas:
                    <br />
                    &nbsp;&nbsp;• [firefox.exe] YouTube — Mozilla Firefox (aberta, 1920x1040)
                    <br />
                    &nbsp;&nbsp;• [Code.exe] Jarvis-Cinematic - VS Code (aberta, 1440x900)
                    <br />
                    &nbsp;&nbsp;• [javaw.exe] Minecraft 1.21.4 - Multiplayer (aberta, 1920x1080)
                  </div>
                  <div className="lp-term-line tool">
                    <span>[match]</span> Alvo &ldquo;meu jogo&rdquo; → <strong>[javaw.exe] Minecraft 1.21.4</strong> (score 120)
                  </div>
                  <div className="lp-term-line ok">
                    <span>[capture]</span> PrintWindow(PW_RENDERFULLCONTENT) → 1280×720 JPEG (sem trocar foco)
                  </div>
                  <div className="lp-term-line ai">
                    <span>J.A.R.V.I.S.:</span> &ldquo;Você já tem as 3 barras de ferro na bancada, senhor; só faltam 2 gravetos no inventário.&rdquo;
                  </div>
                </div>
              </div>
            </div>
          )}

          {activePillar === "voice" && (
            <div className="lp-pillar-grid">
              <div className="lp-pillar-info">
                <span className="lp-badge">WIN32 WAVEIN 16KHZ + ELEVENLABS SCRIBE &amp; FLASH</span>
                <h3>Chame &ldquo;Ei, Jarvis&rdquo; de qualquer lugar do PC — até no meio de uma partida</h3>
                <p>
                  Em vez de depender da Web Speech API quebrada do WebView2, o J.A.R.V.I.S.
                  possui um motor de áudio nativo em Rust (<code>src-tauri/src/mic.rs</code>)
                  que lê seu microfone diretamente pelo Windows Multimedia (<code>waveInOpen</code>).
                </p>
                <ul className="lp-check-list">
                  <li>
                    <Check /> <strong>Wake Word com VAD Local</strong>: buffer circular pré-fala de 480ms que só envia áudio para transcrição quando detecta voz humana real.
                  </li>
                  <li>
                    <Check /> <strong>Modo Mãos-Livres Contínuo</strong>: após responder, volta a ouvir automaticamente para conversas fluidas (diga <em>&ldquo;parar&rdquo;</em> ou <em>&ldquo;tchau&rdquo;</em> para encerrar).
                  </li>
                  <li>
                    <Check /> <strong>Cache Local SHA-256 da ElevenLabs</strong>: respostas faladas ficam salvas em disco para nunca gastar créditos repetidos.
                  </li>
                </ul>
              </div>

              <div className="lp-terminal-card">
                <div className="lp-terminal-head">
                  <span>jarvis::mic::MicController</span>
                  <code>16kHz Mono PCM · Local VAD</code>
                </div>
                <div className="lp-terminal-body">
                  <div className="lp-term-line tool">
                    <span>[waveIn]</span> Microfone (High Definition Audio Device) · 16000 Hz 16-bit
                  </div>
                  <div className="lp-term-line ok">
                    <span>[VAD]</span> Silêncio (RMS 0.002) → 0 bytes enviados · 0 créditos gastos
                  </div>
                  <div className="lp-term-line user">
                    <span>[Voz detectada]</span> RMS 0.084 (1.4s) → ElevenLabs Scribe v2 (pt-BR)
                  </div>
                  <div className="lp-term-line tool">
                    <span>[Wake Match]</span> &ldquo;Ei Jarvis, coloca um timer de 10 minutos pro café&rdquo;
                  </div>
                  <div className="lp-term-line ai">
                    <span>[ElevenLabs Flash v2.5]</span> &ldquo;Pronto, senhor. Timer de 10 minutos iniciado para o seu café.&rdquo;
                  </div>
                </div>
              </div>
            </div>
          )}

          {activePillar === "browser" && (
            <div className="lp-pillar-grid">
              <div className="lp-pillar-info">
                <span className="lp-badge">CHILD WEBVIEW2 60FPS + CHROME DEVTOOLS PROTOCOL</span>
                <h3>Navegador real dividido na tela que o próprio J.A.R.V.I.S. enxerga e opera</h3>
                <p>
                  As abas de <strong>Navegador</strong>, <strong>Agentes</strong> e{" "}
                  <strong>Código</strong> só aparecem no topo quando são necessárias. Peça para
                  o J.A.R.V.I.S. olhar o WhatsApp Web, pesquisar no YouTube ou abrir um site e
                  a tela se divide ao meio estilo Codex.
                </p>
                <ul className="lp-check-list">
                  <li>
                    <Check /> <strong>8 Ferramentas CDP Autônomas</strong>: <code>browser_navigate</code>, <code>browser_inspect</code>, <code>browser_click</code>, <code>browser_type</code>, <code>browser_scroll</code>, <code>browser_press_key</code>, <code>browser_back</code> e <code>browser_close</code>.
                  </li>
                  <li>
                    <Check /> <strong>Mapeamento Visual de Elementos</strong>: identifica botões, inputs e links numerados com coordenadas exatas para clicar e digitar por você.
                  </li>
                  <li>
                    <Check /> <strong>Subagentes Paralelos (<code>spawn_agent</code>)</strong>: delega pesquisas longas para agentes em 2º plano e avisa pelo mascote quando terminam.
                  </li>
                </ul>
              </div>

              <div className="lp-split-preview">
                <div className="lp-split-left">
                  <small>CHAT LATERAL</small>
                  <div className="lp-split-bubble user">
                    Abre o GitHub no navegador embutido e procura repositórios de Rust
                  </div>
                  <div className="lp-split-chip">🌐 Abrir site no navegador · github.com</div>
                  <div className="lp-split-chip">🖱️ Digitar no navegador · &ldquo;rust tauri&rdquo;</div>
                  <div className="lp-split-bubble ai">
                    Pronto, senhor. Abri o GitHub ao lado e filtrei os repositórios.
                  </div>
                </div>
                <div className="lp-split-right">
                  <div className="lp-split-urlbar">
                    <Globe /> <span>https://github.com/search?q=rust+tauri</span>
                  </div>
                  <div className="lp-split-page">
                    <div className="lp-split-repo">
                      <strong>k48w/Jarvis</strong>
                      <span>J.A.R.V.I.S. V8.0 — The Singularity Edition (Tauri 2 + Rust + Three.js)</span>
                    </div>
                    <div className="lp-split-repo">
                      <strong>tauri-apps/tauri</strong>
                      <span>Build smaller, faster, and more secure desktop applications with Rust</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activePillar === "diff" && (
            <div className="lp-pillar-grid">
              <div className="lp-pillar-info">
                <span className="lp-badge">PROJETOS ESTILO CODEX + DIFF VISUAL + CTRL+C</span>
                <h3>Vincule pastas de projetos, revise cada linha alterada e desfaça com 1 clique</h3>
                <p>
                  Vincule qualquer repositório do seu PC à conversa pelo seletor nativo do
                  Windows (<code>IFileOpenDialog</code>). Quando o J.A.R.V.I.S. cria ou edita
                  arquivos, a aba contextual <strong>Código</strong> exibe o diff completo.
                </p>
                <ul className="lp-check-list">
                  <li>
                    <Check /> <strong>Reversão Instantânea (<code>revert_file_change</code>)</strong>: restaura o conteúdo original do arquivo com 1 clique se quiser desfazer.
                  </li>
                  <li>
                    <Check /> <strong>Leitura Nativa do <code>Ctrl+C</code> (<code>read_clipboard</code>)</strong>: lê tanto textos copiados (<code>CF_UNICODETEXT</code>) quanto prints na área de transferência (<code>CF_DIB</code>).
                  </li>
                  <li>
                    <Check /> <strong>Drag &amp; Drop + <code>Ctrl+V</code></strong>: solte imagens, códigos, JSON ou PDFs direto na conversa.
                  </li>
                </ul>
              </div>

              <div className="lp-diff-preview">
                <div className="lp-diff-head">
                  <span>
                    <FileCode2 /> src/main.tsx
                  </span>
                  <div className="lp-diff-badges">
                    <em className="plus">+4</em>
                    <em className="minus">-1</em>
                    <span className="lp-undo-pill">
                      <RotateCcw /> Desfazer alteração
                    </span>
                  </div>
                </div>
                <div className="lp-diff-lines">
                  <div className="lp-dl ctx">
                    <span>42</span>
                    <pre>const config = await loadJarvisConfig();</pre>
                  </div>
                  <div className="lp-dl del">
                    <span>43</span>
                    <pre>- const wakeWord = false;</pre>
                  </div>
                  <div className="lp-dl add">
                    <span>43</span>
                    <pre>+ const wakeWord = config.wake_word_enabled;</pre>
                  </div>
                  <div className="lp-dl add">
                    <span>44</span>
                    <pre>+ await setNativeWakeWord(wakeWord, config.mic_device_id);</pre>
                  </div>
                  <div className="lp-dl ctx">
                    <span>45</span>
                    <pre>activate(&quot;idle&quot;);</pre>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </section>

      {/* BENTO GRID OF FEATURES */}
      <section id="recursos" className="lp-section">
        <div className="lp-section-head">
          <span className="lp-eyebrow">ARSENAL COMPLETO</span>
          <h2>Tudo o que um assistente de elite precisa no desktop</h2>
        </div>

        <div className="lp-bento-grid">
          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <Bot />
            </div>
            <h3>Mascote Vivo em Todo o PC</h3>
            <p>
              Sprite sheet 6×10 a 60fps em janela transparente sempre no topo. Mostra balões
              com as respostas, comemora quando agentes terminam, dorme quando ocioso e tem
              menu rápido no clique direito.
            </p>
          </article>

          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <Zap />
            </div>
            <h3>ChatGPT OAuth &amp; Google Gemini</h3>
            <p>
              Use sua assinatura ChatGPT Plus/Pro via fluxo oficial OAuth 2.0 + PKCE (sem
              pagar API separada) ou conecte Google Gemini via API Key / Conta Google OAuth.
            </p>
          </article>

          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <Volume2 />
            </div>
            <h3>Voz ElevenLabs com Economia</h3>
            <p>
              Suporte às suas vozes clonadas da ElevenLabs, cache local em disco para frases
              repetidas e modo Econômico inteligente (~220 caracteres) para poupar créditos.
            </p>
          </article>

          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <ClipboardPaste />
            </div>
            <h3>Leitura do Ctrl+C &amp; Anexos</h3>
            <p>
              Diga <em>&ldquo;Jarvis, analisa o que eu copiei&rdquo;</em> para ele ler textos ou
              prints direto da área de transferência do Windows, ou arraste arquivos na tela.
            </p>
          </article>

          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <FolderGit2 />
            </div>
            <h3>Conversas &amp; Projetos Codex</h3>
            <p>
              Organize múltiplas conversas agrupadas por pastas de projeto reais do seu
              computador, com renomeação inline e árvore de arquivos automática no prompt.
            </p>
          </article>

          <article className="lp-bento-card">
            <div className="lp-bento-icon">
              <ShieldCheck />
            </div>
            <h3>3 Níveis de Permissão &amp; Segurança</h3>
            <p>
              Alterne com 1 clique entre <strong>Permissão Total</strong>,{" "}
              <strong>Revisão Automática</strong> (pede confirmação antes de agir) e{" "}
              <strong>Somente Conversa</strong>, com proteção contra pastas críticas do sistema.
            </p>
          </article>
        </div>
      </section>

      {/* INTERACTIVE SCREENSHOT GALLERY */}
      <section id="galeria" className="lp-section">
        <div className="lp-section-head">
          <span className="lp-eyebrow">GALERIA DO APLICATIVO</span>
          <h2>Veja cada modo de operação em alta definição</h2>
          <p>
            Capturas reais geradas diretamente da versão empacotada para Windows (WebView2 +
            WebGL2).
          </p>
        </div>

        <div className="lp-gallery-tabs" role="tablist">
          {GALLERY_SLIDES.map((s) => (
            <button
              key={s.id}
              type="button"
              role="tab"
              aria-selected={activeSlide === s.id}
              className={activeSlide === s.id ? "lp-gallery-tab active" : "lp-gallery-tab"}
              onClick={() => setActiveSlide(s.id)}
            >
              <strong>{s.title}</strong>
              <span>{s.subtitle}</span>
            </button>
          ))}
        </div>

        {GALLERY_SLIDES.filter((s) => s.id === activeSlide).map((slide) => (
          <div key={slide.id} className="lp-gallery-frame">
            <img src={slide.img} alt={slide.title} loading="lazy" />
          </div>
        ))}
      </section>

      {/* ARCHITECTURE COMPARISON TABLE */}
      <section id="comparativo" className="lp-section">
        <div className="lp-section-head">
          <span className="lp-eyebrow">COMPARATIVO TÉCNICO</span>
          <h2>Por que o J.A.R.V.I.S. V8.0 está em outro patamar</h2>
        </div>

        <div className="lp-table-wrap">
          <table className="lp-compare-table">
            <thead>
              <tr>
                <th>Capacidade</th>
                <th className="highlight">J.A.R.V.I.S. V8.0 (Singularity)</th>
                <th>Chats Web / Apps Electron Comuns</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>Visão de Jogos &amp; Janelas</td>
                <td className="highlight">
                  <strong>PrintWindow em 2º plano + DXGI</strong> (vê atrás do navegador sem roubar foco)
                </td>
                <td>Print manual ou captura só o que está na frente</td>
              </tr>
              <tr>
                <td>Microfone &amp; Wake Word</td>
                <td className="highlight">
                  <strong>Rust Win32 <code>waveIn</code> nativo</strong> (funciona minimizado e sem erro de permissão)
                </td>
                <td>Depende do navegador / falha em WebViews</td>
              </tr>
              <tr>
                <td>Navegador Integrado</td>
                <td className="highlight">
                  <strong>Child WebView2 60fps + 8 ferramentas CDP</strong> em tela dividida
                </td>
                <td>Apenas busca textual simples sem interação</td>
              </tr>
              <tr>
                <td>Presença no Desktop</td>
                <td className="highlight">
                  <strong>Mascote animado global Always-On-Top</strong> + Atalho <code>Alt+Space</code>
                </td>
                <td>Preso dentro de uma aba ou janela comum</td>
              </tr>
              <tr>
                <td>Edição de Código &amp; Arquivos</td>
                <td className="highlight">
                  <strong>Projetos vinculados + Diff colorido + Desfazer em 1 clique</strong>
                </td>
                <td>Copiar e colar manualmente</td>
              </tr>
              <tr>
                <td>Tamanho do Instalador</td>
                <td className="highlight">
                  <strong>~5.0 MB (Tauri 2 + Rust Nativo)</strong>
                </td>
                <td>150 MB – 300 MB (Electron)</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      {/* DOWNLOAD & BUILD CTA */}
      <section id="download" className="lp-section lp-download-section">
        <div className="lp-download-card">
          <div className="lp-download-copy">
            <span className="lp-eyebrow">PRONTO PARA INSTALAR OU COMPILAR</span>
            <h2>Coloque o J.A.R.V.I.S. V8.0 no seu Windows hoje</h2>
            <p>
              Baixe o instalador oficial pronto para Windows 10/11 x64, compile a partir do
              código-fonte em Rust ou experimente o simulador 3D interativo agora mesmo.
            </p>

            <div className="lp-hero-cta-row">
              <a
                href={GITHUB_RELEASE_URL}
                target="_blank"
                rel="noreferrer"
                className="lp-btn-primary lp-btn-lg"
              >
                <Download />
                <div>
                  <strong>Baixar Instalador Windows (.exe)</strong>
                  <small>GitHub Releases · v8.0.0</small>
                </div>
              </a>

              <button
                type="button"
                className="lp-btn-secondary lp-btn-lg"
                onClick={onEnterSimulator}
              >
                <Play />
                <div>
                  <strong>Testar Simulador 3D Agora</strong>
                  <small>Abrir interface completa no navegador</small>
                </div>
              </button>

              <a
                href={GITHUB_REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="lp-btn-ghost lp-btn-lg"
              >
                <Github /> Repositório GitHub
              </a>
            </div>
          </div>

          <div className="lp-code-box">
            <div className="lp-code-head">
              <span>
                <Terminal /> Build Local (Windows x64 + Rust)
              </span>
              <button type="button" className="lp-copy-btn" onClick={() => void handleCopyInstall()}>
                {copiedCmd ? <Check /> : <Copy />} {copiedCmd ? "Copiado!" : "Copiar"}
              </button>
            </div>
            <pre>
              <code>{`git clone https://github.com/k48w/Jarvis.git
cd Jarvis
npm ci
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build`}</code>
            </pre>
          </div>
        </div>
      </section>

      <footer className="lp-footer">
        <div className="lp-nav-brand">
          <img src="/app-icon.jpg" alt="" className="lp-app-emblem" />
          <strong>J.A.R.V.I.S. V8.0 — The Singularity Edition</strong>
        </div>
        <div className="lp-footer-links">
          <a href={GITHUB_REPO_URL} target="_blank" rel="noreferrer">
            GitHub (k48w/Jarvis)
          </a>
          <a href={GITHUB_RELEASES_PAGE} target="_blank" rel="noreferrer">
            Releases (v8.0.0)
          </a>
          <a href={GITHUB_RELEASE_URL} target="_blank" rel="noreferrer">
            Baixar .exe Direto
          </a>
          <button type="button" className="lp-link-btn" onClick={onEnterSimulator}>
            Simulador 3D Web
          </button>
        </div>
        <small>MIT License · Tauri 2 + Rust + React 19 + Three.js</small>
      </footer>
    </div>
  );
}

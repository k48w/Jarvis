import { useEffect, useRef, useState, type RefObject } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AudioLines,
  Eye,
  EyeOff,
  MessageSquare,
  Palette,
  Radio,
  SquarePen,
  Volume2,
  VolumeX,
} from "lucide-react";
import mascotSheetUrl from "./assets/mascot-sheet.png";
import { mascotAction, onMascotState, type MascotActionKind } from "./bridge";
import type { CoreState } from "./rhythm";
import type { VisualTheme } from "./Stage";

const COLS = 6;
const ROWS = 10;

// Row mapping verified against the 6x10 sprite sheet:
// 0: float idle | 1: blink | 2: talking | 3: curious wiggle | 4: thinking
// 5: celebrating | 6: listening/alert | 7: hover attentive | 8: waving | 9: sleepy
const ROW_FPS: Record<number, number> = {
  0: 6,
  1: 8,
  2: 11,
  3: 8,
  4: 7,
  5: 10,
  6: 8,
  7: 6,
  8: 9,
  9: 4,
};

export function MascotBuddy({
  mode,
  speaking,
  reduced,
  audioLevel,
  disabledMic,
  draggableWindow = false,
  externalBubble = null,
  externalReaction = null,
  muted = false,
  wakeWord = false,
  onNewChat,
  onToggleMic,
  onQuickAction,
}: {
  mode: CoreState;
  speaking: boolean;
  reduced: boolean;
  audioLevel: RefObject<number>;
  disabledMic: boolean;
  draggableWindow?: boolean;
  externalBubble?: string | null;
  externalReaction?: "wave" | "cheer" | "alert" | null;
  muted?: boolean;
  wakeWord?: boolean;
  onNewChat: () => void;
  onToggleMic: () => void;
  onQuickAction?: (action: MascotActionKind) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [reaction, setReaction] = useState<"wave" | "cheer" | "alert" | null>("wave");
  const [bubble, setBubble] = useState<string | null>("Olá, senhor!");
  const [menuOpen, setMenuOpen] = useState(false);
  const lastActivityRef = useRef(Date.now());
  const reactionTimerRef = useRef(0);
  const bubbleTimerRef = useRef(0);
  const dragOriginRef = useRef<{ x: number; y: number } | null>(null);
  const draggedRef = useRef(false);

  const modeRef = useRef(mode);
  const speakingRef = useRef(speaking);
  const reducedRef = useRef(reduced);
  const reactionRef = useRef(reaction);

  modeRef.current = mode;
  speakingRef.current = speaking;
  reducedRef.current = reduced;
  reactionRef.current = reaction;

  // Load sprite sheet once
  useEffect(() => {
    const img = new Image();
    img.src = mascotSheetUrl;
    img.onload = () => {
      imgRef.current = img;
      setLoaded(true);
    };
    return () => {
      imgRef.current = null;
    };
  }, []);

  // Clear initial startup wave/bubble smoothly
  useEffect(() => {
    reactionTimerRef.current = window.setTimeout(() => setReaction(null), 1700);
    bubbleTimerRef.current = window.setTimeout(() => setBubble(null), 3200);
    return () => {
      clearTimeout(reactionTimerRef.current);
      clearTimeout(bubbleTimerRef.current);
    };
  }, []);

  // Sync live speech bubbles from main J.A.R.V.I.S. window
  useEffect(() => {
    if (!externalBubble) return;
    lastActivityRef.current = Date.now();
    clearTimeout(bubbleTimerRef.current);
    setBubble(externalBubble);
    bubbleTimerRef.current = window.setTimeout(() => setBubble(null), 6500);
  }, [externalBubble]);

  // Sync live reactions (cheer when agent/task completes, alert on warning/timer)
  useEffect(() => {
    if (!externalReaction) return;
    lastActivityRef.current = Date.now();
    clearTimeout(reactionTimerRef.current);
    setReaction(externalReaction);
    reactionTimerRef.current = window.setTimeout(
      () => setReaction(null),
      externalReaction === "alert" ? 2600 : 1800,
    );
  }, [externalReaction]);

  // Close quick menu on Escape or outside click
  useEffect(() => {
    if (!menuOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    const onDown = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target?.closest(".mascot-menu")) {
        setMenuOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, [menuOpen]);

  // Reset idle-sleep clock whenever assistant state changes
  useEffect(() => {
    if (mode !== "idle" || speaking) {
      lastActivityRef.current = Date.now();
    }
  }, [mode, speaking]);

  // Trigger brief cheerful reaction when user clicks mascot
  function handleMascotClick() {
    if (draggedRef.current) {
      draggedRef.current = false;
      return;
    }
    if (menuOpen) {
      setMenuOpen(false);
      return;
    }
    lastActivityRef.current = Date.now();
    clearTimeout(reactionTimerRef.current);
    clearTimeout(bubbleTimerRef.current);

    if (mode === "listening") {
      onToggleMic();
      return;
    }

    setReaction("cheer");
    setBubble(
      mode === "thinking"
        ? "Processando..."
        : speaking
          ? "Falando com você!"
          : wakeWord
            ? 'Ouvindo "Ei, Jarvis"!'
            : "Pronto às ordens!",
    );
    reactionTimerRef.current = window.setTimeout(() => setReaction(null), 1300);
    bubbleTimerRef.current = window.setTimeout(() => setBubble(null), 2400);
  }

  function handleContextMenu(e: React.MouseEvent) {
    e.preventDefault();
    lastActivityRef.current = Date.now();
    setBubble(null);
    setMenuOpen((prev) => !prev);
  }

  function runMenuAction(action: MascotActionKind) {
    setMenuOpen(false);
    lastActivityRef.current = Date.now();
    onQuickAction?.(action);
  }

  function handlePointerDown(e: React.PointerEvent<HTMLButtonElement>) {
    if (!draggableWindow || e.button !== 0) return;
    draggedRef.current = false;
    dragOriginRef.current = { x: e.clientX, y: e.clientY };
  }

  function handlePointerMove(e: React.PointerEvent<HTMLButtonElement>) {
    if (!draggableWindow || !dragOriginRef.current) return;
    const dx = Math.abs(e.clientX - dragOriginRef.current.x);
    const dy = Math.abs(e.clientY - dragOriginRef.current.y);
    if (dx > 5 || dy > 5) {
      dragOriginRef.current = null;
      draggedRef.current = true;
      setMenuOpen(false);
      void getCurrentWindow().startDragging().catch(() => {});
    }
  }

  function handlePointerUp() {
    dragOriginRef.current = null;
  }

  // Smooth DPR-aware canvas sprite loop
  useEffect(() => {
    if (!loaded) return;
    const canvas = canvasRef.current;
    const img = imgRef.current;
    if (!canvas || !img) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const cssW = 92;
    const cssH = 92;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = cssW * dpr;
    canvas.height = cssH * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";

    let raf = 0;
    let col = 0;
    let lastStep = performance.now();
    let idleVariantRow = 0;
    let idleCycles = 0;

    const cellW = img.width / COLS;
    const cellH = img.height / ROWS;

    const pickRow = (): number => {
      const m = modeRef.current;
      const spk = speakingRef.current;
      const react = reactionRef.current;

      if (m === "listening") return 6;
      if (m === "thinking") return 4;
      if (spk || m === "responding") return 2;
      if (m === "alert" || react === "alert") return 6;
      if (react === "cheer") return 5;
      if (react === "wave") return 8;

      if (Date.now() - lastActivityRef.current > 45_000) {
        return 9; // sleepy nap after 45s idle
      }
      return idleVariantRow;
    };

    const renderFrame = (now: number) => {
      const row = pickRow();
      const fps = ROW_FPS[row] ?? 7;
      const interval = 1000 / fps;

      if (!reducedRef.current && now - lastStep >= interval) {
        lastStep = now;
        col = (col + 1) % COLS;

        if (col === 0 && modeRef.current === "idle" && !speakingRef.current && !reactionRef.current) {
          idleCycles += 1;
          if (idleCycles % 4 === 0) {
            idleVariantRow = 1; // gentle blink
          } else if (idleCycles % 9 === 0) {
            idleVariantRow = 3; // cute wiggle
          } else {
            idleVariantRow = 0; // floating idle
          }
        }
      } else if (reducedRef.current) {
        col = 0;
      }

      ctx.clearRect(0, 0, cssW, cssH);

      const lvl = Math.min(1, Math.max(0, audioLevel.current ?? 0));
      const bob = reducedRef.current
        ? 0
        : Math.sin(now * 0.0032) * (modeRef.current === "listening" ? 3.5 + lvl * 4 : 2.5);

      const sx = col * cellW;
      const sy = row * cellH;
      const drawSize = 84;
      const dx = (cssW - drawSize) / 2;
      const dy = (cssH - drawSize) / 2 + bob;

      ctx.drawImage(img, sx, sy, cellW, cellH, dx, dy, drawSize, drawSize);

      raf = requestAnimationFrame(renderFrame);
    };

    raf = requestAnimationFrame(renderFrame);
    return () => cancelAnimationFrame(raf);
  }, [loaded, audioLevel]);

  const isListening = mode === "listening";
  const isAlert = mode === "alert" || reaction === "alert";

  return (
    <div
      className={`mascot-buddy state-${mode}${isListening ? " listening" : ""}${isAlert ? " alert-glow" : ""}`}
      aria-label="Mascote J.A.R.V.I.S."
      onContextMenu={handleContextMenu}
    >
      {menuOpen && (
        <div className="mascot-menu" role="menu" aria-label="Menu rápido do mascote">
          <button type="button" role="menuitem" onClick={() => runMenuAction("see_screen")}>
            <Eye /> Ver tela agora
          </button>
          <button type="button" role="menuitem" onClick={() => runMenuAction("toggle_wake")}>
            <Radio /> Wake Word {wakeWord ? "· ON" : "· OFF"}
          </button>
          <button type="button" role="menuitem" onClick={() => runMenuAction("toggle_mute")}>
            {muted ? <Volume2 /> : <VolumeX />} {muted ? "Ativar som" : "Silenciar"}
          </button>
          <button type="button" role="menuitem" onClick={() => runMenuAction("cycle_theme")}>
            <Palette /> Trocar cor
          </button>
          <button type="button" role="menuitem" onClick={() => runMenuAction("open_chat")}>
            <MessageSquare /> Abrir J.A.R.V.I.S.
          </button>
          <button type="button" role="menuitem" onClick={() => runMenuAction("hide")}>
            <EyeOff /> Ocultar mascote
          </button>
        </div>
      )}

      {!menuOpen && bubble && (
        <button
          type="button"
          className="mascot-bubble"
          role="status"
          title="Clique para abrir J.A.R.V.I.S."
          onClick={() => {
            setBubble(null);
            onNewChat();
          }}
        >
          {bubble}
        </button>
      )}

      <button
        type="button"
        className="mascot-avatar-btn"
        aria-label="Interagir com o mascote J.A.R.V.I.S."
        title={
          draggableWindow
            ? "Clique para interagir · Botão direito: menu · Arraste para mover"
            : "Clique para interagir · Botão direito: menu rápido"
        }
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onClick={handleMascotClick}
        onMouseEnter={() => {
          lastActivityRef.current = Date.now();
        }}
      >
        <span className="mascot-halo-glow" aria-hidden="true" />
        <canvas
          ref={canvasRef}
          className="mascot-canvas"
          width={92}
          height={92}
          aria-hidden="true"
        />
      </button>

      <div className="mascot-pill" role="group" aria-label="Ações rápidas do mascote">
        <button
          type="button"
          className="mascot-pill-btn"
          aria-label="Nova conversa rápida"
          title="Abrir conversa"
          onClick={() => {
            lastActivityRef.current = Date.now();
            setMenuOpen(false);
            onNewChat();
          }}
        >
          <SquarePen />
        </button>
        <span className="mascot-pill-divider" aria-hidden="true" />
        <button
          type="button"
          className={
            isListening
              ? "mascot-pill-btn mic-active"
              : wakeWord
                ? "mascot-pill-btn wake-armed"
                : "mascot-pill-btn"
          }
          aria-label="Ativar microfone pelo mascote"
          title={
            isListening
              ? "Parar escuta"
              : wakeWord
                ? 'Ativar microfone (Wake Word "Ei, Jarvis" ativo)'
                : "Ativar microfone"
          }
          aria-pressed={isListening}
          disabled={disabledMic}
          onClick={() => {
            lastActivityRef.current = Date.now();
            setMenuOpen(false);
            onToggleMic();
          }}
        >
          <AudioLines />
        </button>
      </div>
    </div>
  );
}

export function MascotWindow() {
  const [mode, setMode] = useState<CoreState>("idle");
  const [speaking, setSpeaking] = useState(false);
  const [theme, setTheme] = useState<VisualTheme>("cyan");
  const [reduced, setReduced] = useState(false);
  const [bubble, setBubble] = useState<string | null>(null);
  const [reaction, setReaction] = useState<"wave" | "cheer" | "alert" | null>(null);
  const [muted, setMuted] = useState(false);
  const [wakeWord, setWakeWord] = useState(false);
  const audioLevelRef = useRef(0.25);

  useEffect(() => {
    document.documentElement.classList.add("mascot-window-root");
    document.body.classList.add("mascot-window-root");

    let unlisten: (() => void) | undefined;
    void onMascotState((st) => {
      setMode(st.mode);
      setSpeaking(st.speaking);
      setTheme(st.theme);
      setReduced(st.reduced);
      if (st.bubble !== undefined) setBubble(st.bubble ?? null);
      if (st.reaction !== undefined) setReaction(st.reaction ?? null);
      if (st.muted !== undefined) setMuted(st.muted);
      if (st.wakeWord !== undefined) setWakeWord(st.wakeWord);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
      document.documentElement.classList.remove("mascot-window-root");
      document.body.classList.remove("mascot-window-root");
    };
  }, []);

  return (
    <main
      className="mascot-window-stage"
      data-theme={theme}
      data-state={mode}
    >
      <MascotBuddy
        mode={mode}
        speaking={speaking}
        reduced={reduced}
        audioLevel={audioLevelRef}
        disabledMic={mode === "thinking"}
        draggableWindow
        externalBubble={bubble}
        externalReaction={reaction}
        muted={muted}
        wakeWord={wakeWord}
        onNewChat={() => void mascotAction("open_chat")}
        onToggleMic={() => void mascotAction("listen")}
        onQuickAction={(action) => void mascotAction(action)}
      />
    </main>
  );
}

/**
 * # Cockpit Principal & Orquestrador de Estado React (`src/main.tsx`)
 *
 * Ponto de entrada da interface do **Jarvis**. Gerencia:
 * - O núcleo 3D (`Stage`) e a barra de comando por voz/texto (`dock`).
 * - A navegação entre as abas centrais (`Núcleo`, `Agentes`, `Navegador`, `Alterações`, `Terminal`, `Rotinas`).
 * - Os listeners de eventos em tempo real vindos do Rust (`jarvis:stream-chunk`, `jarvis:tool-call`,
 *   `jarvis:tool-approval-request`, `jarvis:agent-update`, `jarvis:wake-detected`, `jarvis:companion-sync`).
 * - A alternância entre o aplicativo desktop (Tauri v2) e a landing page web (`LandingShowcase`).
 */
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Aperture,
  ArrowUp,
  Bot,
  ClipboardPaste,
  Clock,
  FileCode2,
  FileText,
  Focus,
  Globe,
  History,
  Maximize2,
  Mic,
  Minimize2,
  Paperclip,
  Plus,
  Radio,
  Shield,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  Smartphone,
  Sparkles,
  Square,
  Terminal,
  Upload,
  Volume2,
  VolumeX,
  X,
  Zap,
} from "lucide-react";
import "@fontsource-variable/space-grotesk";
import Stage, { type VisualTheme } from "./Stage";
import { HudOverlay, VoiceSpectrum } from "./HudOverlay";
import { ResponseCard } from "./ResponseCard";
import { AgentsView, type AgentRun } from "./AgentsView";
import { BrowserView } from "./BrowserView";
import { CommandPalette } from "./CommandPalette";
import { DiffView, type FileDiffRecord } from "./DiffView";
import { TerminalView, type TerminalSession } from "./TerminalView";
import { RoutinesView } from "./RoutinesView";
import { LandingShowcase } from "./LandingShowcase";
import { MascotBuddy, MascotWindow } from "./MascotBuddy";
import { AudioEngine } from "./audio";
import {
  addMemory,
  addProject,
  browserSetVisible,
  cancelAgentTask,
  cancelChat,
  cancelTimer,
  clearApiKey,
  createConversation,
  defaultConfig,
  deleteConversation,
  deleteMemory,
  deleteProject,
  emptyChatGptAuthStatus,
  emptyGoogleAuthStatus,
  getChatgptAuthStatus,
  getConfig,
  getConversation,
  getFullscreen,
  getGoogleAuthStatus,
  getKeyStatus,
  importGoogleOauthJson,
  isTauri,
  listActiveTimers,
  listChatgptModels,
  listConversations,
  listElevenlabsVoices,
  listMemories,
  listProjects,
  logoutChatgpt,
  logoutGoogle,
  onAgentSpawned,
  onAgentCompletedSummary,
  appendAssistantNote,
  runTerminalCommand,
  cancelTerminalCommand,
  onTerminalEvent,
  listSkills,
  saveSkill,
  deleteSkill,
  listRoutines,
  createRoutine,
  toggleRoutine,
  deleteRoutine,
  onRoutineTrigger,
  onCompanionSync,
  onApprovalResolved,
  type SkillItem,
  type RoutineItem,
  onBrowserPanel,
  onChatEvent,
  onFileDiff,
  onMicAutoStop,
  onMicError,
  onMicLevel,
  onScreenPulse,
  onTimerFired,
  onToolApproval,
  onTrayAction,
  onWakeError,
  onWakeLevel,
  onWakeTriggered,
  pickProjectFolder,
  readClipboardContent,
  renameConversation,
  resolveToolApproval,
  revertFileChange,
  saveApiKey,
  saveConfig,
  sendMessage,
  setConversationProject,
  setMascotVisible,
  setNativeWakePaused,
  setNativeWakeWord,
  startAgentTask,
  startChatgptLogin,
  startGoogleLogin,
  startNativeMicRecording,
  stopNativeMicRecording,
  syncMascotState,
  testConnection,
  toggleFullscreen,
  transcribeAudio,
  updateGlobalShortcut,
  type ActiveTimer,
  type AgentRole,
  type AllKeyStatus,
  type AppConfig,
  type ChatGptAuthStatus,
  type ConversationSummary,
  type ElevenLabsVoice,
  type GoogleAuthStatus,
  type KeySlot,
  type MascotActionKind,
  type MemoryItem,
  type PermissionMode,
  type ProjectItem,
  type StoredMessage,
  type ToolApprovalRequest,
  type ToolTrace,
} from "./bridge";
import {
  HistoryDrawer,
  SettingsDrawer,
  type HistoryTab,
  type Quality,
  type SettingsTab,
} from "./Drawers";
import type { CoreState } from "./rhythm";
import {
  audioLevelRef,
  isBrowserSpeechAvailable,
  isHandsFreeStopPhrase,
  listenWithBrowserSpeech,
  MicRecorder,
  SpeechQueue,
  WakeWordMonitor,
} from "./voice";
import "./style.css";

const statusLabel: Record<CoreState, string> = {
  idle: "Em repouso",
  listening: "Ouvindo você",
  thinking: "Processando",
  responding: "Respondendo",
  alert: "Alerta ativo",
};

const permissionLabels: Record<PermissionMode, string> = {
  auto: "Permissão Total",
  review: "Revisão Auto",
  safe: "Só Conversa",
};

function readLs(key: string) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function saveLs(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

function formatRemaining(endsAt: string): string {
  const diff = Math.max(0, Math.ceil((Date.parse(endsAt) - Date.now()) / 1000));
  const mins = Math.floor(diff / 60);
  const secs = diff % 60;
  return `${mins}:${String(secs).padStart(2, "0")}`;
}

function providerLabel(cfg: AppConfig): string {
  switch (cfg.provider) {
    case "gemini":
      return cfg.gemini_auth_mode === "oauth"
        ? `Gemini OAuth · ${cfg.gemini_model}`
        : `Gemini · ${cfg.gemini_model}`;
    case "chatgpt":
      return `ChatGPT OAuth · ${cfg.chatgpt_model}`;
  }
}

interface AttachedImage {
  name: string;
  base64: string;
  mimeType: string;
  previewUrl: string;
}

interface AttachedFile {
  id: string;
  name: string;
  content: string;
  size: number;
}

async function compressImageBlob(blob: Blob, name: string): Promise<AttachedImage> {
  const url = URL.createObjectURL(blob);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error("Não foi possível ler a imagem anexada."));
      el.src = url;
    });
    const maxDim = 1280;
    const scale = Math.min(1, maxDim / Math.max(img.width || 1, img.height || 1));
    const w = Math.max(1, Math.round(img.width * scale));
    const h = Math.max(1, Math.round(img.height * scale));
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Canvas indisponível.");
    ctx.drawImage(img, 0, 0, w, h);
    const dataUrl = canvas.toDataURL("image/jpeg", 0.84);
    const base64 = dataUrl.split(",")[1] ?? "";
    return {
      name,
      base64,
      mimeType: "image/jpeg",
      previewUrl: dataUrl,
    };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function extractMascotBubble(text: string): string | null {
  const clean = text
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/[`*_#>\[\]()]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!clean) return null;
  const firstSentence = clean.split(/(?<=[.!?])\s+/)[0] ?? clean;
  return firstSentence.length > 108
    ? `${firstSentence.slice(0, 105).trimEnd()}…`
    : firstSentence;
}

function App() {
  const [mode, setMode] = useState<CoreState>("idle");
  const [draft, setDraft] = useState("");
  const [drawer, setDrawer] = useState<"none" | "history" | "settings">("none");
  const [historyTab, setHistoryTab] = useState<HistoryTab>("conversations");
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("ai");
  const [immersive, setImmersive] = useState(false);

  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [keyStatus, setKeyStatus] = useState<AllKeyStatus>({
    gemini: { configured: false, preview: null },
    google_client_secret: { configured: false, preview: null },
    elevenlabs: { configured: false, preview: null },
  });
  const [chatgptAuth, setChatgptAuth] = useState<ChatGptAuthStatus>(
    emptyChatGptAuthStatus,
  );
  const [chatgptModels, setChatgptModels] = useState<string[]>([]);
  const [googleAuth, setGoogleAuth] = useState<GoogleAuthStatus>(
    emptyGoogleAuthStatus,
  );
  const [elevenVoices, setElevenVoices] = useState<ElevenLabsVoice[]>([]);

  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [projects, setProjects] = useState<ProjectItem[]>([]);
  const [activeConvId, setActiveConvId] = useState("");
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [messages, setMessages] = useState<StoredMessage[]>([]);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [timers, setTimers] = useState<ActiveTimer[]>([]);
  const [, setTimerTick] = useState(0);

  const [streamingText, setStreamingText] = useState("");
  const [liveTools, setLiveTools] = useState<ToolTrace[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [pendingApproval, setPendingApproval] =
    useState<ToolApprovalRequest | null>(null);
  const [errorBanner, setErrorBanner] = useState<string | null>(null);
  const [noticeDismissed, setNoticeDismissed] = useState(false);

  const [quality, setQuality] = useState<Quality>(() => {
    const v = readLs("quality");
    return v === "Low" || v === "Ultra" ? v : "High";
  });
  const [theme, setTheme] = useState<VisualTheme>(() => {
    const v = readLs("theme");
    return v === "gold" || v === "violet" || v === "crimson" ? v : "cyan";
  });
  const [clockText, setClockText] = useState(() =>
    new Date().toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }),
  );
  const [reduced, setReduced] = useState(() =>
    readLs("reduced") === null
      ? matchMedia("(prefers-reduced-motion: reduce)").matches
      : readLs("reduced") === "true",
  );
  const [showLanding, setShowLanding] = useState(() => {
    if (isTauri) return false;
    if (typeof window === "undefined") return false;
    const params = new URLSearchParams(window.location.search);
    return params.get("app") !== "1";
  });
  const [showMascot, setShowMascot] = useState(
    () => readLs("showMascot") !== "false",
  );
  const [volume, setVolume] = useState(() => {
    const n = Number(readLs("volume") ?? 0.55);
    return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0.55;
  });
  const [muted, setMuted] = useState(() => readLs("muted") === "true");
  const [audioError, setAudioError] = useState(false);

  const [workspaceView, setWorkspaceView] = useState<
    "core" | "agents" | "browser" | "diff" | "terminal" | "routines"
  >("core");
  const [agents, setAgents] = useState<AgentRun[]>([]);
  const [fileDiffs, setFileDiffs] = useState<FileDiffRecord[]>([]);
  const [terminalSessions, setTerminalSessions] = useState<TerminalSession[]>(
    [],
  );
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [routines, setRoutines] = useState<RoutineItem[]>([]);
  const skillsRef = useRef<SkillItem[]>([]);
  skillsRef.current = skills;
  const [browserSessionOpen, setBrowserSessionOpen] = useState(false);
  const [browserUrl, setBrowserUrl] = useState("");
  const [browserTitle, setBrowserTitle] = useState("");
  const [screenPulseKind, setScreenPulseKind] = useState<
    "capture" | "action" | null
  >(null);

  const [attachedImage, setAttachedImage] = useState<AttachedImage | null>(null);
  const [attachedFiles, setAttachedFiles] = useState<AttachedFile[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [mascotBubble, setMascotBubble] = useState<string | null>(null);
  const [mascotReaction, setMascotReaction] = useState<
    "wave" | "cheer" | "alert" | null
  >(null);
  const [wakeMicLevel, setWakeMicLevel] = useState(0);

  const audioRef = useRef<AudioEngine | null>(null);
  const speechRef = useRef<SpeechQueue | null>(null);
  const micRef = useRef<MicRecorder | null>(null);
  const nativeMicRecordingRef = useRef(false);
  const browserSpeechRef = useRef<{ stop: () => void } | null>(null);
  const wakeMonitorRef = useRef<WakeWordMonitor | null>(null);
  const wakeGenRef = useRef(0);
  const handsFreeSessionRef = useRef(false);
  const handsFreeTimerRef = useRef<number>(0);
  const mascotClearTimerRef = useRef<number>(0);
  const listeningStopGuardRef = useRef(false);

  const idleTimerRef = useRef<number>(0);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const modeRef = useRef<CoreState>("idle");
  const activeConvIdRef = useRef("");
  const configRef = useRef<AppConfig>(defaultConfig);
  const keyStatusRef = useRef<AllKeyStatus>(keyStatus);
  const googleAuthRef = useRef<GoogleAuthStatus>(googleAuth);
  const volumeRef = useRef(volume);
  const mutedRef = useRef(muted);
  const themeRef = useRef<VisualTheme>(theme);
  const isStreamingRef = useRef(false);
  const respondingModeRef = useRef<"responding" | "alert">("responding");
  const attachedImageRef = useRef<AttachedImage | null>(null);
  const attachedFilesRef = useRef<AttachedFile[]>([]);

  modeRef.current = mode;
  activeConvIdRef.current = activeConvId;
  configRef.current = config;
  keyStatusRef.current = keyStatus;
  googleAuthRef.current = googleAuth;
  volumeRef.current = volume;
  mutedRef.current = muted;
  themeRef.current = theme;
  isStreamingRef.current = isStreaming;
  attachedImageRef.current = attachedImage;
  attachedFilesRef.current = attachedFiles;

  function notifyMascot(
    bubble: string | null,
    reaction: "wave" | "cheer" | "alert" | null = null,
  ) {
    window.clearTimeout(mascotClearTimerRef.current);
    setMascotBubble(bubble);
    setMascotReaction(reaction);
    if (bubble || reaction) {
      mascotClearTimerRef.current = window.setTimeout(() => {
        setMascotBubble(null);
        setMascotReaction(null);
      }, 6500);
    }
  }

  function scheduleHandsFreeReopen() {
    window.clearTimeout(handsFreeTimerRef.current);
    if (!configRef.current.hands_free_mode || !handsFreeSessionRef.current) return;
    handsFreeTimerRef.current = window.setTimeout(() => {
      if (
        configRef.current.hands_free_mode &&
        handsFreeSessionRef.current &&
        !isStreamingRef.current &&
        !speechRef.current?.isActive() &&
        modeRef.current !== "listening" &&
        modeRef.current !== "thinking"
      ) {
        void toggleListen();
      }
    }, 620);
  }

  function clearIdleTimer() {
    clearTimeout(idleTimerRef.current);
  }

  function scheduleIdle(ms: number) {
    clearIdleTimer();
    idleTimerRef.current = window.setTimeout(() => {
      if (!isStreamingRef.current && !speechRef.current?.isActive()) {
        activate("idle");
      }
    }, ms);
  }

  function activate(next: CoreState, options?: { suppressLoop?: boolean }) {
    clearIdleTimer();
    modeRef.current = next;
    setMode(next);
    audioRef.current ??= new AudioEngine();
    audioRef.current.setLevel(volumeRef.current, mutedRef.current);
    void audioRef.current.cue(next, options).catch(() => setAudioError(true));
  }

  function getSpeechQueue(): SpeechQueue {
    if (!speechRef.current) {
      speechRef.current = new SpeechQueue(
        (playing) => {
          setIsSpeaking(playing);
          if (
            !playing &&
            !isStreamingRef.current &&
            modeRef.current !== "listening" &&
            modeRef.current !== "thinking"
          ) {
            if (configRef.current.hands_free_mode && handsFreeSessionRef.current) {
              scheduleHandsFreeReopen();
            } else {
              scheduleIdle(650);
            }
          }
        },
        (errMsg) => {
          setErrorBanner(`Falha na voz: ${errMsg}`);
        },
      );
    }
    return speechRef.current;
  }

  async function handleAttachFiles(fileList: FileList | File[]) {
    const items = Array.from(fileList);
    if (items.length === 0) return;
    setErrorBanner(null);

    for (const file of items) {
      try {
        if (file.type.startsWith("image/")) {
          const img = await compressImageBlob(file, file.name || "imagem.jpg");
          setAttachedImage(img);
          continue;
        }
        if (file.size > 4_000_000) {
          setErrorBanner(`Arquivo '${file.name}' excede o limite de 4 MB.`);
          continue;
        }
        let content = "";
        if (file.name.toLowerCase().endsWith(".pdf")) {
          const buf = new Uint8Array(await file.arrayBuffer());
          const raw = new TextDecoder("latin1").decode(buf);
          const textMatches = Array.from(raw.matchAll(/\(([^()\\]{3,200})\)/g))
            .map((m) => m[1])
            .filter((s) => /[a-zA-ZÀ-ÿ0-9]{2,}/.test(s))
            .slice(0, 400);
          content = textMatches.length > 0
            ? `[Trechos extraídos do PDF ${file.name}]\n${textMatches.join(" ").slice(0, 18_000)}`
            : `[PDF anexado: ${file.name} (${Math.round(file.size / 1024)} KB) — PDF binário/escaneado sem texto plano extraível]`;
        } else {
          content = (await file.text()).slice(0, 24_000);
        }
        setAttachedFiles((prev) => [
          ...prev.slice(-3),
          {
            id: crypto.randomUUID(),
            name: file.name,
            content,
            size: file.size,
          },
        ]);
      } catch (err) {
        setErrorBanner(err instanceof Error ? err.message : `Erro ao anexar ${file.name}`);
      }
    }
  }

  async function handlePasteClipboard() {
    setErrorBanner(null);
    try {
      const clip = await readClipboardContent();
      if (clip.kind === "image" && clip.imageBase64 && clip.imageMime) {
        setAttachedImage({
          name: "Print do Ctrl+C",
          base64: clip.imageBase64,
          mimeType: clip.imageMime,
          previewUrl: `data:${clip.imageMime};base64,${clip.imageBase64}`,
        });
        notifyMascot("Imagem do Ctrl+C anexada!", "cheer");
      } else if (clip.kind === "text" && clip.text) {
        const clean = clip.text.slice(0, 24_000);
        setAttachedFiles((prev) => [
          ...prev.slice(-3),
          {
            id: crypto.randomUUID(),
            name: "Texto do Ctrl+C",
            content: clean,
            size: clean.length,
          },
        ]);
        notifyMascot("Texto do Ctrl+C anexado!", "cheer");
      } else {
        setErrorBanner("A área de transferência (Ctrl+C) está vazia.");
      }
    } catch (err) {
      setErrorBanner(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleRevertDiff(diff: FileDiffRecord) {
    try {
      const msg = await revertFileChange(diff.path, diff.existedBefore, diff.oldContent);
      setFileDiffs((prev) =>
        prev.map((d) => (d.id === diff.id ? { ...d, reverted: true } : d)),
      );
      notifyMascot(msg, "cheer");
    } catch (err) {
      setErrorBanner(err instanceof Error ? err.message : String(err));
    }
  }

  function cycleTheme() {
    const order: VisualTheme[] = ["cyan", "gold", "violet", "crimson"];
    const next = order[(order.indexOf(themeRef.current) + 1) % order.length];
    saveLs("theme", next);
    setTheme(next);
    themeRef.current = next;
  }

  function handleMascotQuickAction(action: MascotActionKind) {
    if (action === "listen") {
      void toggleListen();
    } else if (action === "open_chat") {
      setWorkspaceView("core");
      window.setTimeout(() => inputRef.current?.focus(), 60);
    } else if (action === "new_chat") {
      setWorkspaceView("core");
      void startNewConversation();
    } else if (action === "see_screen") {
      void submitText("Olhe minha tela atual e me diga brevemente o que está vendo.", false);
    } else if (action === "toggle_mute") {
      const next = !mutedRef.current;
      saveLs("muted", String(next));
      setMuted(next);
      mutedRef.current = next;
      if (next) speechRef.current?.stop();
    } else if (action === "toggle_wake") {
      const nextWake = !configRef.current.wake_word_enabled;
      void updateAppConfig({ wake_word_enabled: nextWake });
      notifyMascot(
        nextWake ? 'Wake Word "Ei, Jarvis" ativado!' : "Wake Word desativado.",
        nextWake ? "cheer" : null,
      );
    } else if (action === "cycle_theme") {
      cycleTheme();
    } else if (action === "hide") {
      saveLs("showMascot", "false");
      setShowMascot(false);
    }
  }

  async function refreshLists() {
    try {
      const [convs, projs, mems, activeTimers] = await Promise.all([
        listConversations(),
        listProjects(),
        listMemories(),
        listActiveTimers(),
      ]);
      setConversations(convs);
      setProjects(projs);
      setMemories(mems);
      setTimers(activeTimers);
    } catch {
      /* ignore */
    }
  }

  async function loadConversation(id: string) {
    const conv = await getConversation(id);
    setActiveConvId(conv.id);
    activeConvIdRef.current = conv.id;
    setActiveProjectId(conv.project_id ?? null);
    setMessages(conv.messages);
    setStreamingText("");
    setLiveTools([]);
  }

  async function startNewConversation(projectId?: string | null) {
    if (isStreamingRef.current) await stopAll();
    const conv = await createConversation(undefined, projectId ?? null);
    setActiveConvId(conv.id);
    activeConvIdRef.current = conv.id;
    setActiveProjectId(conv.project_id ?? null);
    setMessages([]);
    setStreamingText("");
    setLiveTools([]);
    setAttachedImage(null);
    setAttachedFiles([]);
    setErrorBanner(null);
    await refreshLists();
    inputRef.current?.focus();
  }

  async function handleToggleFullscreen() {
    try {
      const fs = await toggleFullscreen();
      setIsFullscreen(fs);
    } catch {
      /* ignore */
    }
  }

  async function stopAll() {
    clearIdleTimer();
    window.clearTimeout(handsFreeTimerRef.current);
    handsFreeSessionRef.current = false;
    if (nativeMicRecordingRef.current) {
      nativeMicRecordingRef.current = false;
      void stopNativeMicRecording().catch(() => {});
    }
    isStreamingRef.current = false;
    setIsStreaming(false);
    setPendingApproval(null);
    getSpeechQueue().stop();
    await cancelChat().catch(() => {});
    setStreamingText("");
    setLiveTools([]);
    activate("idle");
  }

  async function submitText(
    rawText?: string,
    fromMic = false,
    allowFromThinking = false,
  ) {
    const baseText = (rawText ?? draft).trim();
    const pendingImg = rawText === undefined ? attachedImageRef.current : null;
    const pendingFiles = rawText === undefined ? attachedFilesRef.current : [];

    if (!baseText && !pendingImg && pendingFiles.length === 0) return;
    if (
      isStreamingRef.current ||
      (!allowFromThinking && modeRef.current === "thinking")
    ) {
      return;
    }

    if (fromMic && isHandsFreeStopPhrase(baseText)) {
      handsFreeSessionRef.current = false;
      window.clearTimeout(handsFreeTimerRef.current);
      notifyMascot("Descansando, senhor.", "wave");
      activate("idle");
      return;
    }

    if (fromMic && configRef.current.hands_free_mode) {
      handsFreeSessionRef.current = true;
    }

    let finalText = baseText || "Analise o anexo enviado.";
    if (baseText.startsWith("/")) {
      const [rawCmd, ...rest] = baseText.slice(1).trim().split(/\s+/);
      const slug = rawCmd?.toLowerCase();
      const matched = skillsRef.current.find((s) => s.slug === slug);
      if (matched) {
        const extra = rest.join(" ").trim();
        finalText = extra
          ? `${matched.prompt}\n\nContexto adicional do usuário: ${extra}`
          : matched.prompt;
        notifyMascot(`Executando skill /${matched.slug}...`, "cheer");
      }
    }
    if (pendingFiles.length > 0) {
      const filesBlock = pendingFiles
        .map((f) => `\n\n--- Anexo: ${f.name} ---\n${f.content}`)
        .join("");
      finalText += filesBlock;
    }

    let convId = activeConvIdRef.current;
    if (!convId) {
      const conv = await createConversation();
      convId = conv.id;
      setActiveConvId(convId);
      activeConvIdRef.current = convId;
    }

    if (rawText === undefined) {
      setDraft("");
      setAttachedImage(null);
      setAttachedFiles([]);
    }
    setErrorBanner(null);
    setStreamingText("");
    setLiveTools([]);
    setPendingApproval(null);
    respondingModeRef.current = "responding";

    const speech = getSpeechQueue();
    const replyMode = configRef.current.tts_reply_mode ?? "brief";
    const allowThisTurn = replyMode !== "miconly" || fromMic;
    const ttsOn =
      isTauri &&
      configRef.current.tts_engine !== "off" &&
      keyStatusRef.current.elevenlabs.configured &&
      !mutedRef.current &&
      allowThisTurn;
    speech.setBriefOnly(replyMode !== "full");
    speech.setEnabled(ttsOn);
    speech.setLevel(volumeRef.current, mutedRef.current);
    speech.reset();
    if (ttsOn) {
      speech.unlock();
    }

    isStreamingRef.current = true;
    setIsStreaming(true);
    activate("thinking");

    try {
      const imgPayload = pendingImg
        ? { base64: pendingImg.base64, mimeType: pendingImg.mimeType }
        : null;
      const updated = await sendMessage(convId, finalText, imgPayload);
      setMessages(updated.messages);
      void refreshLists();
    } catch (err) {
      isStreamingRef.current = false;
      setIsStreaming(false);
      setErrorBanner(err instanceof Error ? err.message : String(err));
      activate("idle");
    }
    if (workspaceView === "core") {
      inputRef.current?.focus();
    }
  }

  async function handleLaunchAgent(role: AgentRole, name: string, task: string) {
    setErrorBanner(null);
    try {
      const agentId = await startAgentTask(role, task);
      setAgents((prev) => {
        if (prev.some((a) => a.id === agentId)) return prev;
        return [
          {
            id: agentId,
            role,
            name,
            task,
            status: "running",
            output: "",
            startedAt: Date.now(),
            traces: [],
          },
          ...prev,
        ];
      });
    } catch (err) {
      setErrorBanner(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleCancelAgent(id: string) {
    await cancelAgentTask(id).catch(() => {});
    setAgents((prev) =>
      prev.map((a) =>
        a.id === id && a.status === "running"
          ? { ...a, status: "cancelled", finishedAt: Date.now() }
          : a,
      ),
    );
  }

  function handleRemoveAgent(id: string) {
    setAgents((prev) => {
      const next = prev.filter((a) => a.id !== id);
      if (next.length === 0) {
        setWorkspaceView((cur) => (cur === "agents" ? "core" : cur));
      }
      return next;
    });
  }

  function handleSendAgentToCore(agent: AgentRun) {
    setWorkspaceView("core");
    const excerpt = agent.output.slice(0, 2200);
    void submitText(
      `Resumo produzido pelo agente ${agent.name} para a missão "${agent.task}":\n\n${excerpt}\n\nConsolide o ponto principal em uma resposta curta e diga o melhor próximo passo.`,
      false,
    );
  }

  function handleCloseBrowserPanel() {
    void browserSetVisible(false).catch(() => {});
    setBrowserSessionOpen(false);
    setWorkspaceView((cur) => (cur === "browser" ? "core" : cur));
  }

  async function finishMicRecording() {
    if (listeningStopGuardRef.current) return;
    listeningStopGuardRef.current = true;

    const useNativeMic = nativeMicRecordingRef.current;
    nativeMicRecordingRef.current = false;
    const recorder = micRef.current;
    micRef.current = null;
    if (!useNativeMic && !recorder) {
      listeningStopGuardRef.current = false;
      return;
    }

    activate("thinking");
    try {
      const { audioBase64, mimeType, hadAudio } = useNativeMic
        ? await stopNativeMicRecording()
        : await recorder!.stop();
      if (!hadAudio) {
        activate("idle");
        listeningStopGuardRef.current = false;
        return;
      }
      const transcript = await transcribeAudio(audioBase64, mimeType);
      const clean = transcript.trim();
      if (clean) {
        listeningStopGuardRef.current = false;
        notifyMascot(`Você: "${clean}"`);
        await submitText(clean, true, true);
        return;
      }
      activate("idle");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("NO_STT_PROVIDER")) {
        setErrorBanner(
          "Configure sua chave da ElevenLabs (Ajustes → Voz) ou Gemini (Ajustes → IA) para usar o microfone.",
        );
      } else {
        setErrorBanner(msg);
      }
      activate("idle");
    } finally {
      listeningStopGuardRef.current = false;
    }
  }

  async function toggleListen() {
    if (isStreamingRef.current || modeRef.current === "thinking") return;

    if (modeRef.current === "listening") {
      if (browserSpeechRef.current) {
        browserSpeechRef.current.stop();
        browserSpeechRef.current = null;
      } else {
        await finishMicRecording();
      }
      return;
    }

    setErrorBanner(null);
    getSpeechQueue().stop();

    const cfg = configRef.current;
    const ks = keyStatusRef.current;
    const hasCloudStt =
      isTauri &&
      (ks.gemini.configured ||
        googleAuthRef.current.connected ||
        ks.elevenlabs.configured);
    const useBrowserSpeech =
      cfg.stt_engine === "browser" || (!hasCloudStt && isBrowserSpeechAvailable());

    if (useBrowserSpeech) {
      if (!isBrowserSpeechAvailable()) {
        setErrorBanner(
          "Reconhecimento do navegador indisponível. Configure sua chave ElevenLabs ou Gemini em Ajustes.",
        );
        return;
      }
      activate("listening");
      const session = listenWithBrowserSpeech();
      browserSpeechRef.current = session;
      try {
        const text = await session.promise;
        browserSpeechRef.current = null;
        if (text) {
          await submitText(text, true);
        } else {
          activate("idle");
        }
      } catch (err) {
        browserSpeechRef.current = null;
        setErrorBanner(err instanceof Error ? err.message : String(err));
        activate("idle");
      }
      return;
    }

    if (!hasCloudStt) {
      setErrorBanner(
        "Configure sua chave da ElevenLabs (Ajustes → Voz) ou Gemini (Ajustes → IA) para usar o microfone.",
      );
      return;
    }

    listeningStopGuardRef.current = false;
    activate("listening");

    if (isTauri) {
      try {
        nativeMicRecordingRef.current = true;
        await startNativeMicRecording(cfg.auto_listen_vad, cfg.mic_device_id);
      } catch (err) {
        nativeMicRecordingRef.current = false;
        setErrorBanner(
          err instanceof Error
            ? `Microfone indisponível: ${err.message}`
            : "Microfone indisponível.",
        );
        activate("idle");
      }
      return;
    }

    const recorder = new MicRecorder();
    micRef.current = recorder;

    try {
      await recorder.start(
        cfg.auto_listen_vad,
        () => {
          void finishMicRecording();
        },
        cfg.mic_device_id,
      );
    } catch (err) {
      micRef.current = null;
      setErrorBanner(
        err instanceof Error
          ? `Microfone indisponível: ${err.message}`
          : "Microfone indisponível.",
      );
      activate("idle");
    }
  }

  async function updateAppConfig(patch: Partial<AppConfig>) {
    const next = { ...configRef.current, ...patch };
    setConfig(next);
    configRef.current = next;
    try {
      const saved = await saveConfig(next);
      setConfig(saved);
      configRef.current = saved;
    } catch (err) {
      setErrorBanner(err instanceof Error ? err.message : String(err));
    }
  }

  async function cyclePermissionMode() {
    const order: PermissionMode[] = ["auto", "review", "safe"];
    const idx = order.indexOf(config.permission_mode);
    const next = order[(idx + 1) % order.length];
    await updateAppConfig({ permission_mode: next });
  }

  async function handleApproval(approved: boolean, switchToAuto = false) {
    const req = pendingApproval;
    if (!req) return;
    setPendingApproval(null);
    if (switchToAuto) {
      await updateAppConfig({ permission_mode: "auto" });
    }
    await resolveToolApproval(req.approvalId, approved);
  }

  async function handleTestVoice() {
    if (mutedRef.current) {
      setErrorBanner(
        "O som do J.A.R.V.I.S. está silenciado. Clique no ícone de alto-falante no topo para ativar.",
      );
      return;
    }
    setErrorBanner(null);
    const speech = getSpeechQueue();
    speech.setBriefOnly(true);
    speech.setEnabled(true);
    speech.setLevel(volumeRef.current, false);
    speech.unlock();
    speech.reset();
    activate("responding", { suppressLoop: true });
    speech.pushDelta("Sistemas do J.A.R.V.I.S. prontos para operar.");
    speech.finish();
  }

  function handleReplayText(text: string) {
    if (!text.trim() || mutedRef.current) return;
    setErrorBanner(null);
    const speech = getSpeechQueue();
    speech.setBriefOnly(configRef.current.tts_reply_mode !== "full");
    speech.setEnabled(true);
    speech.setLevel(volumeRef.current, false);
    speech.unlock();
    speech.reset();
    activate("responding", { suppressLoop: true });
    speech.pushDelta(text);
    speech.finish();
  }

  useEffect(() => {
    let active = true;
    async function init() {
      try {
        const [
          cfg,
          keys,
          oauthStatus,
          gAuthStatus,
          convs,
          projs,
          mems,
          activeTimers,
          fsNow,
        ] = await Promise.all([
          getConfig(),
          getKeyStatus(),
          getChatgptAuthStatus().catch(() => emptyChatGptAuthStatus),
          getGoogleAuthStatus().catch(() => emptyGoogleAuthStatus),
          listConversations(),
          listProjects().catch(() => []),
          listMemories(),
          listActiveTimers(),
          getFullscreen().catch(() => false),
        ]);
        if (!active) return;
        setConfig(cfg);
        configRef.current = cfg;
        setKeyStatus(keys);
        keyStatusRef.current = keys;
        setChatgptAuth(oauthStatus);
        setGoogleAuth(gAuthStatus);
        googleAuthRef.current = gAuthStatus;
        setProjects(projs);
        setIsFullscreen(fsNow);

        if (oauthStatus.connected) {
          void listChatgptModels()
            .then((m) => {
              if (active) setChatgptModels(m);
            })
            .catch(() => {});
        }
        if (keys.elevenlabs.configured) {
          void listElevenlabsVoices()
            .then((v) => {
              if (active) setElevenVoices(v);
            })
            .catch(() => {});
        }

        setConversations(convs);
        setMemories(mems);
        setTimers(activeTimers);

        if (convs.length > 0) {
          const loaded = await getConversation(convs[0].id);
          if (active) {
            setActiveConvId(loaded.id);
            activeConvIdRef.current = loaded.id;
            setMessages(loaded.messages);
          }
        } else {
          const first = await createConversation();
          if (active) {
            setActiveConvId(first.id);
            activeConvIdRef.current = first.id;
            setMessages([]);
            setConversations([
              {
                id: first.id,
                title: first.title,
                updated_at: first.updated_at,
                message_count: 0,
                preview: "",
              },
            ]);
          }
        }
      } catch (err) {
        if (active) {
          setErrorBanner(err instanceof Error ? err.message : String(err));
        }
      }
    }
    void init();
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    audioRef.current?.setLevel(volume, muted);
    speechRef.current?.setLevel(volume, muted);
  }, [volume, muted]);

  useEffect(() => {
    if (workspaceView !== "browser" || drawer !== "none" || immersive) {
      void browserSetVisible(false).catch(() => {});
    }
  }, [workspaceView, drawer, immersive]);

  useEffect(() => {
    if (!isTauri) return;
    void setMascotVisible(showMascot).catch(() => {});
  }, [showMascot]);

  useEffect(() => {
    if (!isTauri || !showMascot) return;
    void syncMascotState({
      mode,
      speaking: isSpeaking,
      theme,
      reduced,
      bubble: mascotBubble,
      reaction: mascotReaction,
      muted,
      wakeWord: config.wake_word_enabled,
    }).catch(() => {});
  }, [
    showMascot,
    mode,
    isSpeaking,
    theme,
    reduced,
    mascotBubble,
    mascotReaction,
    muted,
    config.wake_word_enabled,
  ]);

  useEffect(() => {
    if (!config.wake_word_enabled) {
      wakeMonitorRef.current?.stop();
      wakeMonitorRef.current = null;
      if (isTauri) void setNativeWakeWord(false).catch(() => {});
      setWakeMicLevel(0);
      return;
    }

    if (isTauri) {
      const gen = ++wakeGenRef.current;
      void setNativeWakeWord(true, config.mic_device_id).catch((err) => {
        if (wakeGenRef.current === gen) {
          setErrorBanner(err instanceof Error ? err.message : String(err));
        }
      });
      return () => {
        const offGen = ++wakeGenRef.current;
        void setNativeWakeWord(false)
          .catch(() => {})
          .finally(() => {
            if (wakeGenRef.current === offGen) {
              setWakeMicLevel(0);
            }
          });
      };
    }

    const monitor = new WakeWordMonitor({
      useCloudVad: false,
      deviceId: config.mic_device_id,
      onWake: (inlineCommand) => {
        if (isStreamingRef.current || modeRef.current === "thinking") return;
        notifyMascot(
          inlineCommand ? `Ouvido: "${inlineCommand}"` : "Estou ouvindo, senhor!",
          "cheer",
        );
        if (inlineCommand) {
          void submitText(inlineCommand, true);
        } else if (modeRef.current !== "listening") {
          void toggleListen();
        }
      },
      onError: (err) => {
        setErrorBanner(err);
      },
      onLevel: (lvl) => {
        setWakeMicLevel((prev) => prev * 0.55 + lvl * 0.45);
      },
    });
    wakeMonitorRef.current = monitor;
    monitor.start();
    return () => {
      monitor.stop();
      setWakeMicLevel(0);
      if (wakeMonitorRef.current === monitor) {
        wakeMonitorRef.current = null;
      }
    };
  }, [config.wake_word_enabled, config.mic_device_id]);

  useEffect(() => {
    const busyVoice = mode !== "idle" || isStreaming || isSpeaking;
    wakeMonitorRef.current?.setPaused(busyVoice);
    if (isTauri) {
      void setNativeWakePaused(busyVoice).catch(() => {});
    }
  }, [mode, isStreaming, isSpeaking]);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    const unsubs: Array<() => void> = [];
    const track = (p: Promise<() => void>) => {
      void p.then((fn) => {
        if (disposed) {
          fn();
        } else {
          unsubs.push(fn);
        }
      });
    };

    track(
      onMicLevel((lvl) => {
        audioLevelRef.current = audioLevelRef.current * 0.4 + lvl * 0.6;
      }),
    );

    track(
      onMicAutoStop(() => {
        if (nativeMicRecordingRef.current) {
          void finishMicRecording();
        }
      }),
    );

    track(
      onMicError((err) => {
        nativeMicRecordingRef.current = false;
        setErrorBanner(err);
        activate("idle");
      }),
    );

    track(
      onWakeLevel((lvl) => {
        setWakeMicLevel((prev) => prev * 0.5 + lvl * 0.5);
      }),
    );

    track(
      onWakeTriggered(({ command }) => {
        if (isStreamingRef.current || modeRef.current === "thinking") return;
        notifyMascot(
          command ? `Ouvido: "${command}"` : "Estou ouvindo, senhor!",
          "cheer",
        );
        if (command) {
          void submitText(command, true);
        } else if (modeRef.current !== "listening") {
          void toggleListen();
        }
      }),
    );

    track(
      onWakeError((err) => {
        setErrorBanner(err);
      }),
    );

    return () => {
      disposed = true;
      for (const fn of unsubs) fn();
    };
  }, []);

  useEffect(() => {
    const id = window.setInterval(() => {
      setClockText(
        new Date().toLocaleTimeString("pt-BR", {
          hour: "2-digit",
          minute: "2-digit",
        }),
      );
    }, 15000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (timers.length === 0) return;
    const id = window.setInterval(() => setTimerTick((v) => v + 1), 1000);
    return () => clearInterval(id);
  }, [timers.length]);

  useEffect(() => {
    let disposed = false;
    const unsubs: Array<() => void> = [];
    const safeBind = (p: Promise<() => void>) => {
      void p.then((fn) => {
        if (disposed) {
          fn();
        } else {
          unsubs.push(fn);
        }
      });
    };

    safeBind(
      onAgentCompletedSummary((ev) => {
      notifyMascot(ev.spokenSummary, "cheer");
      const summaryText = `**Resumo do agente ${ev.agentName}:**\n\n${ev.cardSummary}`;
      setMessages((prev) => [
        ...prev,
        {
          id: `agent-summary-${ev.agentId}`,
          role: "assistant",
          text: summaryText,
          timestamp: new Date().toISOString(),
          mode: "responding",
          tool_events: [],
        } as (typeof messages)[number],
      ]);
      if (activeConvIdRef.current) {
        void appendAssistantNote(activeConvIdRef.current, summaryText)
          .then(() => refreshLists())
          .catch(() => {});
      }
      if (
        isTauri &&
        configRef.current.tts_engine !== "off" &&
        keyStatusRef.current.elevenlabs.configured &&
        !mutedRef.current &&
        !isStreamingRef.current
      ) {
        const speech = getSpeechQueue();
        speech.setEnabled(true);
        speech.setLevel(volumeRef.current, mutedRef.current);
        speech.reset();
        speech.pushDelta(ev.spokenSummary);
        speech.finish();
      }
      }),
    );
    let pulseTimer = 0;

    void listSkills().then(setSkills).catch(() => {});
    void listRoutines().then(setRoutines).catch(() => {});

    safeBind(
      onChatEvent((ev) => {
      if (ev.conversationId.startsWith("agent:")) {
        setAgents((prev) =>
          prev.map((a) => {
            if (a.id !== ev.conversationId) return a;
            if (ev.type === "delta") {
              return { ...a, output: a.output + ev.text };
            }
            if (ev.type === "tool_start") {
              const item: ToolTrace = {
                id: ev.id,
                name: ev.name,
                title: ev.title,
                status: "running",
                summary: ev.summary,
              };
              const idx = a.traces.findIndex((t) => t.id === ev.id);
              const traces =
                idx >= 0
                  ? a.traces.map((t, i) => (i === idx ? item : t))
                  : [...a.traces, item];
              return { ...a, traces };
            }
            if (ev.type === "tool_end") {
              return {
                ...a,
                traces: a.traces.map((t) =>
                  t.id === ev.id
                    ? { ...t, title: ev.title, status: ev.status, summary: ev.summary }
                    : t,
                ),
              };
            }
            if (ev.type === "done") {
              notifyMascot(`Agente ${a.name} concluiu a missão!`, "cheer");
              return {
                ...a,
                status: "done",
                output: ev.message.text || a.output,
                traces: ev.message.tool_events.length ? ev.message.tool_events : a.traces,
                finishedAt: Date.now(),
              };
            }
            if (ev.type === "cancelled") {
              return { ...a, status: "cancelled", finishedAt: Date.now() };
            }
            if (ev.type === "error") {
              notifyMascot(`Erro no agente ${a.name}.`, "alert");
              return {
                ...a,
                status: "error",
                error: ev.error,
                finishedAt: Date.now(),
              };
            }
            return a;
          }),
        );
        return;
      }

      if (ev.conversationId !== activeConvIdRef.current) return;

      if (ev.type === "tool_start") {
        setLiveTools((prev) => {
          const idx = prev.findIndex((t) => t.id === ev.id);
          const item: ToolTrace = {
            id: ev.id,
            name: ev.name,
            title: ev.title,
            status: "running",
            summary: ev.summary,
          };
          if (idx >= 0) {
            const copy = [...prev];
            copy[idx] = item;
            return copy;
          }
          return [...prev, item];
        });
      } else if (ev.type === "tool_end") {
        setLiveTools((prev) =>
          prev.map((t) =>
            t.id === ev.id
              ? {
                  ...t,
                  title: ev.title,
                  status: ev.status,
                  summary: ev.summary,
                }
              : t,
          ),
        );
      } else if (ev.type === "mode") {
        if (ev.mode === "alert") {
          respondingModeRef.current = "alert";
          if (modeRef.current === "responding") {
            activate("alert");
          }
        }
      } else if (ev.type === "model_updated") {
        if (ev.provider === "chatgpt") {
          setConfig((prev) => {
            const next = { ...prev, chatgpt_model: ev.model };
            configRef.current = next;
            return next;
          });
        }
      } else if (ev.type === "delta") {
        const speech = getSpeechQueue();
        const ttsOn = speech.isEnabled();
        if (modeRef.current === "thinking") {
          activate(respondingModeRef.current, { suppressLoop: ttsOn });
        }
        setStreamingText((prev) => prev + ev.text);
        if (ttsOn) {
          speech.pushDelta(ev.text);
        }
      } else if (ev.type === "done") {
        isStreamingRef.current = false;
        setIsStreaming(false);
        setPendingApproval(null);
        setStreamingText("");
        setLiveTools([]);
        setMessages((prev) => [...prev, ev.message]);
        void refreshLists();

        const targetMode: CoreState =
          ev.message.mode === "alert" || respondingModeRef.current === "alert"
            ? "alert"
            : "responding";

        const snippet = extractMascotBubble(ev.message.text);
        if (snippet) {
          notifyMascot(snippet, targetMode === "alert" ? "alert" : null);
        }

        const speech = getSpeechQueue();
        const ttsOn = speech.isEnabled();
        if (modeRef.current === "thinking") {
          activate(targetMode, { suppressLoop: ttsOn });
        }

        speech.finish();
        if (!speech.isActive()) {
          if (configRef.current.hands_free_mode && handsFreeSessionRef.current) {
            scheduleHandsFreeReopen();
          } else {
            scheduleIdle(targetMode === "alert" ? 4200 : 3400);
          }
        }
      } else if (ev.type === "cancelled") {
        isStreamingRef.current = false;
        setIsStreaming(false);
        setPendingApproval(null);
        setStreamingText("");
        setLiveTools([]);
        getSpeechQueue().stop();
        activate("idle");
      } else if (ev.type === "error") {
        isStreamingRef.current = false;
        setIsStreaming(false);
        setPendingApproval(null);
        setStreamingText("");
        setLiveTools([]);
        getSpeechQueue().stop();
        setErrorBanner(ev.error);
        notifyMascot("Ocorreu um erro, senhor.", "alert");
        activate("idle");
      }
      }),
    );

    safeBind(
      onToolApproval((req) => {
        setPendingApproval(req);
      }),
    );

    safeBind(
      onTimerFired((ev) => {
      void refreshLists();
      activate("alert", { suppressLoop: false });
      setErrorBanner(`⏰ Timer concluído: ${ev.label}`);
      notifyMascot(`⏰ Timer concluído: ${ev.label}`, "alert");
      if (
        isTauri &&
        configRef.current.tts_engine !== "off" &&
        keyStatusRef.current.elevenlabs.configured &&
        !mutedRef.current
      ) {
        const speech = getSpeechQueue();
        speech.setEnabled(true);
        speech.setLevel(volumeRef.current, mutedRef.current);
        speech.reset();
        speech.pushDelta(`Timer concluído: ${ev.label}.`);
        speech.finish();
      } else {
        scheduleIdle(4500);
      }
      }),
    );

    safeBind(
      onTrayAction((ev) => {
      if (ev.action === "listen") {
        void toggleListen();
      } else if (ev.action === "new_chat") {
        setWorkspaceView("core");
        void startNewConversation();
      } else if (ev.action === "focus_chat") {
        setWorkspaceView("core");
        window.setTimeout(() => inputRef.current?.focus(), 60);
      } else if (ev.action === "see_screen") {
        handleMascotQuickAction("see_screen");
      } else if (ev.action === "toggle_mute") {
        handleMascotQuickAction("toggle_mute");
      } else if (ev.action === "toggle_wake") {
        handleMascotQuickAction("toggle_wake");
      } else if (ev.action === "cycle_theme") {
        handleMascotQuickAction("cycle_theme");
      } else if (ev.action === "hide_mascot") {
        handleMascotQuickAction("hide");
      }
      }),
    );

    safeBind(
      onAgentSpawned((ev) => {
        setAgents((prev) => {
          if (prev.some((a) => a.id === ev.id)) return prev;
          return [
            {
              id: ev.id,
              role: ev.role,
              name: ev.name,
              task: ev.task,
              status: "running",
              output: "",
              startedAt: Date.now(),
              traces: [],
            },
            ...prev,
          ];
        });
      }),
    );

    safeBind(
      onBrowserPanel((ev) => {
        if (ev.open) {
          if (ev.url) setBrowserUrl(ev.url);
          if (ev.title) setBrowserTitle(ev.title);
          setBrowserSessionOpen(true);
          setWorkspaceView("browser");
        } else {
          setBrowserSessionOpen(false);
          setWorkspaceView((cur) => (cur === "browser" ? "core" : cur));
        }
      }),
    );

    safeBind(
      onScreenPulse((ev) => {
        setScreenPulseKind(ev.kind);
        window.clearTimeout(pulseTimer);
        pulseTimer = window.setTimeout(() => setScreenPulseKind(null), 1300);
      }),
    );

    safeBind(
      onFileDiff((ev) => {
        setFileDiffs((prev) => [ev, ...prev].slice(0, 40));
        notifyMascot(`Arquivo atualizado: ${ev.fileName}`, "cheer");
      }),
    );

    safeBind(
      onTerminalEvent((ev) => {
        setTerminalSessions((prev) => {
          if (ev.kind === "start") {
            const next: TerminalSession = {
              id: ev.id,
              command: ev.command ?? "",
              cwd: ev.cwd ?? "",
              running: true,
              lines: [],
            };
            return [...prev.slice(-19), next];
          }
          return prev.map((s) => {
            if (s.id !== ev.id) return s;
            if (ev.kind === "stdout" || ev.kind === "stderr") {
              return {
                ...s,
                lines: [
                  ...s.lines.slice(-400),
                  { kind: ev.kind, text: ev.line ?? "" },
                ],
              };
            }
            if (ev.kind === "exit") {
              return {
                ...s,
                running: false,
                exitCode: ev.exitCode,
                cancelled: ev.cancelled,
              };
            }
            return s;
          });
        });
      }),
    );

    safeBind(
      onRoutineTrigger((ev) => {
        notifyMascot(`⏰ Rotina automática: ${ev.name}`, "cheer");
        void listRoutines().then(setRoutines).catch(() => {});
        if (!isStreamingRef.current) {
          void submitText(ev.prompt, false);
        }
      }),
    );

    safeBind(
      onCompanionSync((ev) => {
        const targetId = ev.conversationId;
        if (targetId && targetId !== activeConvIdRef.current) {
          setActiveConvId(targetId);
          activeConvIdRef.current = targetId;
        }
        if (targetId) {
          void getConversation(targetId)
            .then((conv) => {
              setMessages(conv.messages);
              setActiveProjectId(conv.project_id ?? null);
            })
            .catch(() => {});
        }
        void refreshLists();
      }),
    );

    safeBind(
      onApprovalResolved(() => {
        setPendingApproval(null);
      }),
    );

    return () => {
      disposed = true;
      for (const fn of unsubs) fn();
      window.clearTimeout(pulseTimer);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
        return;
      }
      if (e.key === "Escape") {
        setPaletteOpen(false);
        setDrawer("none");
        setImmersive(false);
      }
      if (e.key === "F11" || (e.altKey && e.key === "Enter")) {
        e.preventDefault();
        void handleToggleFullscreen();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(
    () => () => {
      clearIdleTimer();
      window.clearTimeout(handsFreeTimerRef.current);
      window.clearTimeout(mascotClearTimerRef.current);
      wakeMonitorRef.current?.stop();
      void micRef.current?.stop();
      audioRef.current?.dispose();
      speechRef.current?.dispose();
    },
    [],
  );

  const isProviderConfigured =
    (config.provider === "gemini" &&
      (config.gemini_auth_mode === "oauth"
        ? googleAuth.connected
        : keyStatus.gemini.configured)) ||
    (config.provider === "chatgpt" && chatgptAuth.connected);

  const lastAssistant = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  const subtitleText = streamingText || lastAssistant?.text || "";
  const activeTool = liveTools.find((t) => t.status === "running");
  const visibleTraces =
    liveTools.length > 0 ? liveTools : (lastAssistant?.tool_events ?? []);
  const busy = isStreaming || mode === "thinking" || isSpeaking;

  const canReplayVoice =
    isTauri &&
    config.tts_engine !== "off" &&
    keyStatus.elevenlabs.configured &&
    !muted;

  const quickPrompts = [
    "Que horas são?",
    "Notícias de hoje",
    "Abrir calculadora",
    "Timer de 5 minutos",
  ];

  if (!isTauri && showLanding) {
    return (
      <LandingShowcase
        theme={theme}
        onThemeChange={(t) => {
          saveLs("theme", t);
          setTheme(t);
          themeRef.current = t;
        }}
        onEnterSimulator={() => {
          try {
            window.history.replaceState(null, "", "?app=1");
          } catch {
            /* ignore */
          }
          setShowLanding(false);
        }}
      />
    );
  }

  return (
    <main
      data-state={mode}
      data-theme={theme}
      className={`${immersive ? "immersive" : ""} ${!isTauri ? "web-preview" : ""}`}
      onDragOver={(e) => {
        if (e.dataTransfer?.types?.includes("Files")) {
          e.preventDefault();
          setIsDragOver(true);
        }
      }}
      onDragLeave={(e) => {
        if (e.currentTarget === e.target) {
          setIsDragOver(false);
        }
      }}
      onDrop={(e) => {
        e.preventDefault();
        setIsDragOver(false);
        if (e.dataTransfer?.files?.length) {
          void handleAttachFiles(e.dataTransfer.files);
        }
      }}
    >
      {isDragOver && (
        <div className="drop-overlay" aria-hidden="true">
          <div className="drop-overlay-card">
            <Upload />
            <strong>Solte aqui para enviar ao J.A.R.V.I.S.</strong>
            <span>Imagens, prints, código, JSON, TXT ou PDF</span>
          </div>
        </div>
      )}

      <header>
        <div className="brand-group">
          <div className="brand">
            <Aperture />
            <strong>JARVIS</strong>
          </div>
          {!isTauri && !immersive && (
            <button
              type="button"
              className="mode-pill"
              aria-label="Voltar ao site de apresentação"
              title="Voltar à página de apresentação do J.A.R.V.I.S."
              onClick={() => {
                try {
                  window.history.replaceState(null, "", window.location.pathname);
                } catch {
                  /* ignore */
                }
                setShowLanding(true);
              }}
            >
              <Globe />
              <span>Site Oficial</span>
            </button>
          )}
          {!immersive && (
            <button
              type="button"
              className="mode-pill"
              aria-label="Modo de permissão"
              title="Clique para alternar entre Permissão Total, Revisão Automática e Somente Conversa"
              onClick={() => void cyclePermissionMode()}
            >
              {config.permission_mode === "auto" ? (
                <ShieldCheck />
              ) : config.permission_mode === "review" ? (
                <ShieldAlert />
              ) : (
                <Shield />
              )}
              <span>{permissionLabels[config.permission_mode]}</span>
            </button>
          )}
          {!immersive && (
            <button
              type="button"
              className={config.wake_word_enabled ? "wake-pill active" : "wake-pill"}
              aria-label="Alternar Wake Word Ei, Jarvis"
              aria-pressed={config.wake_word_enabled}
              title={
                config.wake_word_enabled
                  ? 'Wake Word "Ei, Jarvis" ativo · Clique para desativar'
                  : 'Ativar comando de voz "Ei, Jarvis"'
              }
              onClick={() => handleMascotQuickAction("toggle_wake")}
            >
              <Radio />
              <span>Ei, Jarvis</span>
              {config.wake_word_enabled && (
                <i
                  className="wake-level-dot"
                  style={{
                    transform: `scale(${1 + Math.min(1, wakeMicLevel) * 1.1})`,
                    opacity: 0.45 + Math.min(1, wakeMicLevel) * 0.55,
                  }}
                />
              )}
            </button>
          )}
          {!immersive && timers.length > 0 && (
            <div className="timer-pill" title="Timer ativo">
              <Clock />
              <span>
                {timers[0].label} · {formatRemaining(timers[0].ends_at)}
              </span>
              <button
                type="button"
                className="icon-tiny"
                aria-label="Cancelar timer"
                onClick={() =>
                  void cancelTimer(timers[0].id).then(() => refreshLists())
                }
              >
                <X />
              </button>
            </div>
          )}
        </div>

        {!immersive && (agents.length > 0 || browserSessionOpen || fileDiffs.length > 0 || terminalSessions.length > 0 || workspaceView === "terminal" || workspaceView === "routines") && (
          <div className="workspace-dock contextual-dock" role="tablist" aria-label="Áreas de trabalho">
            <button
              type="button"
              role="tab"
              aria-selected={workspaceView === "core"}
              className={workspaceView === "core" ? "dock-tab active" : "dock-tab"}
              onClick={() => setWorkspaceView("core")}
            >
              <Sparkles />
              <span>Núcleo</span>
            </button>
            {workspaceView === "routines" && (
              <button
                type="button"
                role="tab"
                aria-selected={workspaceView === "routines"}
                className="dock-tab active"
                onClick={() => setWorkspaceView("routines")}
              >
                <Zap />
                <span>Skills &amp; Rotinas</span>
              </button>
            )}
            {(terminalSessions.length > 0 || workspaceView === "terminal") && (
              <button
                type="button"
                role="tab"
                aria-selected={workspaceView === "terminal"}
                className={workspaceView === "terminal" ? "dock-tab active" : "dock-tab"}
                onClick={() => setWorkspaceView("terminal")}
              >
                <Terminal />
                <span>Terminal</span>
                {terminalSessions.some((s) => s.running) && (
                  <i className="dock-dot" />
                )}
              </button>
            )}
            {agents.length > 0 && (
              <button
                type="button"
                role="tab"
                aria-selected={workspaceView === "agents"}
                className={workspaceView === "agents" ? "dock-tab active" : "dock-tab"}
                onClick={() => setWorkspaceView("agents")}
              >
                <Bot />
                <span>Agentes</span>
                <em className="dock-badge">
                  {agents.filter((a) => a.status === "running").length || agents.length}
                </em>
              </button>
            )}
            {fileDiffs.length > 0 && (
              <button
                type="button"
                role="tab"
                aria-selected={workspaceView === "diff"}
                className={workspaceView === "diff" ? "dock-tab active" : "dock-tab"}
                onClick={() => setWorkspaceView("diff")}
              >
                <FileCode2 />
                <span>Código</span>
                <em className="dock-badge">{fileDiffs.length}</em>
              </button>
            )}
            {browserSessionOpen && (
              <div className="dock-tab-group">
                <button
                  type="button"
                  role="tab"
                  aria-selected={workspaceView === "browser"}
                  className={workspaceView === "browser" ? "dock-tab active" : "dock-tab"}
                  onClick={() => setWorkspaceView("browser")}
                >
                  <Globe />
                  <span>Navegador</span>
                  <i className="dock-dot" />
                </button>
                <button
                  type="button"
                  className="dock-close-btn"
                  aria-label="Fechar aba navegador"
                  title="Fechar navegador embutido"
                  onClick={handleCloseBrowserPanel}
                >
                  <X />
                </button>
              </div>
            )}
          </div>
        )}

        <nav>
          {!immersive && (
            <span className="hud-clock" aria-hidden="true">
              {clockText}
            </span>
          )}
          {!immersive && (
            <button
              aria-label="Nova conversa"
              title="Nova conversa"
              onClick={() => void startNewConversation()}
            >
              <Plus />
            </button>
          )}
          {!immersive && (
            <button
              aria-label="Skills e Rotinas"
              title="Skills (/comandos) e Rotinas Proativas"
              className={workspaceView === "routines" ? "active" : undefined}
              onClick={() => {
                void listSkills().then(setSkills).catch(() => {});
                void listRoutines().then(setRoutines).catch(() => {});
                setWorkspaceView((v) => (v === "routines" ? "core" : "routines"));
              }}
            >
              <Zap />
            </button>
          )}
          {!immersive && (
            <button
              aria-label="Terminal ao Vivo"
              title="Terminal ao Vivo"
              className={workspaceView === "terminal" ? "active" : undefined}
              onClick={() =>
                setWorkspaceView((v) => (v === "terminal" ? "core" : "terminal"))
              }
            >
              <Terminal />
            </button>
          )}
          {!immersive && (
            <button
              aria-label="Conectar Celular"
              title="Conectar Celular (QR Code)"
              aria-expanded={drawer === "settings" && settingsTab === "mobile"}
              onClick={() => {
                setSettingsTab("mobile");
                setDrawer((d) =>
                  d === "settings" && settingsTab === "mobile" ? "none" : "settings",
                );
              }}
            >
              <Smartphone />
            </button>
          )}
          {!immersive && (
            <button
              aria-label="Histórico da conversa"
              title="Conversas, Projetos e Memória"
              aria-expanded={drawer === "history"}
              onClick={() =>
                setDrawer((d) => (d === "history" ? "none" : "history"))
              }
            >
              <History />
            </button>
          )}
          <button
            aria-label={muted ? "Ativar som" : "Silenciar"}
            title={muted ? "Ativar som" : "Silenciar"}
            onClick={() => {
              const next = !muted;
              saveLs("muted", String(next));
              setMuted(next);
              mutedRef.current = next;
              if (next) speechRef.current?.stop();
            }}
          >
            {muted ? <VolumeX /> : <Volume2 />}
          </button>
          <button
            aria-label="Ajustes"
            title="Ajustes"
            aria-expanded={drawer === "settings"}
            onClick={() =>
              setDrawer((d) => (d === "settings" ? "none" : "settings"))
            }
          >
            <SlidersHorizontal />
          </button>
          <button
            aria-label={isFullscreen ? "Sair da tela cheia" : "Tela cheia"}
            title={isFullscreen ? "Sair da tela cheia (F11)" : "Tela cheia (F11)"}
            aria-pressed={isFullscreen}
            onClick={() => void handleToggleFullscreen()}
          >
            {isFullscreen ? <Minimize2 /> : <Maximize2 />}
          </button>
          <button
            aria-label={immersive ? "Sair do modo imersivo" : "Modo imersivo"}
            title={immersive ? "Sair do modo imersivo" : "Modo imersivo"}
            aria-pressed={immersive}
            onClick={() => {
              setImmersive(!immersive);
              setDrawer("none");
            }}
          >
            <Focus />
          </button>
        </nav>
      </header>

      {!isTauri && !immersive && (
        <aside className="web-preview-banner" aria-label="Sobre a demonstração">
          <strong>Demonstração visual</strong>
          <span>IA, voz e controle do Windows exigem o aplicativo desktop configurado.</span>
          <a href="https://github.com/k48w/Jarvis/releases/tag/v8.0.0" target="_blank" rel="noreferrer">Baixar para Windows</a>
        </aside>
      )}
      <div className="workspace">
        <section
          className={
            workspaceView === "core" || immersive
              ? "observatory"
              : "observatory dimmed"
          }
          aria-label="Núcleo de plasma"
        >
          <Stage
              state={mode}
              quality={quality}
              theme={theme}
              reduced={reduced}
              audioLevel={audioLevelRef}
          />
          <HudOverlay state={mode} immersive={immersive || workspaceView !== "core"} />
          {(workspaceView === "core" || immersive) && (
            <div className="core-caption">
              <span role="status">
                <i />
                {statusLabel[mode]}
              </span>
              {activeTool && (
                <div className="tool-live">
                  {activeTool.title}
                  {activeTool.summary ? ` · ${activeTool.summary}` : ""}
                </div>
              )}
            </div>
          )}
        </section>

        {screenPulseKind && (
          <div className={`screen-scan-overlay ${screenPulseKind}`} aria-hidden="true">
            <div className="screen-scan-beam" />
            <span>
              {screenPulseKind === "capture"
                ? "J.A.R.V.I.S. · ANALISANDO SUA TELA"
                : "J.A.R.V.I.S. · OPERANDO TELA"}
            </span>
          </div>
        )}

        {!immersive && workspaceView === "agents" && agents.length > 0 && (
          <AgentsView
            agents={agents}
            onLaunch={handleLaunchAgent}
            onCancel={handleCancelAgent}
            onRemove={handleRemoveAgent}
            onSendToCore={handleSendAgentToCore}
          />
        )}

        {!immersive && workspaceView === "diff" && fileDiffs.length > 0 && (
          <DiffView
            diffs={fileDiffs}
            projectPath={
              activeProjectId
                ? (projects.find((p) => p.id === activeProjectId)?.path ?? null)
                : null
            }
            onRevert={handleRevertDiff}
            onClearAll={() => {
              setFileDiffs([]);
              setWorkspaceView("core");
            }}
          />
        )}

        {!immersive && workspaceView === "terminal" && (
          <TerminalView
            sessions={terminalSessions}
            projectPath={
              activeProjectId
                ? (projects.find((p) => p.id === activeProjectId)?.path ?? null)
                : null
            }
            onRun={async (command, cwd) => {
              await runTerminalCommand(command, cwd);
            }}
            onCancel={async (id) => {
              await cancelTerminalCommand(id);
            }}
            onClear={() => setTerminalSessions([])}
          />
        )}

        {!immersive && workspaceView === "routines" && (
          <RoutinesView
            skills={skills}
            routines={routines}
            onRunPrompt={(prompt) => {
              setWorkspaceView("core");
              void submitText(prompt, false);
            }}
            onSaveSkill={async (slug, name, description, prompt) => {
              await saveSkill(slug, name, description, prompt);
              setSkills(await listSkills());
            }}
            onDeleteSkill={async (idOrSlug) => {
              await deleteSkill(idOrSlug);
              setSkills(await listSkills());
            }}
            onCreateRoutine={async (
              name,
              scheduleKind,
              intervalMinutes,
              dailyHhmm,
              prompt,
            ) => {
              await createRoutine(
                name,
                scheduleKind,
                intervalMinutes,
                dailyHhmm,
                prompt,
              );
              setRoutines(await listRoutines());
            }}
            onToggleRoutine={async (id, enabled) => {
              await toggleRoutine(id, enabled);
              setRoutines(await listRoutines());
            }}
            onDeleteRoutine={async (id) => {
              await deleteRoutine(id);
              setRoutines(await listRoutines());
            }}
          />
        )}

        {!immersive && workspaceView === "browser" && browserSessionOpen && (
          <BrowserView
            initialUrl={browserUrl}
            initialTitle={browserTitle}
            obscured={drawer !== "none" || immersive}
            mode={mode}
            statusText={statusLabel[mode]}
            messages={messages}
            streamingText={streamingText}
            liveTools={liveTools}
            streaming={isStreaming}
            busy={busy}
            draft={draft}
            onDraftChange={setDraft}
            onSend={() => submitText()}
            onStop={stopAll}
            onToggleMic={toggleListen}
            onBackToCore={() => setWorkspaceView("core")}
            onCloseBrowser={handleCloseBrowserPanel}
          />
        )}

        {!immersive && workspaceView === "core" && (
          <section className="conversation">
            {pendingApproval && (
              <div className="approval-card" role="alertdialog" aria-label="Aprovação de ação">
                <div className="approval-head">
                  <ShieldAlert />
                  <strong>{pendingApproval.title}</strong>
                </div>
                {pendingApproval.argsSummary && (
                  <code>{pendingApproval.argsSummary}</code>
                )}
                <p>{pendingApproval.description}</p>
                <div className="approval-actions">
                  <button
                    type="button"
                    className="mini-btn accent"
                    onClick={() => void handleApproval(true)}
                  >
                    Aprovar
                  </button>
                  <button
                    type="button"
                    className="mini-btn"
                    onClick={() => void handleApproval(false)}
                  >
                    Recusar
                  </button>
                  <button
                    type="button"
                    className="mini-btn"
                    onClick={() => void handleApproval(true, true)}
                  >
                    Ativar Permissão Total
                  </button>
                </div>
              </div>
            )}

            {errorBanner && (
              <div className="notice-pill error" role="alert">
                <span>{errorBanner}</span>
                <button
                  type="button"
                  className="icon-tiny"
                  aria-label="Fechar aviso"
                  onClick={() => setErrorBanner(null)}
                >
                  <X />
                </button>
              </div>
            )}

            {!errorBanner &&
              isTauri &&
              !isProviderConfigured &&
              !noticeDismissed && (
                <div className="notice-pill">
                  <button
                    type="button"
                    className="notice-link"
                    onClick={() => {
                      setSettingsTab("ai");
                      setDrawer("settings");
                    }}
                  >
                    Conecte sua conta ChatGPT (OAuth) ou Gemini em Ajustes → IA para começar.
                  </button>
                  <button
                    type="button"
                    className="icon-tiny"
                    aria-label="Dispensar aviso"
                    onClick={() => setNoticeDismissed(true)}
                  >
                    <X />
                  </button>
                </div>
              )}

            {visibleTraces.length > 0 && (
              <div className="tool-chips centered">
                {visibleTraces.map((t) => (
                  <span key={t.id} className={`tool-chip ${t.status}`}>
                    {t.title}
                  </span>
                ))}
              </div>
            )}

            <ResponseCard
              text={subtitleText}
              streaming={isStreaming}
              speaking={isSpeaking}
              canReplay={canReplayVoice}
              onReplay={() => handleReplayText(subtitleText)}
            />

            {!subtitleText && !isStreaming && !pendingApproval && (
              <div className="quick-prompts" aria-label="Sugestões rápidas">
                {quickPrompts.map((prompt) => (
                  <button
                    key={prompt}
                    type="button"
                    className="quick-chip"
                    onClick={() => void submitText(prompt)}
                  >
                    {prompt}
                  </button>
                ))}
              </div>
            )}

            <VoiceSpectrum
              state={mode}
              audioLevel={audioLevelRef}
              active={mode === "listening" || isSpeaking || mode === "thinking"}
            />

            <div className="composer">
              {(attachedImage || attachedFiles.length > 0) && (
                <div className="attachment-tray" aria-label="Anexos prontos para enviar">
                  {attachedImage && (
                    <div className="attach-img-chip">
                      <img src={attachedImage.previewUrl} alt={attachedImage.name} />
                      <span>{attachedImage.name}</span>
                      <button
                        type="button"
                        className="icon-tiny"
                        aria-label="Remover imagem"
                        onClick={() => setAttachedImage(null)}
                      >
                        <X />
                      </button>
                    </div>
                  )}
                  {attachedFiles.map((f) => (
                    <div key={f.id} className="attach-file-chip">
                      <FileText />
                      <span>{f.name}</span>
                      <button
                        type="button"
                        className="icon-tiny"
                        aria-label={`Remover ${f.name}`}
                        onClick={() =>
                          setAttachedFiles((prev) => prev.filter((x) => x.id !== f.id))
                        }
                      >
                        <X />
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <input
                ref={fileInputRef}
                type="file"
                multiple
                accept="image/*,.txt,.md,.json,.ts,.tsx,.js,.jsx,.rs,.py,.html,.css,.csv,.log,.xml,.yaml,.yml,.toml,.sql,.pdf"
                className="sr-only-input"
                onChange={(e) => {
                  if (e.target.files?.length) {
                    void handleAttachFiles(e.target.files);
                  }
                  e.target.value = "";
                }}
              />

              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void submitText();
                }}
              >
                <textarea
                  ref={inputRef}
                  aria-label="Mensagem"
                  value={draft}
                  placeholder="Fale com J.A.R.V.I.S., cole um print (Ctrl+V) ou arraste arquivos…"
                  maxLength={4000}
                  onChange={(e) => setDraft(e.target.value)}
                  onPaste={(e) => {
                    const items = e.clipboardData?.items;
                    if (!items) return;
                    const imgFiles: File[] = [];
                    for (let i = 0; i < items.length; i++) {
                      if (items[i].type.startsWith("image/")) {
                        const f = items[i].getAsFile();
                        if (f) imgFiles.push(f);
                      }
                    }
                    if (imgFiles.length > 0) {
                      e.preventDefault();
                      void handleAttachFiles(imgFiles);
                    }
                  }}
                  onKeyDown={(e) => {
                    if (
                      e.key === "Enter" &&
                      !e.shiftKey &&
                      !e.nativeEvent.isComposing
                    ) {
                      e.preventDefault();
                      void submitText();
                    }
                  }}
                />
                <div className="form-actions">
                  <button
                    type="button"
                    className="attach-btn"
                    aria-label="Anexar arquivo ou imagem"
                    title="Anexar imagem, código ou PDF (ou arraste na tela)"
                    disabled={isStreaming || mode === "thinking"}
                    onClick={() => fileInputRef.current?.click()}
                  >
                    <Paperclip />
                  </button>
                  <button
                    type="button"
                    className="attach-btn"
                    aria-label="Colar conteúdo do Ctrl+C"
                    title="Anexar texto ou print atual da área de transferência (Ctrl+C)"
                    disabled={isStreaming || mode === "thinking"}
                    onClick={() => void handlePasteClipboard()}
                  >
                    <ClipboardPaste />
                  </button>
                  <button
                    type="button"
                    className="mic"
                    aria-label="Falar pelo microfone"
                    title="Falar pelo microfone"
                    aria-pressed={mode === "listening"}
                    disabled={isStreaming || mode === "thinking"}
                    onClick={() => void toggleListen()}
                  >
                    <Mic />
                  </button>
                  {busy ? (
                    <button
                      type="button"
                      className="send stop"
                      aria-label="Interromper"
                      title="Interromper resposta"
                      onClick={() => void stopAll()}
                    >
                      <Square />
                    </button>
                  ) : (
                    <button
                      className="send"
                      aria-label="Enviar"
                      disabled={
                        !draft.trim() && !attachedImage && attachedFiles.length === 0
                      }
                    >
                      <ArrowUp />
                    </button>
                  )}
                </div>
              </form>
              <small className="local-note">
                {isTauri
                  ? `${providerLabel(config)}${
                      activeProjectId
                        ? ` · Projeto: ${projects.find((p) => p.id === activeProjectId)?.name ?? "Vinculado"}`
                        : ""
                    } · ${permissionLabels[config.permission_mode]}`
                  : "Demonstração visual · IA e ações disponíveis no aplicativo desktop"}
              </small>
            </div>
          </section>
        )}
      </div>

      {!isTauri && !immersive && showMascot && workspaceView !== "browser" && (
        <MascotBuddy
          mode={mode}
          speaking={isSpeaking}
          reduced={reduced}
          audioLevel={audioLevelRef}
          disabledMic={isStreaming || mode === "thinking"}
          externalBubble={mascotBubble}
          externalReaction={mascotReaction}
          muted={muted}
          wakeWord={config.wake_word_enabled}
          onNewChat={() => {
            setWorkspaceView("core");
            void startNewConversation();
          }}
          onToggleMic={() => void toggleListen()}
          onQuickAction={handleMascotQuickAction}
        />
      )}

      {drawer === "history" && (
        <HistoryDrawer
          tab={historyTab}
          onTabChange={setHistoryTab}
          conversations={conversations}
          projects={projects}
          activeConversationId={activeConvId}
          activeProjectId={activeProjectId}
          messages={messages}
          memories={memories}
          onSelectConversation={(id) => {
            void loadConversation(id);
            setDrawer("none");
          }}
          onCreateConversation={(projectId) => {
            void startNewConversation(projectId ?? null);
            setDrawer("none");
          }}
          onRenameConversation={async (id, title) => {
            await renameConversation(id, title);
            await refreshLists();
          }}
          onDeleteConversation={(id) => {
            void deleteConversation(id).then(async () => {
              const nextList = await listConversations();
              setConversations(nextList);
              if (id === activeConvId) {
                if (nextList.length > 0) {
                  await loadConversation(nextList[0].id);
                } else {
                  await startNewConversation();
                }
              }
            });
          }}
          onSetConversationProject={async (convId, projectId) => {
            const updated = await setConversationProject(convId, projectId);
            if (convId === activeConvIdRef.current) {
              setActiveProjectId(updated.project_id ?? null);
            }
            await refreshLists();
          }}
          onPickAndAddProject={async () => {
            if (!isTauri) return "MANUAL";
            try {
              const folder = await pickProjectFolder();
              if (!folder) return null;
              await addProject(folder);
              await refreshLists();
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onAddProjectPath={async (path) => {
            try {
              await addProject(path);
              await refreshLists();
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onDeleteProject={async (id) => {
            await deleteProject(id);
            if (activeProjectId === id) {
              setActiveProjectId(null);
            }
            await refreshLists();
          }}
          onAddMemory={async (content) => {
            await addMemory(content);
            await refreshLists();
          }}
          onDeleteMemory={async (id) => {
            await deleteMemory(id);
            await refreshLists();
          }}
          onClose={() => setDrawer("none")}
        />
      )}

      {drawer === "settings" && (
        <SettingsDrawer
          tab={settingsTab}
          onTabChange={setSettingsTab}
          config={config}
          keyStatus={keyStatus}
          chatgptAuth={chatgptAuth}
          chatgptModels={chatgptModels}
          googleAuth={googleAuth}
          elevenVoices={elevenVoices}
          quality={quality}
          theme={theme}
          volume={volume}
          reduced={reduced}
          showMascot={showMascot}
          onUpdateConfig={updateAppConfig}
          onSaveKey={async (slot: KeySlot, value: string) => {
            const next = await saveApiKey(slot, value);
            setKeyStatus(next);
            keyStatusRef.current = next;
          }}
          onClearKey={async (slot: KeySlot) => {
            const next = await clearApiKey(slot);
            setKeyStatus(next);
            keyStatusRef.current = next;
            if (slot === "elevenlabs") {
              setElevenVoices([]);
            }
          }}
          onChatgptLogin={async (forceRegister = false) => {
            try {
              const res = await startChatgptLogin(forceRegister);
              setChatgptAuth(res.status);
              setChatgptModels(res.models);
              if (
                res.selectedModel &&
                res.selectedModel !== configRef.current.chatgpt_model
              ) {
                const updated = {
                  ...configRef.current,
                  chatgpt_model: res.selectedModel,
                };
                setConfig(updated);
                configRef.current = updated;
              }
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onChatgptLogout={async (resetRegistration = false) => {
            const next = await logoutChatgpt(resetRegistration);
            setChatgptAuth(next);
            setChatgptModels([]);
          }}
          onRefreshChatgptModels={async () => {
            try {
              const models = await listChatgptModels();
              setChatgptModels(models);
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onImportGoogleJson={async (jsonText) => {
            try {
              const res = await importGoogleOauthJson(jsonText);
              setConfig(res.config);
              configRef.current = res.config;
              setKeyStatus(res.keyStatus);
              keyStatusRef.current = res.keyStatus;
              setGoogleAuth(res.googleAuth);
              googleAuthRef.current = res.googleAuth;
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onGoogleLogin={async () => {
            try {
              const status = await startGoogleLogin();
              setGoogleAuth(status);
              googleAuthRef.current = status;
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onGoogleLogout={async () => {
            const next = await logoutGoogle();
            setGoogleAuth(next);
            googleAuthRef.current = next;
          }}
          onRefreshElevenVoices={async () => {
            try {
              const list = await listElevenlabsVoices();
              setElevenVoices(list);
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onTestConnection={testConnection}
          onTestVoice={handleTestVoice}
          onUpdateShortcut={async (shortcut) => {
            try {
              const updated = await updateGlobalShortcut(shortcut);
              setConfig(updated);
              configRef.current = updated;
              return null;
            } catch (err) {
              return err instanceof Error ? err.message : String(err);
            }
          }}
          onQualityChange={(q) => {
            saveLs("quality", q);
            setQuality(q);
          }}
          onThemeChange={(t) => {
            saveLs("theme", t);
            setTheme(t);
          }}
          onVolumeChange={(v) => {
            saveLs("volume", String(v));
            setVolume(v);
          }}
          onReducedChange={(r) => {
            saveLs("reduced", String(r));
            setReduced(r);
          }}
          onShowMascotChange={(v) => {
            saveLs("showMascot", String(v));
            setShowMascot(v);
          }}
          onClose={() => setDrawer("none")}
        />
      )}

      <CommandPalette
        open={paletteOpen}
        wakeEnabled={config.wake_word_enabled}
        muted={muted}
        onClose={() => setPaletteOpen(false)}
        onRunPrompt={(prompt) => {
          setWorkspaceView("core");
          void submitText(prompt, false);
        }}
        onPasteClipboard={() => void handlePasteClipboard()}
        onToggleWake={() => handleMascotQuickAction("toggle_wake")}
        onToggleMute={() => handleMascotQuickAction("toggle_mute")}
        onNewChat={() => {
          setWorkspaceView("core");
          void startNewConversation();
        }}
        onThemeChange={(t) => {
          saveLs("theme", t);
          setTheme(t);
          themeRef.current = t;
        }}
        onToggleFullscreen={() => void handleToggleFullscreen()}
        onOpenSettings={() => setDrawer("settings")}
      />

      {audioError && <output>Áudio indisponível nesta sessão.</output>}
    </main>
  );
}

const isMascotWindow =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("window") === "mascot";

createRoot(document.getElementById("root")!).render(
  isMascotWindow ? <MascotWindow /> : <App />,
);

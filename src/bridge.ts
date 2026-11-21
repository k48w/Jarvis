import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Provider = "chatgpt" | "gemini";
export type GeminiAuthMode = "apikey" | "oauth";
export type PermissionMode = "auto" | "review" | "safe";
export type TtsEngine = "elevenlabs" | "off";
export type TtsReplyMode = "brief" | "miconly" | "full";
export type SttEngine = "auto" | "elevenlabs" | "gemini" | "browser";
export type KeySlot = "gemini" | "google_client_secret" | "elevenlabs";

export interface AppConfig {
  provider: Provider;
  gemini_auth_mode: GeminiAuthMode;
  google_oauth_client_id: string;
  google_oauth_project_id: string;
  gemini_model: string;
  chatgpt_model: string;
  permission_mode: PermissionMode;
  system_prompt: string;
  tts_engine: TtsEngine;
  tts_reply_mode: TtsReplyMode;
  tts_rate: number;
  elevenlabs_voice_id: string;
  elevenlabs_model_id: string;
  stt_engine: SttEngine;
  mic_device_id: string;
  auto_listen_vad: boolean;
  wake_word_enabled: boolean;
  hands_free_mode: boolean;
  global_shortcut: string;
  minimize_to_tray: boolean;
  web_search_enabled: boolean;
}

export interface KeyStatus {
  configured: boolean;
  preview: string | null;
}

export interface AllKeyStatus {
  gemini: KeyStatus;
  google_client_secret: KeyStatus;
  elevenlabs: KeyStatus;
}

export interface ConnectionTestResult {
  ok: boolean;
  message: string;
  latencyMs: number;
}

export interface ToolTrace {
  id: string;
  name: string;
  title: string;
  status: string;
  summary: string;
}

export interface StoredMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  timestamp: string;
  mode?: string | null;
  tool_events: ToolTrace[];
}

export interface Conversation {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  project_id?: string | null;
  project_path?: string | null;
  messages: StoredMessage[];
}

export interface ConversationSummary {
  id: string;
  title: string;
  updated_at: string;
  message_count: number;
  preview: string;
  project_id?: string | null;
  project_path?: string | null;
}

export interface ProjectItem {
  id: string;
  name: string;
  path: string;
  created_at: string;
}

export interface MemoryItem {
  id: string;
  content: string;
  created_at: string;
}

export interface ActiveTimer {
  id: string;
  label: string;
  ends_at: string;
  total_seconds: number;
}

export interface ElevenLabsVoice {
  voiceId: string;
  name: string;
  category: string;
}

export interface SpeechOutput {
  mime_type: string;
  audio_base64: string;
}

export type AgentRole = "research" | "coder" | "operator" | "planner";

export interface ScreenCapture {
  dataBase64: string;
  mimeType: string;
  width: number;
  height: number;
}

export interface BrowserPageContent {
  url: string;
  finalUrl: string;
  title: string;
  text: string;
}

export interface BrowserElement {
  id: number;
  tag: string;
  role: string;
  inputType: string;
  label: string;
  placeholder: string;
  value: string;
  href: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BrowserInspectResult {
  url: string;
  title: string;
  scrollY: number;
  maxScrollY: number;
  viewportWidth: number;
  viewportHeight: number;
  text: string;
  elements: BrowserElement[];
}

export interface BrowserStatus {
  url: string;
  title: string;
  elementCount: number;
}

export interface BrowserSnapshot {
  url: string;
  title: string;
  dataBase64: string;
  mimeType: string;
  viewportWidth: number;
  viewportHeight: number;
  scrollY: number;
  maxScrollY: number;
  elements: BrowserElement[];
  text: string;
}

export interface BrowserUpdatedEvent {
  action: string;
  url: string;
  title: string;
}

export interface AgentSpawnedEvent {
  id: string;
  role: AgentRole;
  name: string;
  task: string;
}

export interface BrowserPanelEvent {
  open: boolean;
  url?: string;
  title?: string;
}

export interface ScreenPulseEvent {
  kind: "capture" | "action";
}

export interface ToolApprovalRequest {
  approvalId: string;
  conversationId: string;
  toolName: string;
  title: string;
  description: string;
  argsSummary: string;
}

export type ChatEvent =
  | { type: "delta"; conversationId: string; text: string }
  | {
      type: "tool_start";
      conversationId: string;
      id: string;
      name: string;
      title: string;
      summary: string;
    }
  | {
      type: "tool_end";
      conversationId: string;
      id: string;
      name: string;
      title: string;
      status: string;
      summary: string;
    }
  | { type: "mode"; conversationId: string; mode: string }
  | {
      type: "model_updated";
      conversationId: string;
      provider: Provider;
      model: string;
    }
  | { type: "done"; conversationId: string; message: StoredMessage }
  | { type: "cancelled"; conversationId: string }
  | { type: "error"; conversationId: string; error: string };

export interface TimerFiredEvent {
  id: string;
  label: string;
}

export interface TrayActionEvent {
  action:
    | "listen"
    | "new_chat"
    | "focus_chat"
    | "see_screen"
    | "toggle_mute"
    | "toggle_wake"
    | "cycle_theme"
    | "hide_mascot";
}

export type MascotActionKind =
  | "listen"
  | "open_chat"
  | "new_chat"
  | "see_screen"
  | "toggle_mute"
  | "toggle_wake"
  | "cycle_theme"
  | "hide";

export interface MascotStatePayload {
  mode: "idle" | "listening" | "thinking" | "responding" | "alert";
  speaking: boolean;
  theme: "cyan" | "gold" | "violet" | "crimson";
  reduced: boolean;
  bubble?: string | null;
  reaction?: "wave" | "cheer" | "alert" | null;
  muted?: boolean;
  wakeWord?: boolean;
}

export interface ClipboardContent {
  kind: "text" | "image" | "empty";
  text?: string | null;
  imageBase64?: string | null;
  imageMime?: string | null;
  width?: number | null;
  height?: number | null;
}

export interface FileDiffEvent {
  id: string;
  conversationId: string;
  path: string;
  fileName: string;
  tool: "write_file" | "edit_file";
  existedBefore: boolean;
  oldContent: string;
  newContent: string;
  timestamp: string;
}

export interface ChatGptAuthStatus {
  connected: boolean;
  email: string | null;
  name: string | null;
  planUsageGranted: boolean;
  clientIdPreview: string | null;
  expiresAt: number | null;
}

export interface ChatGptLoginResult {
  status: ChatGptAuthStatus;
  models: string[];
  selectedModel: string;
}

export const emptyChatGptAuthStatus: ChatGptAuthStatus = {
  connected: false,
  email: null,
  name: null,
  planUsageGranted: false,
  clientIdPreview: null,
  expiresAt: null,
};

export interface GoogleAuthStatus {
  connected: boolean;
  email: string | null;
  name: string | null;
  projectId: string | null;
  expiresAt: number | null;
}

export interface GoogleImportResult {
  config: AppConfig;
  keyStatus: AllKeyStatus;
  googleAuth: GoogleAuthStatus;
}

export const emptyGoogleAuthStatus: GoogleAuthStatus = {
  connected: false,
  email: null,
  name: null,
  projectId: null,
  expiresAt: null,
};

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const defaultConfig: AppConfig = {
  provider: "chatgpt",
  gemini_auth_mode: "apikey",
  google_oauth_client_id: "",
  google_oauth_project_id: "",
  gemini_model: "gemini-2.5-flash",
  chatgpt_model: "gpt-6.2-sol",
  permission_mode: "auto",
  system_prompt:
    "Você é J.A.R.V.I.S., um assistente pessoal de desktop para Windows, inteligente, preciso e leal.",
  tts_engine: "elevenlabs",
  tts_reply_mode: "brief",
  tts_rate: 1.0,
  elevenlabs_voice_id: "JBFqnCBsd6RMkjVDRZzb",
  elevenlabs_model_id: "eleven_flash_v2_5",
  stt_engine: "auto",
  mic_device_id: "",
  auto_listen_vad: true,
  wake_word_enabled: false,
  hands_free_mode: false,
  global_shortcut: "Alt+Space",
  minimize_to_tray: true,
  web_search_enabled: true,
};

// Local browser fallback state (used only when running outside Tauri, e.g. Vite browser test).
const bus = new EventTarget();
const BROWSER_CFG_KEY = "jarvis:browser:config";
const BROWSER_CONVS_KEY = "jarvis:browser:convs";
const BROWSER_MEM_KEY = "jarvis:browser:mem";
const BROWSER_PROJ_KEY = "jarvis:browser:projects";
let browserTimers: number[] = [];

function lsRead<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? ({ ...fallback, ...JSON.parse(raw) } as T) : fallback;
  } catch {
    return fallback;
  }
}

function lsReadArray<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T[]) : [];
  } catch {
    return [];
  }
}

function lsWrite(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* ignore storage limits in browser fallback */
  }
}

function emitBrowserEvent(name: string, payload: unknown) {
  bus.dispatchEvent(new CustomEvent(name, { detail: payload }));
}

function browserFallbackReply(text: string): { mode: string; reply: string } {
  const words = text
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase();
  if (/\balerta\b/.test(words)) {
    return {
      mode: "alert",
      reply: "Pulso âmbar ativado. Sistemas de monitoramento em estado de atenção.",
    };
  }
  if (/^(oi|ola|bom dia|boa noite)\b/.test(words)) {
    return {
      mode: "responding",
      reply:
        "Olá. No modo navegador estou em prévia; abra o executável desktop para acesso total ao Windows e às APIs de IA.",
    };
  }
  return {
    mode: "responding",
    reply:
      "Prévia web ativa. No aplicativo desktop, respondo via ChatGPT ou Gemini com voz ElevenLabs e controle do Windows.",
  };
}

export async function getConfig(): Promise<AppConfig> {
  if (isTauri) return invoke<AppConfig>("get_config");
  return lsRead<AppConfig>(BROWSER_CFG_KEY, defaultConfig);
}

export async function saveConfig(config: AppConfig): Promise<AppConfig> {
  if (isTauri) return invoke<AppConfig>("save_config", { config });
  lsWrite(BROWSER_CFG_KEY, config);
  return config;
}

export async function getKeyStatus(): Promise<AllKeyStatus> {
  if (isTauri) return invoke<AllKeyStatus>("get_key_status");
  return {
    gemini: { configured: false, preview: null },
    google_client_secret: { configured: false, preview: null },
    elevenlabs: { configured: false, preview: null },
  };
}

export async function saveApiKey(slot: KeySlot, key: string): Promise<AllKeyStatus> {
  if (isTauri) return invoke<AllKeyStatus>("save_api_key", { slot, key });
  return getKeyStatus();
}

export async function clearApiKey(slot: KeySlot): Promise<AllKeyStatus> {
  if (isTauri) return invoke<AllKeyStatus>("clear_api_key", { slot });
  return getKeyStatus();
}

export async function testConnection(): Promise<ConnectionTestResult> {
  if (isTauri) return invoke<ConnectionTestResult>("test_connection");
  return {
    ok: false,
    message: "Teste de conexão disponível no app desktop J.A.R.V.I.S.",
    latencyMs: 0,
  };
}

export async function updateGlobalShortcut(shortcut: string): Promise<AppConfig> {
  if (isTauri) return invoke<AppConfig>("update_global_shortcut", { shortcut });
  const cfg = lsRead<AppConfig>(BROWSER_CFG_KEY, defaultConfig);
  const next = { ...cfg, global_shortcut: shortcut };
  lsWrite(BROWSER_CFG_KEY, next);
  return next;
}

export async function listConversations(): Promise<ConversationSummary[]> {
  if (isTauri) return invoke<ConversationSummary[]>("list_conversations");
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
  return convs
    .map((c) => ({
      id: c.id,
      title: c.title,
      updated_at: c.updated_at,
      message_count: c.messages.length,
      preview: c.messages.at(-1)?.text.slice(0, 72) ?? "",
      project_id: c.project_id ?? null,
      project_path: c.project_path ?? null,
    }))
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
}

export async function getConversation(id: string): Promise<Conversation> {
  if (isTauri) return invoke<Conversation>("get_conversation", { id });
  const conv = lsReadArray<Conversation>(BROWSER_CONVS_KEY).find((c) => c.id === id);
  if (!conv) throw new Error("Conversa não encontrada");
  return conv;
}

export async function createConversation(
  title?: string,
  projectId?: string | null,
): Promise<Conversation> {
  if (isTauri) {
    return invoke<Conversation>("create_conversation", {
      title: title ?? null,
      projectId: projectId ?? null,
    });
  }
  const now = new Date().toISOString();
  const projects = lsReadArray<ProjectItem>(BROWSER_PROJ_KEY);
  const proj = projectId ? projects.find((p) => p.id === projectId) : undefined;
  const conv: Conversation = {
    id: crypto.randomUUID(),
    title: title?.trim() || "Nova conversa",
    created_at: now,
    updated_at: now,
    project_id: proj?.id ?? null,
    project_path: proj?.path ?? null,
    messages: [],
  };
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
  convs.unshift(conv);
  lsWrite(BROWSER_CONVS_KEY, convs);
  return conv;
}

export async function setConversationProject(
  id: string,
  projectId: string | null,
): Promise<Conversation> {
  if (isTauri) {
    return invoke<Conversation>("set_conversation_project", {
      id,
      projectId: projectId ?? null,
    });
  }
  const projects = lsReadArray<ProjectItem>(BROWSER_PROJ_KEY);
  const proj = projectId ? projects.find((p) => p.id === projectId) : undefined;
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
  const target = convs.find((c) => c.id === id);
  if (!target) throw new Error("Conversa não encontrada");
  target.project_id = proj?.id ?? null;
  target.project_path = proj?.path ?? null;
  target.updated_at = new Date().toISOString();
  lsWrite(BROWSER_CONVS_KEY, convs);
  return target;
}

export async function listProjects(): Promise<ProjectItem[]> {
  if (isTauri) return invoke<ProjectItem[]>("list_projects");
  return lsReadArray<ProjectItem>(BROWSER_PROJ_KEY);
}

export async function addProject(
  path: string,
  name?: string,
): Promise<ProjectItem> {
  if (isTauri) {
    return invoke<ProjectItem>("add_project", { path, name: name ?? null });
  }
  const clean = path.trim();
  const parts = clean.split(/[\\/]/).filter(Boolean);
  const item: ProjectItem = {
    id: crypto.randomUUID(),
    name: name?.trim() || parts.at(-1) || "Projeto",
    path: clean,
    created_at: new Date().toISOString(),
  };
  const list = lsReadArray<ProjectItem>(BROWSER_PROJ_KEY);
  list.unshift(item);
  lsWrite(BROWSER_PROJ_KEY, list);
  return item;
}

export async function deleteProject(id: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_project", { id });
  const list = lsReadArray<ProjectItem>(BROWSER_PROJ_KEY).filter((p) => p.id !== id);
  lsWrite(BROWSER_PROJ_KEY, list);
}

export async function pickProjectFolder(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("pick_project_folder");
  return null;
}

export async function toggleFullscreen(): Promise<boolean> {
  if (isTauri) return invoke<boolean>("toggle_fullscreen");
  if (!document.fullscreenElement) {
    await document.documentElement.requestFullscreen?.().catch(() => {});
    return !!document.fullscreenElement;
  }
  await document.exitFullscreen?.().catch(() => {});
  return false;
}

export async function getFullscreen(): Promise<boolean> {
  if (isTauri) return invoke<boolean>("get_fullscreen");
  return !!document.fullscreenElement;
}

export async function renameConversation(id: string, title: string): Promise<Conversation> {
  if (isTauri) return invoke<Conversation>("rename_conversation", { id, title });
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
  const target = convs.find((c) => c.id === id);
  if (!target) throw new Error("Conversa não encontrada");
  target.title = title;
  target.updated_at = new Date().toISOString();
  lsWrite(BROWSER_CONVS_KEY, convs);
  return target;
}

export async function deleteConversation(id: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_conversation", { id });
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY).filter((c) => c.id !== id);
  lsWrite(BROWSER_CONVS_KEY, convs);
}

export async function listMemories(): Promise<MemoryItem[]> {
  if (isTauri) return invoke<MemoryItem[]>("list_memories");
  return lsReadArray<MemoryItem>(BROWSER_MEM_KEY);
}

export async function addMemory(content: string): Promise<MemoryItem> {
  if (isTauri) return invoke<MemoryItem>("add_memory", { content });
  const items = lsReadArray<MemoryItem>(BROWSER_MEM_KEY);
  const item: MemoryItem = {
    id: crypto.randomUUID(),
    content: content.trim(),
    created_at: new Date().toISOString(),
  };
  items.push(item);
  lsWrite(BROWSER_MEM_KEY, items);
  return item;
}

export async function deleteMemory(id: string): Promise<void> {
  if (isTauri) return invoke<void>("delete_memory", { id });
  const items = lsReadArray<MemoryItem>(BROWSER_MEM_KEY).filter((m) => m.id !== id);
  lsWrite(BROWSER_MEM_KEY, items);
}

export async function sendMessage(
  conversationId: string,
  text: string,
  image?: { base64: string; mimeType: string } | null,
): Promise<Conversation> {
  if (isTauri) {
    return invoke<Conversation>("send_message", {
      conversationId,
      text,
      imageBase64: image?.base64 ?? null,
      imageMime: image?.mimeType ?? null,
    });
  }
  const convs = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
  let conv = convs.find((c) => c.id === conversationId);
  if (!conv) {
    conv = await createConversation();
    convs.unshift(conv);
  }
  const now = new Date().toISOString();
  if (conv.messages.length === 0 && conv.title === "Nova conversa") {
    conv.title = text.trim().slice(0, 48);
  }
  conv.updated_at = now;
  conv.messages.push({
    id: crypto.randomUUID(),
    role: "user",
    text: text.trim(),
    timestamp: now,
    tool_events: [],
  });
  lsWrite(BROWSER_CONVS_KEY, convs);

  const { mode, reply } = browserFallbackReply(text);
  const lower = text.toLowerCase();
  const t1 = window.setTimeout(() => {
    if (lower.includes("navegador") || lower.includes("whatsapp")) {
      emitBrowserEvent("jarvis:browser-panel", {
        open: true,
        url: lower.includes("whatsapp")
          ? "https://web.whatsapp.com/"
          : "https://pt.wikipedia.org/wiki/Intelig%C3%AAncia_artificial",
        title: lower.includes("whatsapp") ? "WhatsApp Web" : "Navegador J.A.R.V.I.S.",
      });
    } else if (lower.includes("fechar navegador")) {
      emitBrowserEvent("jarvis:browser-panel", { open: false });
    } else if (lower.includes("agente")) {
      const aid = `agent:${crypto.randomUUID()}`;
      emitBrowserEvent("jarvis:agent-spawned", {
        id: aid,
        role: "research",
        name: "Pesquisador Web",
        task: text.trim(),
      });
      window.setTimeout(() => {
        const agentReply = `**Missão concluída**\n\n- Pesquisa executada em segundo plano para: *${text.trim()}*`;
        emitBrowserEvent("jarvis:chat-event", {
          type: "delta",
          conversationId: aid,
          text: agentReply,
        });
        emitBrowserEvent("jarvis:chat-event", {
          type: "done",
          conversationId: aid,
          message: {
            id: crypto.randomUUID(),
            role: "assistant",
            text: agentReply,
            timestamp: new Date().toISOString(),
            mode: "responding",
            tool_events: [],
          },
        });
      }, 450);
    }

    if (mode === "alert") {
      emitBrowserEvent("jarvis:chat-event", {
        type: "mode",
        conversationId: conv.id,
        mode: "alert",
      });
    }
    emitBrowserEvent("jarvis:chat-event", {
      type: "delta",
      conversationId: conv.id,
      text: reply,
    });
    const assistantMsg: StoredMessage = {
      id: crypto.randomUUID(),
      role: "assistant",
      text: reply,
      timestamp: new Date().toISOString(),
      mode,
      tool_events: [],
    };
    const latest = lsReadArray<Conversation>(BROWSER_CONVS_KEY);
    const target = latest.find((c) => c.id === conv.id);
    if (target) {
      target.messages.push(assistantMsg);
      target.updated_at = assistantMsg.timestamp;
      lsWrite(BROWSER_CONVS_KEY, latest);
    }
    emitBrowserEvent("jarvis:chat-event", {
      type: "done",
      conversationId: conv.id,
      message: assistantMsg,
    });
  }, 450);
  browserTimers.push(t1);

  return conv;
}

export async function cancelChat(): Promise<void> {
  if (isTauri) return invoke<void>("cancel_chat");
  for (const id of browserTimers) clearTimeout(id);
  browserTimers = [];
}

export async function resolveToolApproval(
  approvalId: string,
  approved: boolean,
): Promise<void> {
  if (isTauri) {
    return invoke<void>("resolve_tool_approval", { approvalId, approved });
  }
}

export async function listActiveTimers(): Promise<ActiveTimer[]> {
  if (isTauri) return invoke<ActiveTimer[]>("list_active_timers");
  return [];
}

export async function cancelTimer(id: string): Promise<boolean> {
  if (isTauri) return invoke<boolean>("cancel_timer", { id });
  return false;
}

export async function listElevenlabsVoices(): Promise<ElevenLabsVoice[]> {
  if (isTauri) return invoke<ElevenLabsVoice[]>("list_elevenlabs_voices");
  return [];
}

export async function synthesizeSpeech(text: string): Promise<SpeechOutput> {
  if (isTauri) return invoke<SpeechOutput>("synthesize_speech", { text });
  throw new Error("TTS disponível no app desktop.");
}

export async function transcribeAudio(
  audioBase64: string,
  mimeType: string,
): Promise<string> {
  if (isTauri) return invoke<string>("transcribe_audio", { audioBase64, mimeType });
  throw new Error("NO_STT_PROVIDER");
}

export async function getChatgptAuthStatus(): Promise<ChatGptAuthStatus> {
  if (isTauri) return invoke<ChatGptAuthStatus>("get_chatgpt_auth_status");
  return emptyChatGptAuthStatus;
}

export async function startChatgptLogin(
  forceRegister = false,
): Promise<ChatGptLoginResult> {
  if (isTauri) {
    return invoke<ChatGptLoginResult>("start_chatgpt_login", { forceRegister });
  }
  throw new Error("Login OAuth com ChatGPT disponível no aplicativo desktop J.A.R.V.I.S.");
}

export async function logoutChatgpt(
  resetRegistration = false,
): Promise<ChatGptAuthStatus> {
  if (isTauri) {
    return invoke<ChatGptAuthStatus>("logout_chatgpt", { resetRegistration });
  }
  return emptyChatGptAuthStatus;
}

export async function listChatgptModels(): Promise<string[]> {
  if (isTauri) return invoke<string[]>("list_chatgpt_models");
  return [];
}

export async function getGoogleAuthStatus(): Promise<GoogleAuthStatus> {
  if (isTauri) return invoke<GoogleAuthStatus>("get_google_auth_status");
  return emptyGoogleAuthStatus;
}

export async function importGoogleOauthJson(
  jsonText: string,
): Promise<GoogleImportResult> {
  if (isTauri) {
    return invoke<GoogleImportResult>("import_google_oauth_json", { jsonText });
  }
  throw new Error("Importação de credenciais Google OAuth disponível no app desktop.");
}

export async function startGoogleLogin(): Promise<GoogleAuthStatus> {
  if (isTauri) return invoke<GoogleAuthStatus>("start_google_login");
  throw new Error("Login OAuth com Google disponível no aplicativo desktop J.A.R.V.I.S.");
}

export async function logoutGoogle(): Promise<GoogleAuthStatus> {
  if (isTauri) return invoke<GoogleAuthStatus>("logout_google");
  return emptyGoogleAuthStatus;
}

export async function startAgentTask(role: AgentRole, task: string): Promise<string> {
  if (isTauri) {
    return invoke<string>("start_agent_task", { role, task });
  }
  const agentId = `agent:${crypto.randomUUID()}`;
  const previewReply = `**Prévia Web (${role})**\n\n- Missão recebida: *${task.trim()}*\n- No app desktop **J.A.R.V.I.S.**, este agente executa em paralelo com acesso às ferramentas reais do Windows e busca na web.`;
  const t = window.setTimeout(() => {
    emitBrowserEvent("jarvis:chat-event", {
      type: "delta",
      conversationId: agentId,
      text: previewReply,
    });
    emitBrowserEvent("jarvis:chat-event", {
      type: "done",
      conversationId: agentId,
      message: {
        id: crypto.randomUUID(),
        role: "assistant",
        text: previewReply,
        timestamp: new Date().toISOString(),
        mode: "responding",
        tool_events: [],
      },
    });
  }, 550);
  browserTimers.push(t);
  return agentId;
}

export async function cancelAgentTask(agentId: string): Promise<void> {
  if (isTauri) {
    return invoke<void>("cancel_agent_task", { agentId });
  }
}

export async function captureScreen(): Promise<ScreenCapture> {
  if (isTauri) {
    return invoke<ScreenCapture>("capture_screen");
  }
  throw new Error(
    "Captura nativa do desktop disponível no app Windows. Use 'Janela / Aba (Stream)' na prévia web.",
  );
}

export async function fetchBrowserPage(url: string): Promise<BrowserPageContent> {
  if (isTauri) {
    return invoke<BrowserPageContent>("fetch_browser_page", { url });
  }
  return {
    url,
    finalUrl: url,
    title: "Prévia Web",
    text: `Leitura direta de página (${url}) disponível no aplicativo desktop J.A.R.V.I.S.`,
  };
}

export async function openNativeBrowserWindow(
  url: string,
  title?: string,
): Promise<void> {
  if (isTauri) {
    return invoke<void>("open_native_browser_window", {
      url,
      title: title ?? null,
    });
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function browserSyncBounds(
  x: number,
  y: number,
  width: number,
  height: number,
  visible: boolean,
  initialUrl?: string,
): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_sync_bounds", {
    x,
    y,
    width,
    height,
    visible,
    initialUrl: initialUrl ?? null,
  });
}

export async function browserSetVisible(visible: boolean): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("browser_set_visible", { visible });
}

export async function browserStatus(): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_status");
}

export async function browserInspectLive(
  showOverlay = false,
): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_inspect_live", { showOverlay });
}

export async function browserNavigate(url: string): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_navigate", { url });
}

export async function browserSnapshot(showOverlay = false): Promise<BrowserSnapshot> {
  return invoke<BrowserSnapshot>("browser_snapshot", { showOverlay });
}

export async function browserBack(): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_back");
}

export async function browserForward(): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_forward");
}

export async function browserReload(): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_reload");
}

export async function browserClick(opts: {
  elementId?: number;
  text?: string;
  x?: number;
  y?: number;
  showOverlay?: boolean;
}): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_click", {
    elementId: opts.elementId ?? null,
    text: opts.text ?? null,
    x: opts.x ?? null,
    y: opts.y ?? null,
    showOverlay: opts.showOverlay ?? false,
  });
}

export async function browserType(opts: {
  text: string;
  elementId?: number;
  targetLabel?: string;
  clearFirst?: boolean;
  pressEnter?: boolean;
  showOverlay?: boolean;
}): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_type", {
    text: opts.text,
    elementId: opts.elementId ?? null,
    targetLabel: opts.targetLabel ?? null,
    clearFirst: opts.clearFirst ?? false,
    pressEnter: opts.pressEnter ?? false,
    showOverlay: opts.showOverlay ?? false,
  });
}

export async function browserScroll(
  direction: "down" | "up" | "top" | "bottom",
  amount?: number,
  showOverlay = false,
): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_scroll", {
    direction,
    amount: amount ?? null,
    showOverlay,
  });
}

export async function browserPressKey(
  key: string,
  showOverlay = false,
): Promise<BrowserInspectResult> {
  return invoke<BrowserInspectResult>("browser_press_key", { key, showOverlay });
}

export function onBrowserUpdated(
  handler: (ev: BrowserUpdatedEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<BrowserUpdatedEvent>("jarvis:browser-updated", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:browser-updated", handler);
}

export function onAgentSpawned(
  handler: (ev: AgentSpawnedEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<AgentSpawnedEvent>("jarvis:agent-spawned", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:agent-spawned", handler);
}

export interface AgentCompletedSummaryEvent {
  agentId: string;
  agentName: string;
  task: string;
  spokenSummary: string;
  cardSummary: string;
}

export function onAgentCompletedSummary(
  handler: (ev: AgentCompletedSummaryEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<AgentCompletedSummaryEvent>(
      "jarvis:agent-completed-summary",
      (e) => handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:agent-completed-summary", handler);
}

export function onBrowserPanel(
  handler: (ev: BrowserPanelEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<BrowserPanelEvent>("jarvis:browser-panel", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:browser-panel", handler);
}

export function onScreenPulse(
  handler: (ev: ScreenPulseEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<ScreenPulseEvent>("jarvis:screen-pulse", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:screen-pulse", handler);
}

export async function readClipboardContent(): Promise<ClipboardContent> {
  if (isTauri) return invoke<ClipboardContent>("read_clipboard_content");
  try {
    const text = await navigator.clipboard.readText();
    if (text.trim()) return { kind: "text", text };
  } catch {
    /* ignore */
  }
  return { kind: "empty" };
}

export async function revertFileChange(
  path: string,
  existedBefore: boolean,
  oldContent: string,
): Promise<string> {
  if (isTauri) {
    return invoke<string>("revert_file_change", {
      path,
      existedBefore,
      oldContent,
    });
  }
  return `Alteração revertida em ${path}`;
}

export function onFileDiff(
  handler: (ev: FileDiffEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<FileDiffEvent>("jarvis:file-diff", (e) => handler(e.payload));
  }
  return subscribeBrowser("jarvis:file-diff", handler);
}

export async function setMascotVisible(visible: boolean): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("set_mascot_visible", { visible });
}

export async function syncMascotState(payload: MascotStatePayload): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("sync_mascot_state", {
    mode: payload.mode,
    speaking: payload.speaking,
    theme: payload.theme,
    reduced: payload.reduced,
    bubble: payload.bubble ?? null,
    reaction: payload.reaction ?? null,
    muted: payload.muted ?? false,
    wakeWord: payload.wakeWord ?? false,
  });
}

export async function mascotAction(action: MascotActionKind): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("mascot_action", { action });
}

export function onMascotState(
  handler: (ev: MascotStatePayload) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<MascotStatePayload>("jarvis:mascot-state", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:mascot-state", handler);
}

function subscribeBrowser<T>(eventName: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  const listener = (e: Event) => handler((e as CustomEvent<T>).detail);
  bus.addEventListener(eventName, listener);
  return Promise.resolve(() => bus.removeEventListener(eventName, listener));
}

export function onChatEvent(handler: (event: ChatEvent) => void): Promise<UnlistenFn> {
  if (isTauri) return listen<ChatEvent>("jarvis:chat-event", (e) => handler(e.payload));
  return subscribeBrowser("jarvis:chat-event", handler);
}

export function onToolApproval(
  handler: (req: ToolApprovalRequest) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<ToolApprovalRequest>("jarvis:tool-approval-request", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:tool-approval-request", handler);
}

export function onTimerFired(handler: (ev: TimerFiredEvent) => void): Promise<UnlistenFn> {
  if (isTauri) return listen<TimerFiredEvent>("jarvis:timer-fired", (e) => handler(e.payload));
  return subscribeBrowser("jarvis:timer-fired", handler);
}

export interface MicDeviceInfo {
  id: string;
  label: string;
}

export interface NativeRecordedAudio {
  audioBase64: string;
  mimeType: string;
  hadAudio: boolean;
}

export async function listMicrophones(): Promise<MicDeviceInfo[]> {
  if (isTauri) return invoke<MicDeviceInfo[]>("list_microphones");
  return [];
}

export async function startNativeMicRecording(
  vadEnabled: boolean,
  deviceId?: string,
): Promise<void> {
  return invoke<void>("start_native_mic_recording", {
    vadEnabled,
    deviceId: deviceId?.trim() || null,
  });
}

export async function stopNativeMicRecording(): Promise<NativeRecordedAudio> {
  return invoke<NativeRecordedAudio>("stop_native_mic_recording");
}

export async function startNativeMicTest(deviceId?: string): Promise<void> {
  return invoke<void>("start_native_mic_test", {
    deviceId: deviceId?.trim() || null,
  });
}

export async function stopNativeMicTest(): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("stop_native_mic_test");
}

export async function setNativeWakeWord(
  enabled: boolean,
  deviceId?: string,
): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("set_native_wake_word", {
    enabled,
    deviceId: deviceId?.trim() || null,
  });
}

export async function setNativeWakePaused(paused: boolean): Promise<void> {
  if (!isTauri) return;
  return invoke<void>("set_native_wake_paused", { paused });
}

export function onMicLevel(handler: (level: number) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ level: number }>("jarvis:mic-level", (e) => handler(e.payload.level));
  }
  return Promise.resolve(() => {});
}

export function onMicAutoStop(handler: () => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen("jarvis:mic-auto-stop", () => handler());
  }
  return Promise.resolve(() => {});
}

export function onMicError(handler: (error: string) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ error: string }>("jarvis:mic-error", (e) => handler(e.payload.error));
  }
  return Promise.resolve(() => {});
}

export function onMicTestLevel(handler: (level: number) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ level: number }>("jarvis:mic-test-level", (e) =>
      handler(e.payload.level),
    );
  }
  return Promise.resolve(() => {});
}

export function onMicTestError(handler: (error: string) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ error: string }>("jarvis:mic-test-error", (e) =>
      handler(e.payload.error),
    );
  }
  return Promise.resolve(() => {});
}

export function onWakeLevel(handler: (level: number) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ level: number }>("jarvis:wake-level", (e) => handler(e.payload.level));
  }
  return Promise.resolve(() => {});
}

export function onWakeTriggered(
  handler: (ev: { command: string; raw?: string }) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ command: string; raw?: string }>("jarvis:wake-triggered", (e) =>
      handler(e.payload),
    );
  }
  return Promise.resolve(() => {});
}

export function onWakeError(handler: (error: string) => void): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ error: string }>("jarvis:wake-error", (e) => handler(e.payload.error));
  }
  return Promise.resolve(() => {});
}

export function onTrayAction(handler: (ev: TrayActionEvent) => void): Promise<UnlistenFn> {
  if (isTauri) return listen<TrayActionEvent>("jarvis:tray-action", (e) => handler(e.payload));
  return subscribeBrowser("jarvis:tray-action", handler);
}

export interface GitChangeItem {
  status: string;
  path: string;
}

export interface GitRepoStatus {
  isGitRepo: boolean;
  path: string;
  branch: string;
  ahead: number;
  behind: number;
  changes: GitChangeItem[];
}

export async function getGitRepoStatus(path: string): Promise<GitRepoStatus> {
  if (isTauri) {
    return invoke<GitRepoStatus>("get_git_repo_status", { path });
  }
  return {
    isGitRepo: true,
    path: path || "C:\\Workspace\\Jarvis",
    branch: "main",
    ahead: 0,
    behind: 0,
    changes: [],
  };
}

export async function gitCommitWorkspace(
  path: string,
  message: string,
): Promise<string> {
  if (isTauri) {
    return invoke<string>("git_commit_workspace", { path, message });
  }
  return `Commit simulado em main: ${message}`;
}

export async function appendAssistantNote(
  conversationId: string,
  text: string,
): Promise<StoredMessage> {
  if (isTauri) {
    return invoke<StoredMessage>("append_assistant_note", {
      conversationId,
      text,
    });
  }
  return {
    id: `note-${Date.now()}`,
    role: "assistant",
    text,
    timestamp: new Date().toISOString(),
    mode: "responding",
    tool_events: [],
  };
}

export interface TerminalEvent {
  id: string;
  kind: "start" | "stdout" | "stderr" | "exit";
  command?: string;
  cwd?: string;
  line?: string;
  exitCode?: number;
  cancelled?: boolean;
}

export async function runTerminalCommand(
  command: string,
  cwd?: string,
): Promise<string> {
  if (isTauri) {
    return invoke<string>("run_terminal_command", { command, cwd });
  }
  const id = `sim-${Date.now()}`;
  emitBrowserEvent("jarvis:terminal-event", {
    id,
    kind: "start",
    command,
    cwd: cwd ?? "C:\\Workspace\\Jarvis",
  });
  window.setTimeout(() => {
    emitBrowserEvent("jarvis:terminal-event", {
      id,
      kind: "stdout",
      line: `[Simulador Web] Executado: ${command}`,
    });
    emitBrowserEvent("jarvis:terminal-event", {
      id,
      kind: "exit",
      exitCode: 0,
      cancelled: false,
    });
  }, 260);
  return id;
}

export async function cancelTerminalCommand(id: string): Promise<boolean> {
  if (isTauri) {
    return invoke<boolean>("cancel_terminal_command", { id });
  }
  return true;
}

export function onTerminalEvent(
  handler: (ev: TerminalEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<TerminalEvent>("jarvis:terminal-event", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:terminal-event", handler);
}

export interface SkillItem {
  id: string;
  slug: string;
  name: string;
  description: string;
  prompt: string;
  builtin: boolean;
  createdAt: string;
}

export interface RoutineItem {
  id: string;
  name: string;
  scheduleKind: "interval" | "daily";
  intervalMinutes?: number | null;
  dailyHhmm?: string | null;
  prompt: string;
  enabled: boolean;
  lastRunAt?: string | null;
  createdAt: string;
}

export interface RoutineTriggerEvent {
  id: string;
  name: string;
  prompt: string;
}

const SIM_SKILLS: SkillItem[] = [
  {
    id: "builtin-briefing",
    slug: "briefing",
    name: "Briefing do Dia",
    description: "Hora local, saúde do PC e 3 principais notícias do dia.",
    prompt: "Faça um briefing rápido com hora, status do sistema e principais notícias de hoje.",
    builtin: true,
    createdAt: new Date().toISOString(),
  },
  {
    id: "builtin-saude-pc",
    slug: "saude-pc",
    name: "Diagnóstico do PC",
    description: "Inspeciona CPU, RAM, discos e processos pesados.",
    prompt: "Verifique o status do sistema e entregue um diagnóstico rápido.",
    builtin: true,
    createdAt: new Date().toISOString(),
  },
  {
    id: "builtin-foco",
    slug: "foco",
    name: "Modo Foco (Pomodoro)",
    description: "Volume em 25% e timer de foco de 25 minutos.",
    prompt: "Ajuste volume para 25% e crie um timer de 25 minutos para foco.",
    builtin: true,
    createdAt: new Date().toISOString(),
  },
  {
    id: "builtin-git-review",
    slug: "git-review",
    name: "Revisão Git do Projeto",
    description: "Lê git_status + git_diff e sugere mensagem de commit.",
    prompt: "Revise git status e git diff e sugira uma mensagem de commit.",
    builtin: true,
    createdAt: new Date().toISOString(),
  },
];

let simSkillsState = [...SIM_SKILLS];
let simRoutinesState: RoutineItem[] = [];

export async function listSkills(): Promise<SkillItem[]> {
  if (isTauri) return invoke<SkillItem[]>("list_skills");
  return [...simSkillsState];
}

export async function saveSkill(
  slug: string,
  name: string,
  description: string,
  prompt: string,
): Promise<SkillItem> {
  if (isTauri) {
    return invoke<SkillItem>("save_skill", { slug, name, description, prompt });
  }
  const clean = slug.trim().replace(/^\/+/, "").toLowerCase();
  const item: SkillItem = {
    id: `sim-skill-${Date.now()}`,
    slug: clean,
    name: name.trim() || clean,
    description: description.trim(),
    prompt: prompt.trim(),
    builtin: false,
    createdAt: new Date().toISOString(),
  };
  simSkillsState = [...simSkillsState.filter((s) => s.slug !== clean), item];
  return item;
}

export async function deleteSkill(idOrSlug: string): Promise<boolean> {
  if (isTauri) return invoke<boolean>("delete_skill", { idOrSlug });
  simSkillsState = simSkillsState.filter(
    (s) => s.id !== idOrSlug && s.slug !== idOrSlug,
  );
  return true;
}

export async function listRoutines(): Promise<RoutineItem[]> {
  if (isTauri) return invoke<RoutineItem[]>("list_routines");
  return [...simRoutinesState];
}

export async function createRoutine(
  name: string,
  scheduleKind: "interval" | "daily",
  intervalMinutes: number | undefined,
  dailyHhmm: string | undefined,
  prompt: string,
): Promise<RoutineItem> {
  if (isTauri) {
    return invoke<RoutineItem>("create_routine", {
      name,
      scheduleKind,
      intervalMinutes,
      dailyHhmm,
      prompt,
    });
  }
  const item: RoutineItem = {
    id: `sim-routine-${Date.now()}`,
    name: name.trim(),
    scheduleKind,
    intervalMinutes: scheduleKind === "interval" ? (intervalMinutes ?? 60) : null,
    dailyHhmm: scheduleKind === "daily" ? (dailyHhmm ?? "09:00") : null,
    prompt: prompt.trim(),
    enabled: true,
    lastRunAt: null,
    createdAt: new Date().toISOString(),
  };
  simRoutinesState = [...simRoutinesState, item];
  return item;
}

export async function toggleRoutine(
  id: string,
  enabled: boolean,
): Promise<RoutineItem> {
  if (isTauri) return invoke<RoutineItem>("toggle_routine", { id, enabled });
  simRoutinesState = simRoutinesState.map((r) =>
    r.id === id ? { ...r, enabled } : r,
  );
  return simRoutinesState.find((r) => r.id === id)!;
}

export async function deleteRoutine(id: string): Promise<boolean> {
  if (isTauri) return invoke<boolean>("delete_routine", { id });
  simRoutinesState = simRoutinesState.filter((r) => r.id !== id);
  return true;
}

export function onRoutineTrigger(
  handler: (ev: RoutineTriggerEvent) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<RoutineTriggerEvent>("jarvis:routine-trigger", (e) =>
      handler(e.payload),
    );
  }
  return subscribeBrowser("jarvis:routine-trigger", handler);
}

export interface CompanionInfo {
  enabled: boolean;
  lanEnabled: boolean;
  port: number;
  ip: string;
  bindHost: string;
  pairUrl: string;
  pairingCode: string;
  connectedClients: number;
}

let simCompanionEnabled = false;
let simCompanionLan = false;
let simCompanionCode = "DEMO4821";

export async function getCompanionInfo(): Promise<CompanionInfo> {
  if (isTauri) return invoke<CompanionInfo>("get_companion_info");
  const ip = simCompanionEnabled && simCompanionLan ? "192.168.1.100" : "127.0.0.1";
  return {
    enabled: simCompanionEnabled,
    lanEnabled: simCompanionLan,
    port: 47821,
    ip,
    bindHost: simCompanionEnabled && simCompanionLan ? "0.0.0.0" : "127.0.0.1",
    pairUrl: `http://${ip}:47821/?pair=${simCompanionCode}`,
    pairingCode: simCompanionCode,
    connectedClients: 0,
  };
}

export async function setCompanionEnabled(enabled: boolean): Promise<CompanionInfo> {
  if (isTauri) return invoke<CompanionInfo>("set_companion_enabled", { enabled });
  simCompanionEnabled = enabled;
  return getCompanionInfo();
}

export async function setCompanionLanEnabled(lanEnabled: boolean): Promise<CompanionInfo> {
  if (isTauri) return invoke<CompanionInfo>("set_companion_lan_enabled", { lanEnabled });
  simCompanionLan = lanEnabled;
  return getCompanionInfo();
}

export async function regenerateCompanionToken(): Promise<CompanionInfo> {
  if (isTauri) return invoke<CompanionInfo>("regenerate_companion_token");
  simCompanionCode = Math.random().toString(36).slice(2, 10).toUpperCase();
  return getCompanionInfo();
}

export async function allowCompanionFirewall(): Promise<string> {
  if (isTauri) return invoke<string>("allow_companion_firewall");
  return "Porta liberada no Firewall (simulação web).";
}

export function onCompanionSync(
  handler: (ev: { conversationId: string }) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ conversationId: string }>("jarvis:companion-sync", (e) =>
      handler(e.payload),
    );
  }
  return Promise.resolve(() => {});
}

export function onApprovalResolved(
  handler: (ev: { approvalId: string; approved: boolean }) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return listen<{ approvalId: string; approved: boolean }>(
      "jarvis:approval-resolved",
      (e) => handler(e.payload),
    );
  }
  return Promise.resolve(() => {});
}

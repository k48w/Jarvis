/**
 * # Gavetas Laterais de Conversas, Base RAG e Ajustes (`src/Drawers.tsx`)
 *
 * Contém dois componentes principais do cockpit:
 * - `HistoryDrawer`: Gerencia Conversas, Projetos, Memórias de longo prazo e a
 *   **Base de Conhecimento Local (RAG)** (indexação de pastas/arquivos e busca BM25).
 * - `SettingsDrawer`: Configurações de IA (ChatGPT OAuth / Gemini), Voz (ElevenLabs /
 *   Wake Word), Sistema (Permissões, Atalho Global e **Auto-Updater GitHub Releases**),
 *   Visual (Temas e Qualidade 3D) e Celular (**Jarvis Companion** via LAN, Tailscale ou Túnel HTTPS).
 */
import { useEffect, useRef, useState } from "react";
import QRCode from "qrcode";
import {
  Check,
  Copy,
  Download,
  Folder,
  FolderPlus,
  Loader2,
  LogIn,
  LogOut,
  MessageSquare,
  Mic,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Smartphone,
  Trash2,
  Upload,
  Volume2,
  X,
} from "lucide-react";
import { listAudioInputDevices, openMicStream } from "./voice";
import {
  allowCompanionFirewall,
  checkAppUpdate,
  getCompanionInfo,
  indexKnowledgePath,
  installAppUpdate,
  isTauri,
  listKnowledgeSources,
  listMicrophones,
  onMicTestError,
  onMicTestLevel,
  onUpdateProgress,
  pickProjectFolder,
  regenerateCompanionToken,
  removeKnowledgeSource,
  searchKnowledgeBase,
  setCompanionEnabled,
  setCompanionExternalUrl,
  setCompanionLanEnabled,
  startNativeMicTest,
  stopNativeMicTest,
  type AllKeyStatus,
  type AppConfig,
  type ChatGptAuthStatus,
  type CompanionInfo,
  type ConnectionTestResult,
  type ConversationSummary,
  type ElevenLabsVoice,
  type GeminiAuthMode,
  type GoogleAuthStatus,
  type KeySlot,
  type KnowledgeHit,
  type KnowledgeSourceSummary,
  type MemoryItem,
  type PermissionMode,
  type ProjectItem,
  type Provider,
  type StoredMessage,
  type SttEngine,
  type TtsEngine,
  type TtsReplyMode,
  type UpdateCheckResult,
  type UpdateProgressEvent,
} from "./bridge";
import type { VisualTheme } from "./Stage";

export type Quality = "Low" | "High" | "Ultra";
export type HistoryTab = "conversations" | "memory" | "knowledge";
export type SettingsTab = "ai" | "voice" | "system" | "visual" | "mobile";

interface HistoryDrawerProps {
  tab: HistoryTab;
  onTabChange: (tab: HistoryTab) => void;
  conversations: ConversationSummary[];
  projects: ProjectItem[];
  activeConversationId: string;
  activeProjectId: string | null;
  messages: StoredMessage[];
  memories: MemoryItem[];
  onSelectConversation: (id: string) => void;
  onCreateConversation: (projectId?: string | null) => void;
  onRenameConversation: (id: string, title: string) => Promise<void>;
  onDeleteConversation: (id: string) => void;
  onSetConversationProject: (convId: string, projectId: string | null) => Promise<void>;
  onPickAndAddProject: () => Promise<string | null>;
  onAddProjectPath: (path: string) => Promise<string | null>;
  onDeleteProject: (id: string) => Promise<void>;
  onAddMemory: (content: string) => Promise<void>;
  onDeleteMemory: (id: string) => Promise<void>;
  onClose: () => void;
}

export function HistoryDrawer({
  tab,
  onTabChange,
  conversations,
  projects,
  activeConversationId,
  activeProjectId,
  messages,
  memories,
  onSelectConversation,
  onCreateConversation,
  onRenameConversation,
  onDeleteConversation,
  onSetConversationProject,
  onPickAndAddProject,
  onAddProjectPath,
  onDeleteProject,
  onAddMemory,
  onDeleteMemory,
  onClose,
}: HistoryDrawerProps) {
  const [memoryDraft, setMemoryDraft] = useState("");
  const [showPathInput, setShowPathInput] = useState(false);
  const [projectPathDraft, setProjectPathDraft] = useState("");
  const [projectErr, setProjectErr] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [convSearch, setConvSearch] = useState("");

  // Base de Conhecimento Local (RAG)
  const [kbSources, setKbSources] = useState<KnowledgeSourceSummary[]>([]);
  const [kbPathDraft, setKbPathDraft] = useState("");
  const [kbQueryDraft, setKbQueryDraft] = useState("");
  const [kbHits, setKbHits] = useState<KnowledgeHit[]>([]);
  const [kbBusy, setKbBusy] = useState(false);
  const [kbStatus, setKbStatus] = useState<string | null>(null);

  useEffect(() => {
    void listKnowledgeSources()
      .then(setKbSources)
      .catch(() => {});
  }, [tab]);

  async function handleIndexFolderPicker() {
    setKbStatus(null);
    setKbBusy(true);
    try {
      const picked = await pickProjectFolder();
      if (!picked) {
        setKbBusy(false);
        return;
      }
      const sum = await indexKnowledgePath(picked);
      const next = await listKnowledgeSources();
      setKbSources(next);
      setKbStatus(
        `Indexado '${sum.name}': ${sum.fileCount} arquivo(s), ${sum.chunkCount} blocos.`,
      );
    } catch (err) {
      setKbStatus(String(err));
    } finally {
      setKbBusy(false);
    }
  }

  async function handleIndexManualKbPath(e: React.FormEvent) {
    e.preventDefault();
    const clean = kbPathDraft.trim();
    if (!clean) return;
    setKbStatus(null);
    setKbBusy(true);
    try {
      const sum = await indexKnowledgePath(clean);
      setKbPathDraft("");
      const next = await listKnowledgeSources();
      setKbSources(next);
      setKbStatus(
        `Indexado '${sum.name}': ${sum.fileCount} arquivo(s), ${sum.chunkCount} blocos.`,
      );
    } catch (err) {
      setKbStatus(String(err));
    } finally {
      setKbBusy(false);
    }
  }

  async function handleSearchKb(e: React.FormEvent) {
    e.preventDefault();
    const q = kbQueryDraft.trim();
    if (!q) {
      setKbHits([]);
      return;
    }
    setKbBusy(true);
    setKbStatus(null);
    try {
      const results = await searchKnowledgeBase(q, 6);
      setKbHits(results);
      if (results.length === 0) {
        setKbStatus(`Nenhum trecho encontrado para "${q}".`);
      }
    } catch (err) {
      setKbStatus(String(err));
    } finally {
      setKbBusy(false);
    }
  }

  async function handleRemoveKbSource(id: string) {
    setKbBusy(true);
    setKbStatus(null);
    try {
      await removeKnowledgeSource(id);
      const next = await listKnowledgeSources();
      setKbSources(next);
      setKbHits((prev) => prev.filter((h) => h.sourceId !== id));
    } catch (err) {
      setKbStatus(String(err));
    } finally {
      setKbBusy(false);
    }
  }

  async function submitMemory(e: React.FormEvent) {
    e.preventDefault();
    const clean = memoryDraft.trim();
    if (!clean) return;
    setMemoryDraft("");
    await onAddMemory(clean);
  }

  async function handlePickFolder() {
    setProjectErr(null);
    const err = await onPickAndAddProject();
    if (err === "MANUAL") {
      setShowPathInput(true);
    } else if (err) {
      setProjectErr(err);
    }
  }

  async function handleAddManualPath(e: React.FormEvent) {
    e.preventDefault();
    const clean = projectPathDraft.trim();
    if (!clean) return;
    setProjectErr(null);
    const err = await onAddProjectPath(clean);
    if (err) {
      setProjectErr(err);
    } else {
      setProjectPathDraft("");
      setShowPathInput(false);
    }
  }

  async function submitRename(id: string) {
    const clean = renameDraft.trim();
    setRenamingId(null);
    if (clean) {
      await onRenameConversation(id, clean);
    }
  }

  function exportConversationMarkdown() {
    if (messages.length === 0) return;
    const activeConv = conversations.find((c) => c.id === activeConversationId);
    const title = activeConv?.title ?? "Conversa J.A.R.V.I.S.";
    const lines = [
      `# ${title}`,
      `> Exportado do **J.A.R.V.I.S. V8.0 — The Singularity Edition** em ${new Date().toLocaleString("pt-BR")}`,
      "",
    ];
    for (const m of messages) {
      const author = m.role === "user" ? "🧑 Você" : "🤖 J.A.R.V.I.S.";
      lines.push(`### ${author}`);
      if (m.tool_events.length > 0) {
        lines.push(
          `*Ferramentas:* ${m.tool_events.map((t) => `\`${t.title}\``).join(", ")}`,
        );
      }
      lines.push("", m.text, "");
    }
    const blob = new Blob([lines.join("\n")], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${title.replace(/[^a-zA-Z0-9_-]+/g, "-").toLowerCase() || "jarvis-chat"}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const searchNeedle = convSearch.trim().toLowerCase();
  const filteredConvs = searchNeedle
    ? conversations.filter(
        (c) =>
          c.title.toLowerCase().includes(searchNeedle) ||
          c.preview.toLowerCase().includes(searchNeedle),
      )
    : conversations;

  const projectIds = new Set(projects.map((p) => p.id));
  const recentConvs = filteredConvs.filter(
    (c) => !c.project_id || !projectIds.has(c.project_id),
  );

  function renderConvRow(c: ConversationSummary) {
    const isActive = c.id === activeConversationId;
    if (renamingId === c.id) {
      return (
        <form
          key={c.id}
          className="conv-rename-form"
          onSubmit={(e) => {
            e.preventDefault();
            void submitRename(c.id);
          }}
        >
          <input
            type="text"
            autoFocus
            value={renameDraft}
            aria-label="Novo título da conversa"
            onChange={(e) => setRenameDraft(e.target.value)}
            onBlur={() => void submitRename(c.id)}
          />
          <button type="submit" className="icon-tiny" aria-label="Confirmar nome">
            <Check />
          </button>
        </form>
      );
    }

    return (
      <div
        key={c.id}
        className={isActive ? "conv-row active" : "conv-row"}
      >
        <button
          type="button"
          className="conv-row-btn"
          onClick={() => onSelectConversation(c.id)}
        >
          <MessageSquare />
          <span className="conv-row-title">{c.title}</span>
          <small>{c.message_count}</small>
        </button>
        <div className="conv-row-actions">
          <button
            type="button"
            className="icon-tiny"
            aria-label={`Renomear ${c.title}`}
            title="Renomear conversa"
            onClick={() => {
              setRenamingId(c.id);
              setRenameDraft(c.title);
            }}
          >
            <Pencil />
          </button>
          {conversations.length > 1 && (
            <button
              type="button"
              className="icon-tiny"
              aria-label={`Excluir ${c.title}`}
              title="Excluir conversa"
              onClick={() => onDeleteConversation(c.id)}
            >
              <Trash2 />
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="transcript drawer drawer-wide" role="dialog" aria-label="Conversas, Projetos e Memória">
      <div className="settings-head">
        <h2>Conversas &amp; Projetos</h2>
        <button aria-label="Fechar histórico" onClick={onClose}>
          <X />
        </button>
      </div>

      <div className="tabs" role="tablist">
        <button
          role="tab"
          aria-selected={tab === "conversations"}
          className={tab === "conversations" ? "tab active" : "tab"}
          onClick={() => onTabChange("conversations")}
        >
          Conversas ({conversations.length})
        </button>
        <button
          role="tab"
          aria-selected={tab === "memory"}
          className={tab === "memory" ? "tab active" : "tab"}
          onClick={() => onTabChange("memory")}
        >
          Memória ({memories.length})
        </button>
        <button
          role="tab"
          aria-selected={tab === "knowledge"}
          className={tab === "knowledge" ? "tab active" : "tab"}
          onClick={() => onTabChange("knowledge")}
        >
          Base RAG ({kbSources.length})
        </button>
      </div>

      {tab === "conversations" ? (
        <div className="drawer-body">
          <div className="conv-top-actions">
            <button
              type="button"
              className="mini-btn accent"
              onClick={() => onCreateConversation(null)}
            >
              <Plus /> Nova conversa
            </button>
            <button
              type="button"
              className="mini-btn"
              onClick={() => void handlePickFolder()}
            >
              <FolderPlus /> Adicionar projeto
            </button>
            {messages.length > 0 && (
              <button
                type="button"
                className="mini-btn"
                title="Exportar conversa atual como Markdown (.md)"
                onClick={exportConversationMarkdown}
              >
                <Download /> .md
              </button>
            )}
          </div>

          {showPathInput && (
            <form className="memory-form" onSubmit={(e) => void handleAddManualPath(e)}>
              <input
                type="text"
                value={projectPathDraft}
                placeholder="C:\Users\...\meu-projeto"
                aria-label="Caminho do projeto"
                onChange={(e) => setProjectPathDraft(e.target.value)}
              />
              <button type="submit" className="mini-btn accent">
                Adicionar
              </button>
            </form>
          )}

          {projectErr && <small className="warn-tag">{projectErr}</small>}

          {conversations.length > 2 && (
            <input
              type="search"
              value={convSearch}
              placeholder="Filtrar conversas por título ou conteúdo…"
              aria-label="Filtrar conversas"
              onChange={(e) => setConvSearch(e.target.value)}
            />
          )}

          <div className="codex-sidebar-lists">
            {projects.length > 0 && (
              <div className="project-section">
                <div className="section-kicker">Projetos</div>
                {projects.map((proj) => {
                  const projConvs = filteredConvs.filter((c) => c.project_id === proj.id);
                  return (
                    <div key={proj.id} className="project-group">
                      <div className="project-row" title={proj.path}>
                        <Folder />
                        <strong>{proj.name}</strong>
                        <div className="project-row-actions">
                          <button
                            type="button"
                            className="icon-tiny"
                            title={`Nova conversa em ${proj.name}`}
                            aria-label={`Nova conversa em ${proj.name}`}
                            onClick={() => onCreateConversation(proj.id)}
                          >
                            <Plus />
                          </button>
                          <button
                            type="button"
                            className="icon-tiny"
                            title={`Remover projeto ${proj.name}`}
                            aria-label={`Remover projeto ${proj.name}`}
                            onClick={() => void onDeleteProject(proj.id)}
                          >
                            <Trash2 />
                          </button>
                        </div>
                      </div>
                      <div className="project-convs">
                        {projConvs.length === 0 ? (
                          <button
                            type="button"
                            className="project-empty-add"
                            onClick={() => onCreateConversation(proj.id)}
                          >
                            + Iniciar conversa neste projeto
                          </button>
                        ) : (
                          projConvs.map(renderConvRow)
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}

            <div className="project-section">
              <div className="section-kicker">Recentes</div>
              <div className="recent-convs">
                {recentConvs.length === 0 ? (
                  <p className="empty-hint">Todas as conversas estão organizadas nos projetos acima.</p>
                ) : (
                  recentConvs.map(renderConvRow)
                )}
              </div>
            </div>
          </div>

          {projects.length > 0 && activeConversationId && (
            <label className="conv-project-bar">
              <span>Projeto desta conversa</span>
              <select
                aria-label="Projeto vinculado à conversa atual"
                value={activeProjectId ?? ""}
                onChange={(e) =>
                  void onSetConversationProject(
                    activeConversationId,
                    e.target.value || null,
                  )
                }
              >
                <option value="">Sem projeto (geral)</option>
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
          )}

          <div
            className="messages"
            role="log"
            aria-label="Histórico da conversa"
          >
            {messages.length === 0 ? (
              <p className="empty-hint">
                Nenhuma mensagem nesta conversa ainda. Escreva ou use o microfone abaixo.
              </p>
            ) : (
              messages.map((m) => (
                <div
                  key={m.id}
                  className={m.role === "user" ? "message user" : "message"}
                >
                  <small>{m.role === "user" ? "Você" : "J.A.R.V.I.S."}</small>
                  {m.tool_events.length > 0 && (
                    <div className="tool-chips">
                      {m.tool_events.map((t) => (
                        <span key={t.id} className={`tool-chip ${t.status}`}>
                          {t.title}
                        </span>
                      ))}
                    </div>
                  )}
                  <p>{m.text}</p>
                </div>
              ))
            )}
          </div>
        </div>
      ) : tab === "memory" ? (
        <div className="drawer-body">
          <form className="memory-form" onSubmit={(e) => void submitMemory(e)}>
            <input
              type="text"
              value={memoryDraft}
              placeholder="Ex.: Meu nome é Lucas, gosto de respostas curtas"
              aria-label="Nova memória"
              onChange={(e) => setMemoryDraft(e.target.value)}
            />
            <button type="submit" className="mini-btn accent">
              Salvar
            </button>
          </form>
          <div className="memory-list">
            {memories.length === 0 ? (
              <p className="empty-hint">
                Nenhuma memória salva. O J.A.R.V.I.S. também pode salvar fatos automaticamente durante a conversa.
              </p>
            ) : (
              memories.map((m) => (
                <div key={m.id} className="memory-item">
                  <span>{m.content}</span>
                  <button
                    type="button"
                    className="icon-tiny"
                    aria-label="Remover memória"
                    onClick={() => void onDeleteMemory(m.id)}
                  >
                    <Trash2 />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      ) : (
        <div className="drawer-body">
          <div className="conv-top-actions">
            <button
              type="button"
              className="mini-btn accent"
              disabled={kbBusy}
              onClick={() => void handleIndexFolderPicker()}
            >
              <FolderPlus /> Indexar pasta local
            </button>
          </div>

          <form className="memory-form" onSubmit={(e) => void handleIndexManualKbPath(e)}>
            <input
              type="text"
              value={kbPathDraft}
              placeholder="Caminho de pasta ou arquivo (ex: C:\Projetos\meu-app)"
              aria-label="Caminho para indexar na Base RAG"
              onChange={(e) => setKbPathDraft(e.target.value)}
            />
            <button type="submit" className="mini-btn accent" disabled={kbBusy}>
              Indexar
            </button>
          </form>

          <form className="memory-form" onSubmit={(e) => void handleSearchKb(e)}>
            <input
              type="search"
              value={kbQueryDraft}
              placeholder="Pesquisar nos arquivos indexados (BM25 + código)…"
              aria-label="Pesquisar na Base RAG"
              onChange={(e) => setKbQueryDraft(e.target.value)}
            />
            <button type="submit" className="mini-btn" disabled={kbBusy}>
              <Search size={14} /> Buscar
            </button>
          </form>

          {kbStatus && <small className="hint-line">{kbStatus}</small>}

          {kbHits.length > 0 && (
            <div className="project-section">
              <div className="section-kicker">
                Trechos Encontrados ({kbHits.length})
              </div>
              <div className="memory-list">
                {kbHits.map((hit) => (
                  <div
                    key={hit.chunkId}
                    className="memory-item"
                    style={{ flexDirection: "column", alignItems: "stretch", gap: "6px" }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px" }}>
                      <strong>
                        {hit.relativePath} (L{hit.startLine}–L{hit.endLine})
                      </strong>
                      <span>Score {hit.score.toFixed(2)}</span>
                    </div>
                    <pre
                      style={{
                        margin: 0,
                        padding: "8px",
                        borderRadius: "8px",
                        background: "rgba(0,0,0,0.35)",
                        fontSize: "11.5px",
                        whiteSpace: "pre-wrap",
                        maxHeight: "140px",
                        overflowY: "auto",
                      }}
                    >
                      {hit.snippet}
                    </pre>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="project-section">
            <div className="section-kicker">
              Fontes Indexadas ({kbSources.length})
            </div>
            <div className="memory-list">
              {kbSources.length === 0 ? (
                <p className="empty-hint">
                  Nenhuma pasta ou documento indexado. Indexe um projeto acima ou peça no chat para o Jarvis consultar seus arquivos via RAG.
                </p>
              ) : (
                kbSources.map((src) => (
                  <div key={src.id} className="memory-item" title={src.path}>
                    <div style={{ display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 }}>
                      <strong>{src.name}</strong>
                      <small style={{ opacity: 0.75, fontSize: "11.5px" }}>
                        {src.fileCount} arquivo(s) · {src.chunkCount} chunks · {src.path}
                      </small>
                    </div>
                    <button
                      type="button"
                      className="icon-tiny"
                      aria-label={`Remover ${src.name} da Base RAG`}
                      title="Remover da Base RAG"
                      disabled={kbBusy}
                      onClick={() => void handleRemoveKbSource(src.id)}
                    >
                      <Trash2 />
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface SettingsDrawerProps {
  tab: SettingsTab;
  onTabChange: (tab: SettingsTab) => void;
  config: AppConfig;
  keyStatus: AllKeyStatus;
  chatgptAuth: ChatGptAuthStatus;
  chatgptModels: string[];
  googleAuth: GoogleAuthStatus;
  elevenVoices: ElevenLabsVoice[];
  quality: Quality;
  theme: VisualTheme;
  volume: number;
  reduced: boolean;
  showMascot: boolean;
  onUpdateConfig: (patch: Partial<AppConfig>) => Promise<void>;
  onSaveKey: (slot: KeySlot, value: string) => Promise<void>;
  onClearKey: (slot: KeySlot) => Promise<void>;
  onChatgptLogin: (forceRegister?: boolean) => Promise<string | null>;
  onChatgptLogout: (resetRegistration?: boolean) => Promise<void>;
  onRefreshChatgptModels: () => Promise<string | null>;
  onImportGoogleJson: (jsonText: string) => Promise<string | null>;
  onGoogleLogin: () => Promise<string | null>;
  onGoogleLogout: () => Promise<void>;
  onRefreshElevenVoices: () => Promise<string | null>;
  onTestConnection: () => Promise<ConnectionTestResult>;
  onTestVoice: () => Promise<void>;
  onUpdateShortcut: (shortcut: string) => Promise<string | null>;
  onQualityChange: (q: Quality) => void;
  onThemeChange: (t: VisualTheme) => void;
  onVolumeChange: (v: number) => void;
  onReducedChange: (r: boolean) => void;
  onShowMascotChange: (v: boolean) => void;
  onClose: () => void;
}

const VISUAL_THEMES: Array<{ id: VisualTheme; label: string; color: string }> = [
  { id: "cyan", label: "Stark Cyan", color: "#6ce8ff" },
  { id: "gold", label: "Arc Gold", color: "#ffc95c" },
  { id: "violet", label: "Quantum Violet", color: "#b68cff" },
  { id: "crimson", label: "Crimson Core", color: "#ff5c72" },
];

const GEMINI_PRESETS = ["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash"];
const CHATGPT_PRESETS = ["gpt-6.2-sol", "gpt-6.2-astra", "gpt-6.1-sol", "gpt-6-luna"];

const ELEVENLABS_VOICE_PRESETS: Array<{ id: string; label: string }> = [
  { id: "JBFqnCBsd6RMkjVDRZzb", label: "George (Grave / Britânico)" },
  { id: "onwK4e9ZLuTAKqWW03F9", label: "Daniel (Calmo / Estúdio)" },
  { id: "nPczCjzI2devNBz1zQrb", label: "Brian (Profundo)" },
  { id: "TX3LPaxmHKxFdv7VOQHJ", label: "Liam (Natural)" },
];

export function SettingsDrawer({
  tab,
  onTabChange,
  config,
  keyStatus,
  chatgptAuth,
  chatgptModels,
  googleAuth,
  elevenVoices,
  quality,
  theme,
  volume,
  reduced,
  showMascot,
  onUpdateConfig,
  onSaveKey,
  onClearKey,
  onChatgptLogin,
  onChatgptLogout,
  onRefreshChatgptModels,
  onImportGoogleJson,
  onGoogleLogin,
  onGoogleLogout,
  onRefreshElevenVoices,
  onTestConnection,
  onTestVoice,
  onUpdateShortcut,
  onQualityChange,
  onThemeChange,
  onVolumeChange,
  onReducedChange,
  onShowMascotChange,
  onClose,
}: SettingsDrawerProps) {
  const [geminiKeyInput, setGeminiKeyInput] = useState("");
  const [googleSecretInput, setGoogleSecretInput] = useState("");
  const [elevenKeyInput, setElevenKeyInput] = useState("");
  const [shortcutDraft, setShortcutDraft] = useState(config.global_shortcut);
  const [shortcutStatus, setShortcutStatus] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [oauthError, setOauthError] = useState<string | null>(null);
  const [googleLoading, setGoogleLoading] = useState(false);
  const [googleError, setGoogleError] = useState<string | null>(null);
  const [loadingChatgptModels, setLoadingChatgptModels] = useState(false);
  const [loadingElevenVoices, setLoadingElevenVoices] = useState(false);
  const [elevenError, setElevenError] = useState<string | null>(null);
  const [micDevices, setMicDevices] = useState<Array<{ deviceId: string; label: string }>>([]);
  const [testingMic, setTestingMic] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [micStatus, setMicStatus] = useState<string | null>(null);
  const micTestCleanupRef = useRef<(() => void) | null>(null);
  const googleFileRef = useRef<HTMLInputElement>(null);

  const [companionInfo, setCompanionInfo] = useState<CompanionInfo | null>(null);
  const [companionQr, setCompanionQr] = useState<string>("");
  const [companionBusy, setCompanionBusy] = useState(false);
  const [companionMsg, setCompanionMsg] = useState<string | null>(null);
  const [copiedMobileUrl, setCopiedMobileUrl] = useState(false);
  const [companionRoute, setCompanionRoute] = useState<"lan" | "tailscale" | "external">("lan");
  const [externalUrlDraft, setExternalUrlDraft] = useState("");

  // Auto-Updater (GitHub Releases)
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgressEvent | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void onUpdateProgress((ev) => {
      if (!disposed) setUpdateProgress(ev);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  function resolveActivePairUrl(
    info: CompanionInfo | null,
    route: "lan" | "tailscale" | "external",
  ): string {
    if (!info) return "";
    if (route === "tailscale" && info.tailscalePairUrl) {
      return info.tailscalePairUrl;
    }
    if (route === "external" && info.externalPairUrl) {
      return info.externalPairUrl;
    }
    return info.pairUrl;
  }

  useEffect(() => {
    if (tab !== "mobile") return;
    let active = true;
    const refresh = () => {
      void getCompanionInfo()
        .then(async (info) => {
          if (!active) return;
          setCompanionInfo(info);
          setExternalUrlDraft((prev) => prev || info.externalUrl || "");
          const targetUrl = resolveActivePairUrl(info, companionRoute);
          const dataUrl =
            info.enabled && (info.lanEnabled || companionRoute === "external")
              ? await QRCode.toDataURL(targetUrl, {
                  width: 210,
                  margin: 2,
                  color: { dark: "#050811", light: "#ffffff" },
                }).catch(() => "")
              : "";
          if (active) setCompanionQr(dataUrl);
        })
        .catch(() => {});
    };
    refresh();
    const id = window.setInterval(refresh, 3000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [tab, companionRoute]);

  useEffect(() => {
    if (tab !== "voice") return;
    if (isTauri) {
      void listMicrophones()
        .then((devs) =>
          setMicDevices(devs.map((d) => ({ deviceId: d.id, label: d.label }))),
        )
        .catch(() => {});
    } else {
      void listAudioInputDevices().then(setMicDevices).catch(() => {});
    }
  }, [tab]);

  useEffect(() => {
    return () => {
      micTestCleanupRef.current?.();
      micTestCleanupRef.current = null;
      if (isTauri) void stopNativeMicTest().catch(() => {});
    };
  }, []);

  async function toggleMicTest() {
    if (testingMic) {
      micTestCleanupRef.current?.();
      micTestCleanupRef.current = null;
      if (isTauri) void stopNativeMicTest().catch(() => {});
      setTestingMic(false);
      setMicLevel(0);
      return;
    }
    setMicStatus(null);
    setTestingMic(true);

    if (isTauri) {
      try {
        const devs = await listMicrophones();
        setMicDevices(devs.map((d) => ({ deviceId: d.id, label: d.label })));
        let peak = 0;
        const unLvl = await onMicTestLevel((lvl) => {
          setMicLevel(lvl);
          if (lvl > peak) {
            peak = lvl;
            if (peak > 0.1) {
              setMicStatus("Microfone captando sua voz perfeitamente!");
            }
          }
        });
        const unErr = await onMicTestError((err) => {
          setMicStatus(`Erro no microfone: ${err}`);
          setTestingMic(false);
        });
        await startNativeMicTest(config.mic_device_id);
        micTestCleanupRef.current = () => {
          unLvl();
          unErr();
          void stopNativeMicTest().catch(() => {});
        };
      } catch (err) {
        setTestingMic(false);
        setMicLevel(0);
        setMicStatus(
          err instanceof Error
            ? `Erro no microfone: ${err.message}`
            : "Não foi possível abrir o microfone.",
        );
      }
      return;
    }

    try {
      const stream = await openMicStream(config.mic_device_id);
      const devices = await listAudioInputDevices();
      setMicDevices(devices);

      const ctx = new AudioContext();
      if (ctx.state === "suspended") await ctx.resume();
      const src = ctx.createMediaStreamSource(stream);
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      src.connect(analyser);
      const data = new Uint8Array(analyser.frequencyBinCount);
      let raf = 0;
      let peak = 0;
      const tick = () => {
        analyser.getByteTimeDomainData(data);
        let sum = 0;
        for (let i = 0; i < data.length; i++) {
          const v = (data[i] - 128) / 128;
          sum += v * v;
        }
        const rms = Math.sqrt(sum / data.length);
        const level = Math.min(1, rms * 8);
        setMicLevel(level);
        if (level > peak) {
          peak = level;
          if (peak > 0.12) {
            setMicStatus("Microfone captando sua voz perfeitamente!");
          }
        }
        raf = requestAnimationFrame(tick);
      };
      raf = requestAnimationFrame(tick);

      micTestCleanupRef.current = () => {
        cancelAnimationFrame(raf);
        src.disconnect();
        stream.getTracks().forEach((t) => t.stop());
        void ctx.close().catch(() => {});
      };
    } catch (err) {
      setTestingMic(false);
      setMicLevel(0);
      setMicStatus(
        err instanceof Error
          ? `Erro no microfone: ${err.message}`
          : "Não foi possível abrir o microfone.",
      );
    }
  }

  async function handleGoogleJsonFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    setGoogleError(null);
    const text = await file.text();
    const err = await onImportGoogleJson(text);
    if (err) setGoogleError(err);
  }

  async function handleGoogleLogin() {
    setGoogleLoading(true);
    setGoogleError(null);
    setTestResult(null);
    try {
      const err = await onGoogleLogin();
      if (err) setGoogleError(err);
    } finally {
      setGoogleLoading(false);
    }
  }

  async function handleChatgptLogin(forceRegister = false) {
    setOauthLoading(true);
    setOauthError(null);
    setTestResult(null);
    try {
      const err = await onChatgptLogin(forceRegister);
      if (err) setOauthError(err);
    } finally {
      setOauthLoading(false);
    }
  }

  async function handleRefreshChatgptModels() {
    setLoadingChatgptModels(true);
    setOauthError(null);
    try {
      const err = await onRefreshChatgptModels();
      if (err) setOauthError(err);
    } finally {
      setLoadingChatgptModels(false);
    }
  }

  async function handleRefreshEleven() {
    setLoadingElevenVoices(true);
    setElevenError(null);
    try {
      const err = await onRefreshElevenVoices();
      if (err) setElevenError(err);
    } finally {
      setLoadingElevenVoices(false);
    }
  }

  async function handleTest() {
    setTesting(true);
    setTestResult(null);
    try {
      const res = await onTestConnection();
      setTestResult(res);
    } finally {
      setTesting(false);
    }
  }

  async function handleApplyShortcut() {
    const err = await onUpdateShortcut(shortcutDraft);
    setShortcutStatus(err ?? "Atalho atualizado.");
  }

  return (
    <aside className="drawer" aria-label="Preferências">
      <div className="settings-head">
        <h2>Ajustes</h2>
        <button aria-label="Fechar ajustes" onClick={onClose}>
          <X />
        </button>
      </div>

      <div className="tabs" role="tablist">
        {(
          [
            ["ai", "IA"],
            ["voice", "Voz"],
            ["system", "Sistema"],
            ["visual", "Visual"],
            ["mobile", "Celular"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "tab active" : "tab"}
            onClick={() => onTabChange(id)}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="drawer-body">
        {tab === "ai" && (
          <>
            <label>
              Provedor de IA
              <select
                aria-label="Provedor de IA"
                value={config.provider}
                onChange={(e) => {
                  setTestResult(null);
                  void onUpdateConfig({ provider: e.target.value as Provider });
                }}
              >
                <option value="chatgpt">ChatGPT Plus / Pro (OAuth)</option>
                <option value="gemini">Google Gemini</option>
              </select>
            </label>

            {config.provider === "gemini" && (
              <>
                <div className="tabs" role="group" aria-label="Autenticação Gemini">
                  {(
                    [
                      ["apikey", "Chave da API (AI Studio)"],
                      ["oauth", "Conta Google (OAuth)"],
                    ] as Array<[GeminiAuthMode, string]>
                  ).map(([modeId, label]) => (
                    <button
                      key={modeId}
                      type="button"
                      className={config.gemini_auth_mode === modeId ? "tab active" : "tab"}
                      onClick={() => {
                        setTestResult(null);
                        setGoogleError(null);
                        void onUpdateConfig({ gemini_auth_mode: modeId });
                      }}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                {config.gemini_auth_mode === "apikey" ? (
                  <div className="field-block">
                    <div className="field-label-row">
                      <span>Chave da API Gemini</span>
                      <small className={keyStatus.gemini.configured ? "ok-tag" : "warn-tag"}>
                        {keyStatus.gemini.configured
                          ? `Salva (${keyStatus.gemini.preview})`
                          : "Não configurada"}
                      </small>
                    </div>
                    <div className="inline-row">
                      <input
                        type="password"
                        value={geminiKeyInput}
                        placeholder={
                          keyStatus.gemini.configured
                            ? "Substituir chave Gemini"
                            : "Cole sua chave do Google AI Studio"
                        }
                        aria-label="Chave da API"
                        onChange={(e) => setGeminiKeyInput(e.target.value)}
                      />
                      <button
                        type="button"
                        className="mini-btn accent"
                        disabled={!geminiKeyInput.trim()}
                        onClick={() => {
                          const val = geminiKeyInput.trim();
                          setGeminiKeyInput("");
                          setTestResult(null);
                          void onSaveKey("gemini", val);
                        }}
                      >
                        Salvar
                      </button>
                      {keyStatus.gemini.configured && (
                        <button
                          type="button"
                          className="mini-btn"
                          onClick={() => void onClearKey("gemini")}
                        >
                          Limpar
                        </button>
                      )}
                    </div>
                  </div>
                ) : (
                  <div className="oauth-card">
                    <div className="field-label-row">
                      <span>Conta Google (Cloud OAuth 2.0)</span>
                      <small className={googleAuth.connected ? "ok-tag" : "warn-tag"}>
                        {googleAuth.connected ? "Conectada" : "Desconectada"}
                      </small>
                    </div>

                    <p className="hint-box">
                      Use um OAuth Client ID (tipo Computador/Desktop) do seu projeto no Google Cloud Console com a Generative Language API ativada.
                    </p>

                    <input
                      ref={googleFileRef}
                      type="file"
                      accept=".json,application/json"
                      hidden
                      onChange={(e) => void handleGoogleJsonFile(e)}
                    />
                    <button
                      type="button"
                      className="mini-btn"
                      onClick={() => googleFileRef.current?.click()}
                    >
                      <Upload /> Importar JSON do Google Cloud
                    </button>

                    <label>
                      Client ID
                      <input
                        type="text"
                        value={config.google_oauth_client_id}
                        placeholder="xxxx.apps.googleusercontent.com"
                        onChange={(e) =>
                          void onUpdateConfig({ google_oauth_client_id: e.target.value })
                        }
                      />
                    </label>

                    <div className="field-block">
                      <div className="field-label-row">
                        <span>Client Secret</span>
                        <small
                          className={
                            keyStatus.google_client_secret.configured ? "ok-tag" : "warn-tag"
                          }
                        >
                          {keyStatus.google_client_secret.configured
                            ? `Salvo (${keyStatus.google_client_secret.preview})`
                            : "Não salvo"}
                        </small>
                      </div>
                      <div className="inline-row">
                        <input
                          type="password"
                          value={googleSecretInput}
                          placeholder={
                            keyStatus.google_client_secret.configured
                              ? "Substituir Client Secret"
                              : "GOCSPX-..."
                          }
                          onChange={(e) => setGoogleSecretInput(e.target.value)}
                        />
                        <button
                          type="button"
                          className="mini-btn"
                          disabled={!googleSecretInput.trim()}
                          onClick={() => {
                            const val = googleSecretInput.trim();
                            setGoogleSecretInput("");
                            void onSaveKey("google_client_secret", val);
                          }}
                        >
                          Salvar
                        </button>
                      </div>
                    </div>

                    <label>
                      Project ID (opcional)
                      <input
                        type="text"
                        value={config.google_oauth_project_id}
                        placeholder="meu-projeto-gcp"
                        onChange={(e) =>
                          void onUpdateConfig({ google_oauth_project_id: e.target.value })
                        }
                      />
                    </label>

                    {googleAuth.connected ? (
                      <>
                        <p className="hint-box">
                          Conectado como{" "}
                          <strong>
                            {googleAuth.email ?? googleAuth.name ?? "Conta Google"}
                          </strong>
                        </p>
                        <div className="inline-row">
                          <button
                            type="button"
                            className="mini-btn"
                            disabled={googleLoading}
                            onClick={() => void onGoogleLogout()}
                          >
                            <LogOut /> Desconectar
                          </button>
                          <button
                            type="button"
                            className="mini-btn accent"
                            disabled={googleLoading}
                            onClick={() => void handleGoogleLogin()}
                          >
                            {googleLoading ? <Loader2 className="spin" /> : <RefreshCw />} Reconectar
                          </button>
                        </div>
                      </>
                    ) : (
                      <button
                        type="button"
                        className="mini-btn accent"
                        disabled={
                          googleLoading ||
                          !config.google_oauth_client_id.trim() ||
                          !keyStatus.google_client_secret.configured
                        }
                        onClick={() => void handleGoogleLogin()}
                      >
                        {googleLoading ? <Loader2 className="spin" /> : <LogIn />}{" "}
                        {googleLoading
                          ? "Aguardando login Google no navegador..."
                          : "Entrar com Google (OAuth)"}
                      </button>
                    )}
                    {googleError && <small className="warn-tag">{googleError}</small>}
                  </div>
                )}

                <label>
                  Modelo Gemini
                  <input
                    type="text"
                    value={config.gemini_model}
                    onChange={(e) => void onUpdateConfig({ gemini_model: e.target.value })}
                  />
                  <div className="preset-row">
                    {GEMINI_PRESETS.map((m) => (
                      <button
                        key={m}
                        type="button"
                        className={config.gemini_model === m ? "preset active" : "preset"}
                        onClick={() => void onUpdateConfig({ gemini_model: m })}
                      >
                        {m}
                      </button>
                    ))}
                  </div>
                </label>
              </>
            )}

            {config.provider === "chatgpt" && (
              <>
                <div className="oauth-card">
                  <div className="field-label-row">
                    <span>Conta ChatGPT (OAuth 2.0)</span>
                    <small className={chatgptAuth.connected ? "ok-tag" : "warn-tag"}>
                      {chatgptAuth.connected ? "Conectada" : "Desconectada"}
                    </small>
                  </div>

                  {chatgptAuth.connected ? (
                    <>
                      <p className="hint-box">
                        Conectado como{" "}
                        <strong>
                          {chatgptAuth.email ?? chatgptAuth.name ?? "Conta ChatGPT"}
                        </strong>
                        {chatgptAuth.planUsageGranted
                          ? " · Uso pelo plano ativo liberado."
                          : " · Atenção: permissão de uso do plano não concedida."}
                      </p>
                      <div className="inline-row">
                        <button
                          type="button"
                          className="mini-btn"
                          disabled={oauthLoading}
                          onClick={() => void onChatgptLogout(false)}
                        >
                          <LogOut /> Desconectar
                        </button>
                        <button
                          type="button"
                          className="mini-btn"
                          disabled={oauthLoading}
                          onClick={() => void handleChatgptLogin(true)}
                        >
                          {oauthLoading ? <Loader2 className="spin" /> : <RefreshCw />} Reconectar
                        </button>
                      </div>
                    </>
                  ) : (
                    <>
                      <p className="hint-box">
                        Entre no navegador com sua conta ChatGPT Plus ou Pro usando o fluxo oficial Sign in with ChatGPT.
                      </p>
                      <button
                        type="button"
                        className="mini-btn accent"
                        disabled={oauthLoading}
                        onClick={() => void handleChatgptLogin(false)}
                      >
                        {oauthLoading ? <Loader2 className="spin" /> : <LogIn />}{" "}
                        {oauthLoading
                          ? "Aguardando login no navegador..."
                          : "Entrar com ChatGPT (OAuth)"}
                      </button>
                    </>
                  )}
                  {oauthError && <small className="warn-tag">{oauthError}</small>}
                </div>

                <div className="field-block">
                  <div className="field-label-row">
                    <span>Modelo ChatGPT</span>
                    {chatgptAuth.connected && (
                      <button
                        type="button"
                        className="mini-btn"
                        disabled={loadingChatgptModels}
                        onClick={() => void handleRefreshChatgptModels()}
                      >
                        {loadingChatgptModels ? <Loader2 className="spin" /> : <RefreshCw />} Modelos
                      </button>
                    )}
                  </div>
                  <input
                    type="text"
                    value={config.chatgpt_model}
                    onChange={(e) => void onUpdateConfig({ chatgpt_model: e.target.value })}
                  />
                  <div className="preset-row">
                    {(chatgptModels.length > 0 ? chatgptModels.slice(0, 8) : CHATGPT_PRESETS).map(
                      (m) => (
                        <button
                          key={m}
                          type="button"
                          className={config.chatgpt_model === m ? "preset active" : "preset"}
                          onClick={() => void onUpdateConfig({ chatgpt_model: m })}
                        >
                          {m}
                        </button>
                      ),
                    )}
                  </div>
                </div>
              </>
            )}

            <div className="inline-row">
              <button
                type="button"
                className="mini-btn accent"
                disabled={testing}
                onClick={() => void handleTest()}
              >
                {testing ? <Loader2 className="spin" /> : <Check />} Testar conexão
              </button>
              {testResult && (
                <small className={testResult.ok ? "ok-tag" : "warn-tag"}>
                  {testResult.message} ({testResult.latencyMs}ms)
                </small>
              )}
            </div>

            <label>
              Instrução de personalidade (System Prompt)
              <textarea
                className="prompt-box"
                rows={3}
                value={config.system_prompt}
                onChange={(e) =>
                  void onUpdateConfig({ system_prompt: e.target.value })
                }
              />
            </label>
          </>
        )}

        {tab === "voice" && (
          <>
            <label>
              Motor de Fala (TTS)
              <select
                aria-label="Motor de voz"
                value={config.tts_engine}
                onChange={(e) =>
                  void onUpdateConfig({ tts_engine: e.target.value as TtsEngine })
                }
              >
                <option value="elevenlabs">ElevenLabs (Nuvem · Ultra-Rápida)</option>
                <option value="off">Desativado (somente texto)</option>
              </select>
            </label>

            {config.tts_engine === "elevenlabs" && (
              <div className="voice-card">
                <div className="field-label-row">
                  <span>Chave da API ElevenLabs</span>
                  <small className={keyStatus.elevenlabs.configured ? "ok-tag" : "warn-tag"}>
                    {keyStatus.elevenlabs.configured
                      ? `Salva (${keyStatus.elevenlabs.preview})`
                      : "Não configurada"}
                  </small>
                </div>

                <div className="inline-row">
                  <input
                    type="password"
                    value={elevenKeyInput}
                    placeholder={
                      keyStatus.elevenlabs.configured
                        ? "Substituir chave da ElevenLabs"
                        : "Cole sua chave xi-api-key da ElevenLabs"
                    }
                    aria-label="Chave da ElevenLabs"
                    onChange={(e) => setElevenKeyInput(e.target.value)}
                  />
                  <button
                    type="button"
                    className="mini-btn accent"
                    disabled={!elevenKeyInput.trim()}
                    onClick={async () => {
                      const val = elevenKeyInput.trim();
                      setElevenKeyInput("");
                      await onSaveKey("elevenlabs", val);
                      void handleRefreshEleven();
                    }}
                  >
                    Salvar
                  </button>
                  {keyStatus.elevenlabs.configured && (
                    <button
                      type="button"
                      className="mini-btn"
                      onClick={() => void onClearKey("elevenlabs")}
                    >
                      Limpar
                    </button>
                  )}
                </div>

                <label>
                  Economia de Créditos (Quanto falar)
                  <select
                    aria-label="Economia de créditos"
                    value={config.tts_reply_mode}
                    onChange={(e) =>
                      void onUpdateConfig({
                        tts_reply_mode: e.target.value as TtsReplyMode,
                      })
                    }
                  >
                    <option value="brief">
                      Econômico (Resumo natural · Até ~220 caracteres)
                    </option>
                    <option value="miconly">
                      Só no Microfone (Mudo ao digitar · Resumo ao falar)
                    </option>
                    <option value="full">
                      Completo (Ler resposta inteira)
                    </option>
                  </select>
                </label>

                <label>
                  Modelo ElevenLabs
                  <select
                    aria-label="Modelo ElevenLabs"
                    value={config.elevenlabs_model_id}
                    onChange={(e) =>
                      void onUpdateConfig({ elevenlabs_model_id: e.target.value })
                    }
                  >
                    <option value="eleven_flash_v2_5">
                      Eleven Flash v2.5 (Ultra-rápido · 50% menos créditos)
                    </option>
                    <option value="eleven_turbo_v2_5">
                      Eleven Turbo v2.5 (Baixa latência · 50% menos créditos)
                    </option>
                    <option value="eleven_multilingual_v2">
                      Eleven Multilingual v2 (Máxima expressividade)
                    </option>
                  </select>
                </label>

                <div className="field-block">
                  <div className="field-label-row">
                    <span>Voice ID (Voz Clonada ou da Biblioteca)</span>
                    {keyStatus.elevenlabs.configured && (
                      <button
                        type="button"
                        className="mini-btn"
                        disabled={loadingElevenVoices}
                        onClick={() => void handleRefreshEleven()}
                      >
                        {loadingElevenVoices ? (
                          <Loader2 className="spin" />
                        ) : (
                          <RefreshCw />
                        )}{" "}
                        Minhas vozes
                      </button>
                    )}
                  </div>

                  <input
                    type="text"
                    aria-label="Voice ID da ElevenLabs"
                    value={config.elevenlabs_voice_id}
                    placeholder="Ex.: JBFqnCBsd6RMkjVDRZzb"
                    onChange={(e) =>
                      void onUpdateConfig({ elevenlabs_voice_id: e.target.value })
                    }
                  />

                  {elevenVoices.length > 0 ? (
                    <select
                      aria-label="Vozes da sua conta ElevenLabs"
                      value={config.elevenlabs_voice_id}
                      onChange={(e) =>
                        void onUpdateConfig({ elevenlabs_voice_id: e.target.value })
                      }
                    >
                      {elevenVoices.map((v) => (
                        <option key={v.voiceId} value={v.voiceId}>
                          {v.category !== "premade" ? `[Clonada] ${v.name}` : v.name}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <div className="preset-row">
                      {ELEVENLABS_VOICE_PRESETS.map((p) => (
                        <button
                          key={p.id}
                          type="button"
                          className={
                            config.elevenlabs_voice_id === p.id
                              ? "preset active"
                              : "preset"
                          }
                          onClick={() =>
                            void onUpdateConfig({ elevenlabs_voice_id: p.id })
                          }
                        >
                          {p.label}
                        </button>
                      ))}
                    </div>
                  )}
                </div>

                {elevenError && <small className="warn-tag">{elevenError}</small>}

                <label>
                  Velocidade da fala ({config.tts_rate.toFixed(2)}×)
                  <input
                    type="range"
                    min="0.7"
                    max="1.2"
                    step="0.05"
                    value={config.tts_rate}
                    onChange={(e) =>
                      void onUpdateConfig({ tts_rate: Number(e.target.value) })
                    }
                  />
                </label>

                <p className="hint-box">
                  Cache local em disco ativo: frases repetidas não gastam créditos da ElevenLabs.
                </p>

                <button
                  type="button"
                  className="mini-btn accent"
                  onClick={() => void onTestVoice()}
                >
                  <Volume2 /> Testar voz
                </button>
              </div>
            )}

            <label>
              Reconhecimento de Microfone (STT)
              <select
                aria-label="Reconhecimento de voz"
                value={config.stt_engine}
                onChange={(e) =>
                  void onUpdateConfig({ stt_engine: e.target.value as SttEngine })
                }
              >
                <option value="auto">Automático (Gemini / ElevenLabs Scribe)</option>
                <option value="elevenlabs">ElevenLabs Scribe (Usa sua chave ElevenLabs)</option>
                <option value="gemini">Gemini Audio</option>
                <option value="browser">Navegador (Web Speech)</option>
              </select>
            </label>

            <div className="voice-card">
              <div className="field-label-row">
                <span>Dispositivo de Microfone</span>
                <button
                  type="button"
                  className={testingMic ? "mini-btn accent" : "mini-btn"}
                  onClick={() => void toggleMicTest()}
                >
                  <Mic /> {testingMic ? "Parar teste" : "Testar microfone"}
                </button>
              </div>
              <select
                aria-label="Selecionar microfone"
                value={config.mic_device_id}
                onChange={(e) =>
                  void onUpdateConfig({ mic_device_id: e.target.value })
                }
              >
                <option value="">Padrão do Windows</option>
                {micDevices.map((d) => (
                  <option key={d.deviceId} value={d.deviceId}>
                    {d.label}
                  </option>
                ))}
              </select>
              {testingMic && (
                <div className="mic-meter-track" aria-label="Nível do microfone">
                  <div
                    className="mic-meter-fill"
                    style={{ width: `${Math.round(micLevel * 100)}%` }}
                  />
                </div>
              )}
              {micStatus && (
                <small className={micStatus.startsWith("Erro") ? "warn-tag" : "ok-tag"}>
                  {micStatus}
                </small>
              )}
            </div>

            <label className="check-row">
              <input
                type="checkbox"
                checked={config.auto_listen_vad}
                onChange={(e) =>
                  void onUpdateConfig({ auto_listen_vad: e.target.checked })
                }
              />
              Enviar automaticamente após pausa na fala (VAD)
            </label>

            <label className="check-row">
              <input
                type="checkbox"
                checked={config.wake_word_enabled}
                onChange={(e) =>
                  void onUpdateConfig({ wake_word_enabled: e.target.checked })
                }
              />
              Ativar por voz com &ldquo;Ei, Jarvis&rdquo; (Wake Word)
            </label>

            <label className="check-row">
              <input
                type="checkbox"
                checked={config.hands_free_mode}
                onChange={(e) =>
                  void onUpdateConfig({ hands_free_mode: e.target.checked })
                }
              />
              Modo Mãos-Livres (conversa contínua após falar)
            </label>

            {(config.wake_word_enabled || config.hands_free_mode) && (
              <p className="hint-box">
                Diga &ldquo;Ei, Jarvis&rdquo; (ou &ldquo;Ei, Jarvis, que horas são?&rdquo;) para chamar sem clicar. No modo Mãos-Livres, diga &ldquo;parar&rdquo; ou &ldquo;tchau&rdquo; para encerrar.
              </p>
            )}
          </>
        )}

        {tab === "system" && (
          <>
            <label>
              Modo de permissão e autonomia
              <select
                aria-label="Modo de permissão"
                value={config.permission_mode}
                onChange={(e) =>
                  void onUpdateConfig({
                    permission_mode: e.target.value as PermissionMode,
                  })
                }
              >
                <option value="auto">Permissão Total (Automático)</option>
                <option value="review">Revisão Automática (Aprovar ações)</option>
                <option value="safe">Somente Conversa (Sem ações no PC)</option>
              </select>
            </label>

            <p className="hint-box">
              {config.permission_mode === "auto"
                ? "Executa automaticamente consultas, busca na web, apps, volume, mídia, timers e comandos PowerShell."
                : config.permission_mode === "review"
                  ? "Executa consultas e busca na web automaticamente, mas pede sua aprovação antes de abrir apps, alterar volume ou rodar comandos."
                  : "Bloqueia ações locais e leitura de hardware; mantém conversa, memória e busca na web."}
            </p>

            <label className="check-row">
              <input
                type="checkbox"
                checked={config.web_search_enabled}
                onChange={(e) =>
                  void onUpdateConfig({ web_search_enabled: e.target.checked })
                }
              />
              Permitir busca na web (DuckDuckGo)
            </label>

            <div className="field-block">
              <div className="field-label-row">
                <span>Atalho global para chamar J.A.R.V.I.S.</span>
              </div>
              <div className="inline-row">
                <input
                  type="text"
                  value={shortcutDraft}
                  aria-label="Atalho global"
                  onChange={(e) => setShortcutDraft(e.target.value)}
                />
                <button
                  type="button"
                  className="mini-btn accent"
                  onClick={() => void handleApplyShortcut()}
                >
                  Aplicar
                </button>
              </div>
              {shortcutStatus && <small className="hint-line">{shortcutStatus}</small>}
            </div>

            <label className="check-row">
              <input
                type="checkbox"
                checked={config.minimize_to_tray}
                onChange={(e) =>
                  void onUpdateConfig({ minimize_to_tray: e.target.checked })
                }
              />
              Minimizar para a bandeja ao fechar janela
            </label>

            <div className="field-block">
              <div className="field-label-row">
                <span>Atualizações do Aplicativo (GitHub Releases)</span>
                <small>{updateCheck ? `v${updateCheck.currentVersion}` : "v8.0.0"}</small>
              </div>
              <div className="inline-row">
                <button
                  type="button"
                  className="mini-btn"
                  disabled={updateBusy}
                  onClick={async () => {
                    setUpdateBusy(true);
                    setUpdateMsg(null);
                    try {
                      const res = await checkAppUpdate();
                      setUpdateCheck(res);
                      setUpdateMsg(
                        res.updateAvailable
                          ? `Nova versão v${res.latestVersion} disponível!`
                          : `Você está na versão mais recente (v${res.latestVersion}).`,
                      );
                    } catch (err) {
                      setUpdateMsg(String(err));
                    } finally {
                      setUpdateBusy(false);
                    }
                  }}
                >
                  {updateBusy ? <Loader2 className="spin" size={13} /> : <RefreshCw size={13} />}
                  Verificar Atualização
                </button>

                {updateCheck?.installerUrl && (
                  <button
                    type="button"
                    className="mini-btn accent"
                    disabled={updateBusy}
                    onClick={async () => {
                      if (!updateCheck.installerUrl) return;
                      setUpdateBusy(true);
                      setUpdateMsg(null);
                      setUpdateProgress(null);
                      try {
                        const msg = await installAppUpdate(
                          updateCheck.installerUrl,
                          updateCheck.installerName ?? "JARVIS-setup.exe",
                        );
                        setUpdateMsg(msg);
                      } catch (err) {
                        setUpdateMsg(String(err));
                      } finally {
                        setUpdateBusy(false);
                      }
                    }}
                  >
                    <Download size={13} />
                    {updateCheck.updateAvailable
                      ? `Atualizar para v${updateCheck.latestVersion}`
                      : "Reinstalar Build do Release (.exe)"}
                  </button>
                )}
              </div>
              {updateProgress && (
                <small className="hint-line">
                  {updateProgress.stage === "launching"
                    ? "Iniciando instalador…"
                    : `Baixando instalador: ${updateProgress.percent}% (${(
                        updateProgress.downloadedBytes /
                        (1024 * 1024)
                      ).toFixed(1)} MB)`}
                </small>
              )}
              {updateMsg && <small className="hint-line">{updateMsg}</small>}
            </div>
          </>
        )}

        {tab === "visual" && (
          <>
            <div className="field-block">
              <div className="field-label-row">
                <span>Tema do Núcleo e HUD</span>
              </div>
              <div className="theme-grid" role="group" aria-label="Tema visual">
                {VISUAL_THEMES.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    className={theme === item.id ? "theme-chip active" : "theme-chip"}
                    onClick={() => onThemeChange(item.id)}
                  >
                    <span
                      className="theme-swatch"
                      style={{
                        background: item.color,
                        boxShadow: `0 0 10px ${item.color}`,
                      }}
                    />
                    {item.label}
                  </button>
                ))}
              </div>
            </div>
            <label>
              Qualidade gráfica
              <select
                value={quality}
                onChange={(e) => onQualityChange(e.target.value as Quality)}
              >
                <option value="Low">Baixa</option>
                <option value="High">Alta</option>
                <option value="Ultra">Ultra</option>
              </select>
            </label>
            <label>
              Volume
              <input
                type="range"
                min="0"
                max="1"
                step=".01"
                value={volume}
                onChange={(e) => onVolumeChange(+e.target.value)}
              />
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={showMascot}
                onChange={(e) => onShowMascotChange(e.target.checked)}
              />
              Mostrar mascote animado
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={reduced}
                onChange={(e) => onReducedChange(e.target.checked)}
              />
              Reduzir movimento
            </label>
          </>
        )}

        {tab === "mobile" && (
          <div className="companion-tab">
            <label className="check-row">
              <input
                type="checkbox"
                checked={companionInfo?.enabled ?? false}
                disabled={companionBusy}
                onChange={async (e) => {
                  setCompanionBusy(true);
                  setCompanionMsg(null);
                  try {
                    const next = await setCompanionEnabled(e.target.checked);
                    setCompanionInfo(next);
                    if (next.enabled && next.lanEnabled) {
                      const qr = await QRCode.toDataURL(next.pairUrl, {
                        width: 210,
                        margin: 2,
                        color: { dark: "#050811", light: "#ffffff" },
                      }).catch(() => "");
                      setCompanionQr(qr);
                    } else {
                      setCompanionQr("");
                    }
                  } finally {
                    setCompanionBusy(false);
                  }
                }}
              />
              <span>Ativar Jarvis Companion (servidor local)</span>
            </label>

            {companionInfo && companionInfo.enabled && (
              <>
                <label className="check-row">
                  <input
                    type="checkbox"
                    checked={companionInfo.lanEnabled}
                    disabled={companionBusy}
                    onChange={async (e) => {
                      setCompanionBusy(true);
                      setCompanionMsg(null);
                      try {
                        const next = await setCompanionLanEnabled(e.target.checked);
                        setCompanionInfo(next);
                        if (next.lanEnabled) {
                          const qr = await QRCode.toDataURL(next.pairUrl, {
                            width: 210,
                            margin: 2,
                            color: { dark: "#050811", light: "#ffffff" },
                          }).catch(() => "");
                          setCompanionQr(qr);
                        } else {
                          setCompanionQr("");
                        }
                      } finally {
                        setCompanionBusy(false);
                      }
                    }}
                  />
                  <span>Expor na rede Wi-Fi (Modo LAN para celular)</span>
                </label>

                <div className="companion-lan-warning" role="alert">
                  <strong>Aviso de rede:</strong> O acesso pela rede local usa HTTP sem TLS
                  e permite que dispositivos pareados controlem chat, aprovações e capturas de tela do PC.
                  Ative o modo LAN apenas em redes Wi-Fi privadas confiáveis.
                </div>

                <div className="companion-status-row">
                  <span
                    className={
                      companionInfo.connectedClients > 0
                        ? "companion-status-pill live"
                        : "companion-status-pill"
                    }
                  >
                    <Smartphone size={13} />
                    {companionInfo.connectedClients > 0
                      ? `${companionInfo.connectedClients} celular(es) conectado(s)`
                      : `Bind ${companionInfo.bindHost}:${companionInfo.port}`}
                  </span>
                  <span className="companion-code-pill" title="Código de pareamento de uso único">
                    Código: <strong>{companionInfo.pairingCode}</strong>
                  </span>
                </div>

                <div className="field-block">
                  <div className="field-label-row">
                    <span>Rota de conexão do QR Code</span>
                  </div>
                  <div className="tabs" role="tablist" aria-label="Rota de pareamento">
                    <button
                      type="button"
                      className={companionRoute === "lan" ? "tab active" : "tab"}
                      onClick={() => setCompanionRoute("lan")}
                    >
                      Wi-Fi Local ({companionInfo.ip})
                    </button>
                    <button
                      type="button"
                      className={companionRoute === "tailscale" ? "tab active" : "tab"}
                      disabled={!companionInfo.tailscaleIp}
                      title={
                        companionInfo.tailscaleIp
                          ? `Tailscale VPN detectado (${companionInfo.tailscaleIp})`
                          : "Tailscale não detectado neste PC"
                      }
                      onClick={() => setCompanionRoute("tailscale")}
                    >
                      Tailscale 4G/5G {companionInfo.tailscaleIp ? `(${companionInfo.tailscaleIp})` : "(Off)"}
                    </button>
                    <button
                      type="button"
                      className={companionRoute === "external" ? "tab active" : "tab"}
                      onClick={() => setCompanionRoute("external")}
                    >
                      Túnel HTTPS
                    </button>
                  </div>
                </div>

                {companionRoute === "external" && (
                  <div className="field-block">
                    <div className="field-label-row">
                      <span>URL do Túnel HTTPS (Tailscale Serve/Funnel ou Cloudflare Tunnel)</span>
                    </div>
                    <div className="inline-row">
                      <input
                        type="url"
                        value={externalUrlDraft}
                        placeholder="https://meu-pc.tailxxxx.ts.net"
                        aria-label="URL externa HTTPS do Companion"
                        onChange={(e) => setExternalUrlDraft(e.target.value)}
                      />
                      <button
                        type="button"
                        className="mini-btn accent"
                        disabled={companionBusy}
                        onClick={async () => {
                          setCompanionBusy(true);
                          setCompanionMsg(null);
                          try {
                            const next = await setCompanionExternalUrl(
                              externalUrlDraft.trim() || null,
                            );
                            setCompanionInfo(next);
                            const targetUrl = resolveActivePairUrl(next, "external");
                            if (next.externalPairUrl) {
                              const qr = await QRCode.toDataURL(targetUrl, {
                                width: 210,
                                margin: 2,
                                color: { dark: "#050811", light: "#ffffff" },
                              }).catch(() => "");
                              setCompanionQr(qr);
                              setCompanionMsg("URL de túnel HTTPS salva e QR Code atualizado.");
                            } else {
                              setCompanionMsg("URL de túnel removida.");
                            }
                          } catch (err) {
                            setCompanionMsg(String(err));
                          } finally {
                            setCompanionBusy(false);
                          }
                        }}
                      >
                        Salvar URL
                      </button>
                    </div>
                  </div>
                )}

                {(companionInfo.lanEnabled || companionRoute === "external") && companionQr && (
                  <div className="companion-qr-card">
                    <img src={companionQr} alt="QR Code Jarvis Companion" />
                  </div>
                )}

                <div className="companion-url-row">
                  <input
                    type="text"
                    readOnly
                    value={resolveActivePairUrl(companionInfo, companionRoute)}
                    aria-label="URL de pareamento único"
                  />
                  <button
                    type="button"
                    className="mini-btn"
                    onClick={async () => {
                      await navigator.clipboard.writeText(
                        resolveActivePairUrl(companionInfo, companionRoute),
                      );
                      setCopiedMobileUrl(true);
                      setTimeout(() => setCopiedMobileUrl(false), 1600);
                    }}
                  >
                    {copiedMobileUrl ? <Check size={13} /> : <Copy size={13} />}
                    {copiedMobileUrl ? "Copiado" : "Copiar"}
                  </button>
                </div>

                <div className="companion-actions-row">
                  <button
                    type="button"
                    className="mini-btn"
                    disabled={companionBusy}
                    onClick={async () => {
                      setCompanionBusy(true);
                      try {
                        const next = await regenerateCompanionToken();
                        setCompanionInfo(next);
                        if (next.lanEnabled) {
                          const dataUrl = await QRCode.toDataURL(next.pairUrl, {
                            width: 210,
                            margin: 2,
                            color: { dark: "#050811", light: "#ffffff" },
                          }).catch(() => "");
                          setCompanionQr(dataUrl);
                        }
                        setCompanionMsg(
                          "Sessões anteriores revogadas e novo código de uso único gerado.",
                        );
                      } finally {
                        setCompanionBusy(false);
                      }
                    }}
                  >
                    <RefreshCw size={13} /> Revogar Sessões / Novo Código
                  </button>
                  {companionInfo.lanEnabled && (
                    <button
                      type="button"
                      className="mini-btn accent"
                      disabled={companionBusy}
                      onClick={async () => {
                        setCompanionBusy(true);
                        setCompanionMsg(null);
                        try {
                          const msg = await allowCompanionFirewall();
                          setCompanionMsg(msg);
                        } catch (err) {
                          setCompanionMsg(String(err));
                        } finally {
                          setCompanionBusy(false);
                        }
                      }}
                    >
                      <ShieldCheck size={13} /> Liberar Firewall
                    </button>
                  )}
                </div>

                {companionMsg && <p className="field-hint">{companionMsg}</p>}

                <div className="companion-steps">
                  <strong>Como conectar com segurança:</strong>
                  <ol>
                    <li>Conecte PC e celular no mesmo Wi-Fi confiável e ative <b>Modo LAN</b>.</li>
                    <li>Se necessário no primeiro uso, clique em <b>Liberar Firewall</b>.</li>
                    <li>Escaneie o QR Code (código de uso único troca para cookie <code>HttpOnly</code> de sessão) e toque em <b>Adicionar à Tela de Início</b>.</li>
                  </ol>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}

import { synthesizeSpeech, transcribeAudio } from "./bridge";

export const audioLevelRef: { current: number } = { current: 0 };

export async function openMicStream(deviceId?: string): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error("Microfone indisponível neste ambiente.");
  }
  const baseAudio: MediaTrackConstraints = {
    echoCancellation: true,
    noiseSuppression: true,
    autoGainControl: true,
  };
  const cleanId = deviceId?.trim();
  if (cleanId) {
    try {
      return await navigator.mediaDevices.getUserMedia({
        audio: { ...baseAudio, deviceId: { exact: cleanId } },
      });
    } catch {
      // Fallback to default system microphone if selected device was unplugged
    }
  }
  return navigator.mediaDevices.getUserMedia({ audio: baseAudio });
}

export async function listAudioInputDevices(): Promise<
  Array<{ deviceId: string; label: string }>
> {
  if (!navigator.mediaDevices?.enumerateDevices) return [];
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices
    .filter((d) => d.kind === "audioinput")
    .map((d, idx) => ({
      deviceId: d.deviceId,
      label: d.label || `Microfone ${idx + 1}`,
    }));
}

function downsampleTo16k(samples: Float32Array, inputRate: number): Int16Array {
  if (inputRate <= 16000) {
    const out = new Int16Array(samples.length);
    for (let i = 0; i < samples.length; i++) {
      const s = Math.max(-1, Math.min(1, samples[i]));
      out[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
    }
    return out;
  }
  const ratio = inputRate / 16000;
  const outLength = Math.floor(samples.length / ratio);
  const out = new Int16Array(outLength);
  for (let i = 0; i < outLength; i++) {
    const start = Math.floor(i * ratio);
    const end = Math.min(samples.length, Math.floor((i + 1) * ratio));
    let sum = 0;
    let count = 0;
    for (let j = start; j < end; j++) {
      sum += samples[j];
      count++;
    }
    const avg = count > 0 ? sum / count : 0;
    const s = Math.max(-1, Math.min(1, avg));
    out[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
  }
  return out;
}

function encodeWav16k(chunks: Float32Array[], sampleRate: number): Uint8Array {
  let totalLen = 0;
  for (const c of chunks) totalLen += c.length;
  const merged = new Float32Array(totalLen);
  let offset = 0;
  for (const c of chunks) {
    merged.set(c, offset);
    offset += c.length;
  }
  const pcm = downsampleTo16k(merged, sampleRate);
  const byteLength = pcm.length * 2;
  const buffer = new ArrayBuffer(44 + byteLength);
  const view = new DataView(buffer);

  const writeStr = (pos: number, str: string) => {
    for (let i = 0; i < str.length; i++) view.setUint8(pos + i, str.charCodeAt(i));
  };

  writeStr(0, "RIFF");
  view.setUint32(4, 36 + byteLength, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, 16000, true);
  view.setUint32(28, 16000 * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, "data");
  view.setUint32(40, byteLength, true);

  let dataPos = 44;
  for (let i = 0; i < pcm.length; i++) {
    view.setInt16(dataPos, pcm[i], true);
    dataPos += 2;
  }
  return new Uint8Array(buffer);
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

function base64ToArrayBuffer(b64: string): ArrayBuffer {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

export class MicRecorder {
  private stream?: MediaStream;
  private context?: AudioContext;
  private processor?: ScriptProcessorNode;
  private source?: MediaStreamAudioSourceNode;
  private muteGain?: GainNode;
  private chunks: Float32Array[] = [];
  private sampleRate = 48000;
  private heardSpeech = false;
  private silenceStart = 0;
  private maxTimer = 0;
  private stopped = false;

  async start(
    vadEnabled: boolean,
    onAutoStop: () => void,
    deviceId?: string,
  ): Promise<void> {
    this.stream = await openMicStream(deviceId);
    const context = new AudioContext();
    this.context = context;
    if (context.state === "suspended") await context.resume();
    this.sampleRate = context.sampleRate;

    const source = context.createMediaStreamSource(this.stream);
    const processor = context.createScriptProcessor(2048, 1, 1);
    const muteGain = context.createGain();
    muteGain.gain.value = 0;

    this.chunks = [];
    this.heardSpeech = false;
    this.silenceStart = 0;
    this.stopped = false;

    processor.onaudioprocess = (ev) => {
      if (this.stopped) return;
      const input = ev.inputBuffer.getChannelData(0);
      this.chunks.push(new Float32Array(input));

      let sum = 0;
      for (let i = 0; i < input.length; i++) sum += input[i] * input[i];
      const rms = Math.sqrt(sum / Math.max(1, input.length));
      const norm = Math.min(1, rms * 6.5);
      audioLevelRef.current = audioLevelRef.current * 0.45 + norm * 0.55;

      if (vadEnabled) {
        const now = performance.now();
        if (rms > 0.025) {
          this.heardSpeech = true;
          this.silenceStart = 0;
        } else if (this.heardSpeech) {
          if (this.silenceStart === 0) {
            this.silenceStart = now;
          } else if (now - this.silenceStart >= 1450) {
            onAutoStop();
          }
        }
      }
    };

    source.connect(processor);
    processor.connect(muteGain);
    muteGain.connect(context.destination);

    this.source = source;
    this.processor = processor;
    this.muteGain = muteGain;

    this.maxTimer = window.setTimeout(() => {
      if (!this.stopped) onAutoStop();
    }, 30000);
  }

  async stop(): Promise<{ audioBase64: string; mimeType: string; hadAudio: boolean }> {
    this.stopped = true;
    clearTimeout(this.maxTimer);
    audioLevelRef.current = 0;

    this.processor?.disconnect();
    this.source?.disconnect();
    this.muteGain?.disconnect();
    this.stream?.getTracks().forEach((t) => t.stop());
    if (this.context && this.context.state !== "closed") {
      await this.context.close().catch(() => {});
    }

    const totalSamples = this.chunks.reduce((acc, c) => acc + c.length, 0);
    const hadAudio = totalSamples > this.sampleRate * 0.25;
    const wav = encodeWav16k(this.chunks, this.sampleRate);
    this.chunks = [];
    return {
      audioBase64: bytesToBase64(wav),
      mimeType: "audio/wav",
      hadAudio,
    };
  }
}

export function isBrowserSpeechAvailable(): boolean {
  const w = window as unknown as Record<string, unknown>;
  return Boolean(w.SpeechRecognition || w.webkitSpeechRecognition);
}

export function listenWithBrowserSpeech(): {
  promise: Promise<string>;
  stop: () => void;
} {
  const w = window as unknown as Record<
    string,
    new () => {
      lang: string;
      interimResults: boolean;
      maxAlternatives: number;
      onresult: ((e: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
      onerror: ((e: { error: string }) => void) | null;
      onend: (() => void) | null;
      start: () => void;
      stop: () => void;
    }
  >;
  const Ctor = w.SpeechRecognition || w.webkitSpeechRecognition;
  if (!Ctor) {
    return {
      promise: Promise.reject(new Error("Reconhecimento do navegador indisponível.")),
      stop: () => {},
    };
  }
  const rec = new Ctor();
  rec.lang = "pt-BR";
  rec.interimResults = false;
  rec.maxAlternatives = 1;
  let finalText = "";
  let settled = false;

  const promise = new Promise<string>((resolve, reject) => {
    rec.onresult = (e) => {
      const first = e.results?.[0]?.[0]?.transcript;
      if (first) finalText = first;
    };
    rec.onerror = (e) => {
      if (!settled) {
        settled = true;
        reject(new Error(`Erro de voz (${e.error})`));
      }
    };
    rec.onend = () => {
      if (!settled) {
        settled = true;
        resolve(finalText.trim());
      }
    };
    rec.start();
  });

  return {
    promise,
    stop: () => {
      try {
        rec.stop();
      } catch {
        /* ignore */
      }
    },
  };
}

export class SpeechQueue {
  private context?: AudioContext;
  private gain?: GainNode;
  private analyser?: AnalyserNode;
  private currentSource?: AudioBufferSourceNode;
  private raf = 0;
  private pendingBuffer = "";
  private queue: Array<{ text: string; promise: Promise<AudioBuffer | null> }> = [];
  private processing = false;
  private finished = false;
  private enabled = true;
  private generation = 0;
  private level = 0.55;
  private playing = false;
  private briefOnly = true;
  private enqueuedInTurn = 0;
  private onPlayingChange?: (playing: boolean) => void;
  private onError?: (message: string) => void;
  private reportedErrorGen = -1;

  constructor(
    onPlayingChange?: (playing: boolean) => void,
    onError?: (message: string) => void,
  ) {
    this.onPlayingChange = onPlayingChange;
    this.onError = onError;
  }

  unlock() {
    try {
      const ctx = this.ensureAudio();
      if (ctx.state === "suspended") {
        void ctx.resume().catch(() => {});
      }
    } catch {
      /* ignore */
    }
  }

  setEnabled(enabled: boolean) {
    this.enabled = enabled;
    if (!enabled) this.stop();
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  setBriefOnly(briefOnly: boolean) {
    this.briefOnly = briefOnly;
  }

  setLevel(volume: number, muted: boolean) {
    this.level = muted || !Number.isFinite(volume) ? 0 : Math.max(0, Math.min(1, volume));
    if (this.gain && this.context) {
      this.gain.gain.setTargetAtTime(this.level, this.context.currentTime, 0.01);
    }
  }

  isActive(): boolean {
    return this.playing || this.processing || this.queue.length > 0;
  }

  reset() {
    this.stop();
    this.finished = false;
    this.pendingBuffer = "";
    this.enqueuedInTurn = 0;
  }

  pushDelta(chunk: string) {
    if (!this.enabled || !chunk) return;
    if (this.briefOnly && this.enqueuedInTurn >= 1) return;
    this.pendingBuffer += chunk;
    this.drainSentences(false);
  }

  finish() {
    this.finished = true;
    this.drainSentences(true);
    if (!this.processing && this.queue.length === 0 && !this.playing) {
      this.setPlaying(false);
    }
  }

  stop() {
    this.generation++;
    this.pendingBuffer = "";
    this.enqueuedInTurn = 0;
    this.queue = [];
    this.processing = false;
    this.finished = true;
    if (this.currentSource) {
      try {
        this.currentSource.onended = null;
        this.currentSource.stop();
        this.currentSource.disconnect();
      } catch {
        /* ignore */
      }
      this.currentSource = undefined;
    }
    cancelAnimationFrame(this.raf);
    audioLevelRef.current = 0;
    this.setPlaying(false);
  }

  dispose() {
    this.stop();
    if (this.context && this.context.state !== "closed") {
      void this.context.close().catch(() => {});
    }
    this.context = undefined;
    this.gain = undefined;
    this.analyser = undefined;
  }

  private setPlaying(next: boolean) {
    if (this.playing !== next) {
      this.playing = next;
      this.onPlayingChange?.(next);
    }
  }

  private ensureAudio(): AudioContext {
    if (!this.context) {
      const ctx = new AudioContext();
      const gain = ctx.createGain();
      gain.gain.value = this.level;

      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;

      gain.connect(analyser).connect(ctx.destination);

      this.context = ctx;
      this.gain = gain;
      this.analyser = analyser;
    }
    if (this.context.state === "suspended") {
      void this.context.resume();
    }
    return this.context;
  }

  private clampBriefSentence(text: string): string {
    const clean = text.trim();
    if (clean.length <= 220) return clean;
    // Prefer ending cleanly at the last full sentence boundary within 220 chars
    const head = clean.slice(0, 220);
    const punctMatches = Array.from(head.matchAll(/[.!?…](?=\s|$)/g));
    const lastPunct = punctMatches.length > 0 ? punctMatches[punctMatches.length - 1] : null;
    if (lastPunct && (lastPunct.index ?? 0) >= 60) {
      return head.slice(0, (lastPunct.index ?? 0) + 1).trim();
    }
    const spaceIdx = clean.lastIndexOf(" ", 218);
    const cut = spaceIdx > 60 ? spaceIdx : 218;
    return `${clean.slice(0, cut).trim().replace(/[,;:-]+$/, "")}.`;
  }

  private drainSentences(flushAll: boolean) {
    if (!this.enabled) return;
    if (this.briefOnly && this.enqueuedInTurn >= 1) {
      this.pendingBuffer = "";
      return;
    }

    const regex = /(?<=[.!?…\n])\s+/g;

    if (this.briefOnly) {
      // Wait for at least ~75 chars before cutting on a sentence end so short greetings like "Olá!"
      // or "Pronto!" always include the actual reply that follows.
      if (!flushAll) {
        let bestEnd = 0;
        for (const match of this.pendingBuffer.matchAll(regex)) {
          const endIdx = (match.index ?? 0) + match[0].length;
          if (endIdx <= 220) {
            bestEnd = endIdx;
          }
        }
        if (bestEnd >= 75 && this.pendingBuffer.length >= 140) {
          const candidate = this.pendingBuffer.slice(0, bestEnd).trim();
          this.pendingBuffer = "";
          this.enqueuedInTurn = 1;
          this.enqueueSentence(this.clampBriefSentence(candidate));
          return;
        }
        if (this.pendingBuffer.length >= 220) {
          const candidate = this.pendingBuffer.trim();
          this.pendingBuffer = "";
          this.enqueuedInTurn = 1;
          this.enqueueSentence(this.clampBriefSentence(candidate));
        }
        return;
      }

      const candidate = this.pendingBuffer.trim();
      this.pendingBuffer = "";
      if (candidate) {
        this.enqueuedInTurn = 1;
        this.enqueueSentence(this.clampBriefSentence(candidate));
      }
      return;
    }

    if (!flushAll) {
      let lastSplit = 0;
      for (const match of this.pendingBuffer.matchAll(regex)) {
        const endIdx = (match.index ?? 0) + match[0].length;
        const candidate = this.pendingBuffer.slice(lastSplit, endIdx).trim();
        if (candidate.length >= 24) {
          this.enqueuedInTurn++;
          this.enqueueSentence(candidate);
          lastSplit = endIdx;
        }
      }
      if (lastSplit > 0) {
        this.pendingBuffer = this.pendingBuffer.slice(lastSplit);
      } else if (this.pendingBuffer.length > 220) {
        const spaceIdx = this.pendingBuffer.lastIndexOf(" ", 200);
        const cut = spaceIdx > 60 ? spaceIdx : 200;
        this.enqueuedInTurn++;
        this.enqueueSentence(this.pendingBuffer.slice(0, cut).trim());
        this.pendingBuffer = this.pendingBuffer.slice(cut).trimStart();
      }
    } else {
      const rest = this.pendingBuffer.trim();
      this.pendingBuffer = "";
      if (rest) {
        this.enqueuedInTurn++;
        this.enqueueSentence(rest);
      }
    }
  }

  private enqueueSentence(sentence: string) {
    const gen = this.generation;
    const promise = this.fetchDecodedAudio(sentence, gen);
    this.queue.push({ text: sentence, promise });
    if (!this.processing) {
      void this.processLoop(gen);
    }
  }

  private async fetchDecodedAudio(sentence: string, gen: number): Promise<AudioBuffer | null> {
    try {
      const out = await synthesizeSpeech(sentence);
      if (gen !== this.generation) return null;
      const ctx = this.ensureAudio();
      const raw = base64ToArrayBuffer(out.audio_base64);
      return await ctx.decodeAudioData(raw);
    } catch (err) {
      if (gen === this.generation && this.reportedErrorGen !== gen) {
        this.reportedErrorGen = gen;
        const msg = err instanceof Error ? err.message : String(err);
        this.onError?.(msg);
      }
      return null;
    }
  }

  private startMeter() {
    cancelAnimationFrame(this.raf);
    const analyser = this.analyser;
    if (!analyser) return;
    const data = new Uint8Array(analyser.fftSize);
    const tick = () => {
      if (!this.playing) {
        audioLevelRef.current = 0;
        return;
      }
      analyser.getByteTimeDomainData(data);
      let sum = 0;
      for (let i = 0; i < data.length; i++) {
        const centered = (data[i] - 128) / 128;
        sum += centered * centered;
      }
      const rms = Math.sqrt(sum / data.length);
      const norm = Math.min(1, rms * 4.2);
      audioLevelRef.current = audioLevelRef.current * 0.4 + norm * 0.6;
      this.raf = requestAnimationFrame(tick);
    };
    this.raf = requestAnimationFrame(tick);
  }

  private async processLoop(gen: number) {
    this.processing = true;
    while (this.queue.length > 0 && gen === this.generation) {
      const next = this.queue.shift()!;
      const buffer = await next.promise;
      if (gen !== this.generation) break;
      if (!buffer) continue;

      const ctx = this.ensureAudio();
      if (ctx.state === "suspended") {
        await ctx.resume().catch(() => {});
      }

      this.setPlaying(true);
      this.startMeter();

      await new Promise<void>((resolve) => {
        if (gen !== this.generation) {
          resolve();
          return;
        }
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(this.gain!);
        this.currentSource = source;
        source.onended = () => {
          source.disconnect();
          if (this.currentSource === source) this.currentSource = undefined;
          resolve();
        };
        source.start();
      });
    }

    if (gen === this.generation) {
      this.processing = false;
      cancelAnimationFrame(this.raf);
      audioLevelRef.current = 0;
      if (this.finished && this.queue.length === 0) {
        this.setPlaying(false);
      }
    }
  }
}

export function parseWakeCommand(transcript: string): { matched: boolean; command: string } {
  const clean = transcript.trim();
  if (!clean) return { matched: false, command: "" };
  const normalized = clean
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase();

  // Matches "jarvis", "ei jarvis", "hey jarvis", "ok jarvis", plus common PT-BR ASR slips ("jarves", "javes", "charvis")
  const match = /(?:^|\b)(?:(?:ei|hey|oi|ok|fala|e\s+ai|ola)\s+)?(?:jarvis|jarves|javes|jarviz|charvis)\b[\s,.:;!?-]*(.*)$/i.exec(
    normalized,
  );
  if (!match) return { matched: false, command: "" };

  // Extract trailing command preserving original casing/accents if present
  const rawMatch = /(?:jarvis|jarves|javes|jarviz|charvis)\b[\s,.:;!?-]*(.*)$/i.exec(clean);
  const command = (rawMatch?.[1] ?? match[1] ?? "").trim();
  return { matched: true, command };
}

export function isHandsFreeStopPhrase(transcript: string): boolean {
  const norm = transcript
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .trim();
  return /^(?:parar|para|encerrar|encerrar conversa|tchau|ate mais|pode descansar|descansar|silencio|calado|chega|obrigado jarvis|valeu jarvis)\.?$/.test(
    norm,
  );
}

interface SpeechRecognitionLike {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  maxAlternatives: number;
  onresult: ((e: {
    resultIndex: number;
    results: ArrayLike<{
      isFinal?: boolean;
      0: { transcript: string };
    }>;
  }) => void) | null;
  onerror: ((e: { error: string }) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
  abort?: () => void;
}

export class WakeWordMonitor {
  private rec: SpeechRecognitionLike | null = null;
  private stream?: MediaStream;
  private context?: AudioContext;
  private processor?: ScriptProcessorNode;
  private source?: MediaStreamAudioSourceNode;
  private muteGain?: GainNode;
  private sampleRate = 48000;
  private preRoll: Float32Array[] = [];
  private speechChunks: Float32Array[] = [];
  private capturingSpeech = false;
  private speechStartAt = 0;
  private silenceStartAt = 0;
  private transcribing = false;

  private active = false;
  private paused = false;
  private restartTimer = 0;
  private lastWakeAt = 0;
  private useCloudVad: boolean;
  private deviceId?: string;
  private onWake: (inlineCommand: string) => void;
  private onError?: (message: string) => void;
  private onLevel?: (level: number) => void;

  constructor(
    opts: {
      useCloudVad: boolean;
      deviceId?: string;
      onWake: (inlineCommand: string) => void;
      onError?: (message: string) => void;
      onLevel?: (level: number) => void;
    },
  ) {
    this.useCloudVad = opts.useCloudVad;
    this.deviceId = opts.deviceId;
    this.onWake = opts.onWake;
    this.onError = opts.onError;
    this.onLevel = opts.onLevel;
  }

  start() {
    if (this.active) return;
    this.active = true;
    this.paused = false;

    if (this.useCloudVad) {
      void this.bootAudioVad();
    } else if (isBrowserSpeechAvailable()) {
      this.bootBrowserRecognizer();
    } else {
      this.active = false;
      this.onError?.(
        "Configure sua chave ElevenLabs ou Gemini em Ajustes para ativar o Wake Word 'Ei, Jarvis'.",
      );
    }
  }

  stop() {
    this.active = false;
    this.paused = false;
    clearTimeout(this.restartTimer);
    this.teardownBrowserRecognizer();
    this.teardownAudioVad();
    this.onLevel?.(0);
  }

  setPaused(paused: boolean) {
    if (this.paused === paused) return;
    this.paused = paused;
    if (!this.active) return;

    if (this.useCloudVad) {
      if (paused) {
        this.capturingSpeech = false;
        this.speechChunks = [];
        this.preRoll = [];
        this.onLevel?.(0);
      }
      return;
    }

    clearTimeout(this.restartTimer);
    if (paused) {
      this.teardownBrowserRecognizer();
    } else {
      this.restartTimer = window.setTimeout(() => {
        if (this.active && !this.paused) this.bootBrowserRecognizer();
      }, 320);
    }
  }

  private async bootAudioVad() {
    try {
      this.stream = await openMicStream(this.deviceId);
      if (!this.active) {
        this.stream.getTracks().forEach((t) => t.stop());
        this.stream = undefined;
        return;
      }
      const context = new AudioContext();
      this.context = context;
      if (context.state === "suspended") await context.resume();
      this.sampleRate = context.sampleRate;

      const source = context.createMediaStreamSource(this.stream);
      const processor = context.createScriptProcessor(2048, 1, 1);
      const muteGain = context.createGain();
      muteGain.gain.value = 0;

      const maxPreRollChunks = Math.max(4, Math.ceil((this.sampleRate * 0.45) / 2048));
      this.preRoll = [];
      this.speechChunks = [];
      this.capturingSpeech = false;
      this.silenceStartAt = 0;

      processor.onaudioprocess = (ev) => {
        if (!this.active || this.paused) return;
        const raw = ev.inputBuffer.getChannelData(0);
        let sum = 0;
        for (let i = 0; i < raw.length; i++) sum += raw[i] * raw[i];
        const rms = Math.sqrt(sum / Math.max(1, raw.length));
        const norm = Math.min(1, rms * 7.5);
        this.onLevel?.(norm);

        if (this.transcribing) return;

        const copy = new Float32Array(raw);
        const now = performance.now();

        // Voice Activity Detection (runs 100% locally — 0 network calls during silence)
        if (rms > 0.018) {
          if (!this.capturingSpeech) {
            this.capturingSpeech = true;
            this.speechStartAt = now;
            this.speechChunks = [...this.preRoll, copy];
          } else {
            this.speechChunks.push(copy);
          }
          this.silenceStartAt = 0;

          // Flush if user spoke continuously for 3.8s (e.g. "Ei Jarvis, abre o YouTube agora")
          if (now - this.speechStartAt >= 3800) {
            this.flushSpeechForWakeCheck();
          }
        } else if (this.capturingSpeech) {
          this.speechChunks.push(copy);
          if (this.silenceStartAt === 0) {
            this.silenceStartAt = now;
          } else if (now - this.silenceStartAt >= 680) {
            this.flushSpeechForWakeCheck();
          }
        } else {
          this.preRoll.push(copy);
          if (this.preRoll.length > maxPreRollChunks) {
            this.preRoll.shift();
          }
        }
      };

      source.connect(processor);
      processor.connect(muteGain);
      muteGain.connect(context.destination);

      this.source = source;
      this.processor = processor;
      this.muteGain = muteGain;
    } catch (err) {
      this.active = false;
      this.teardownAudioVad();
      this.onError?.(
        err instanceof Error
          ? `Microfone não captado para o Wake Word: ${err.message}. Verifique se o microfone está conectado e permitido no Windows.`
          : "Não foi possível abrir o microfone para o Wake Word.",
      );
    }
  }

  private flushSpeechForWakeCheck() {
    const chunks = this.speechChunks;
    this.speechChunks = [];
    this.capturingSpeech = false;
    this.silenceStartAt = 0;

    const totalSamples = chunks.reduce((acc, c) => acc + c.length, 0);
    // Ignore tiny accidental clicks/pops (< 0.35s)
    if (totalSamples < this.sampleRate * 0.35) return;

    const wav = encodeWav16k(chunks, this.sampleRate);
    const b64 = bytesToBase64(wav);
    this.transcribing = true;

    void transcribeAudio(b64, "audio/wav")
      .then((text) => {
        if (!this.active || this.paused) return;
        const parsed = parseWakeCommand(text);
        if (!parsed.matched) return;
        const now = Date.now();
        if (now - this.lastWakeAt < 1800) return;
        this.lastWakeAt = now;
        this.onWake(parsed.command);
      })
      .catch(() => {
        // Ignore transient STT hiccups during background wake listening
      })
      .finally(() => {
        this.transcribing = false;
      });
  }

  private teardownAudioVad() {
    this.capturingSpeech = false;
    this.speechChunks = [];
    this.preRoll = [];
    this.processor?.disconnect();
    this.source?.disconnect();
    this.muteGain?.disconnect();
    this.stream?.getTracks().forEach((t) => t.stop());
    if (this.context && this.context.state !== "closed") {
      void this.context.close().catch(() => {});
    }
    this.processor = undefined;
    this.source = undefined;
    this.muteGain = undefined;
    this.stream = undefined;
    this.context = undefined;
  }

  private teardownBrowserRecognizer() {
    if (!this.rec) return;
    const r = this.rec;
    this.rec = null;
    r.onresult = null;
    r.onerror = null;
    r.onend = null;
    try {
      r.abort ? r.abort() : r.stop();
    } catch {
      /* ignore */
    }
  }

  private bootBrowserRecognizer() {
    if (!this.active || this.paused) return;
    this.teardownBrowserRecognizer();

    const w = window as unknown as Record<string, new () => SpeechRecognitionLike>;
    const Ctor = w.SpeechRecognition || w.webkitSpeechRecognition;
    if (!Ctor) return;

    const rec = new Ctor();
    rec.lang = "pt-BR";
    rec.continuous = true;
    rec.interimResults = true;
    rec.maxAlternatives = 1;

    rec.onresult = (e) => {
      if (!this.active || this.paused) return;
      for (let i = e.resultIndex; i < e.results.length; i++) {
        const res = e.results[i];
        const text = res?.[0]?.transcript?.trim() ?? "";
        if (!text) continue;
        const parsed = parseWakeCommand(text);
        if (!parsed.matched) continue;

        const now = Date.now();
        if (now - this.lastWakeAt < 1800) continue;

        if (parsed.command.length > 0 && !res.isFinal) {
          continue;
        }
        if (parsed.command.length === 0 || res.isFinal) {
          this.lastWakeAt = now;
          this.teardownBrowserRecognizer();
          this.onWake(parsed.command);
          return;
        }
      }
    };

    rec.onerror = (e) => {
      if (!this.active) return;
      if (
        e.error === "not-allowed" ||
        e.error === "service-not-allowed" ||
        e.error === "network" ||
        e.error === "audio-capture"
      ) {
        this.active = false;
        this.teardownBrowserRecognizer();
        this.onError?.(
          e.error === "network"
            ? "O WebView2 não tem reconhecimento de voz embutido. Configure ElevenLabs ou Gemini em Ajustes → Voz para o 'Ei, Jarvis' funcionar!"
            : e.error === "audio-capture"
              ? "Nenhum microfone detectado pelo Windows."
              : "Permissão de microfone negada.",
        );
      }
    };

    rec.onend = () => {
      this.rec = null;
      if (!this.active || this.paused) return;
      clearTimeout(this.restartTimer);
      this.restartTimer = window.setTimeout(() => {
        if (this.active && !this.paused) this.bootBrowserRecognizer();
      }, 420);
    };

    this.rec = rec;
    try {
      rec.start();
    } catch {
      this.rec = null;
      this.restartTimer = window.setTimeout(() => {
        if (this.active && !this.paused) this.bootBrowserRecognizer();
      }, 900);
    }
  }
}

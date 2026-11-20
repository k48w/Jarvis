type State = 'idle' | 'listening' | 'thinking' | 'responding' | 'alert'
type Sound = Exclude<State, 'idle'>
type Material = 'glass' | 'wood' | 'skin' | 'alloy'

interface Strike {
  at: number
  material: Material
  pitch: number
  weight: number
  pan: number
}

// Ratios and damping describe the resonances excited by each contact.
const materials = {
  glass: { ratios: [1, 2.37, 3.84], decay: 0.095, bass: 0.12, air: 0.22 },
  wood:  { ratios: [1, 1.49, 2.81], decay: 0.028, bass: 0.38, air: 0.32 },
  skin:  { ratios: [1, 1.59, 2.14], decay: 0.048, bass: 0.92, air: 0.12 },
  alloy: { ratios: [1, 1.44, 2.68], decay: 0.073, bass: 0.57, air: 0.27 },
}

const cues: Record<Sound, { duration: number, peakDb: number, strikes: Strike[] }> = {
  listening: { duration: 0.46, peakDb: -12, strikes: [
    { at: 0, material: 'wood', pitch: 265, weight: 0.65, pan: -0.22 },
    { at: 0.045, material: 'glass', pitch: 645, weight: 0.8, pan: 0.28 },
  ] },
  thinking: { duration: 0.32, peakDb: -16, strikes: [
    { at: 0, material: 'wood', pitch: 380, weight: 1, pan: -0.35 },
    { at: 0.105, material: 'wood', pitch: 485, weight: 0.58, pan: 0.4 },
  ] },
  responding: { duration: 0.8, peakDb: -18, strikes: [
    { at: 0, material: 'skin', pitch: 178, weight: 1, pan: -0.14 },
    { at: 0.31, material: 'wood', pitch: 255, weight: 0.56, pan: 0.22 },
  ] },
  alert: { duration: 0.5, peakDb: -9, strikes: [
    { at: 0, material: 'alloy', pitch: 486, weight: 1, pan: -0.19 },
    { at: 0.15, material: 'alloy', pitch: 359, weight: 0.87, pan: 0.19 },
  ] },
}

function synthesize(context: AudioContext, state: Sound): AudioBuffer {
  const { duration, strikes, peakDb } = cues[state]
  const rate = 44100
  const tau = Math.PI * 2
  const buffer = context.createBuffer(2, Math.round(duration * rate), rate)
  const left = buffer.getChannelData(0)
  const right = buffer.getChannelData(1)
  for (const strike of strikes) {
    const { ratios, decay, air, bass } = materials[strike.material]
    const start = Math.round(strike.at * rate)
    const angle = (strike.pan + 1) * Math.PI / 4
    const l = Math.cos(angle)
    const r = Math.sin(angle)
    let seed = Math.round(strike.pitch * 193)
    let fast = 0
    let slow = 0
    for (let i = start; i < Math.min(left.length, start + 8 * decay * rate); i++) {
      const t = (i - start) / rate
      const phase = tau * strike.pitch * t
      const ring = Math.exp(-t / decay)
      const body = ring * (Math.sin(phase) + 0.38 * ring * Math.sin(phase * ratios[1]) +
        0.19 * ring * ring * Math.sin(phase * ratios[2]))
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
      fast += 0.45 * (seed / 2147483648 - 1 - fast)
      slow += 0.055 * (fast - slow)
      const brush = air * (fast - slow) * (2 * Math.exp(-t / 0.009) + ring)
      const attack = Math.min(1, t / 0.005) * strike.weight
      const contact = attack * (body + brush)
      // Center the low impact and pan only its brighter contact.
      const lowPhase = tau * (72 * t + 0.94 * (1 - Math.exp(-t / 0.022)))
      const low = attack * bass * Math.exp(-t / 0.057) * Math.sin(lowPhase)
      left[i] += contact * l + low
      right[i] += contact * r + low
    }
  }

  // Cross-channel early reflections. Work backwards to keep the taps finite.
  const near = Math.round(0.017 * rate)
  const far = Math.round(0.031 * rate)
  let peak = 0
  for (let i = left.length - 1; i >= 0; i--) {
    if (i >= near) left[i] += 0.14 * right[i - near]
    if (i >= far) right[i] += 0.11 * left[i - far]
    const tail = Math.min(1, (left.length - 1 - i) / (0.024 * rate))
    left[i] *= tail
    right[i] *= tail
    peak = Math.max(peak, Math.abs(left[i]), Math.abs(right[i]))
  }
  const gain = 10 ** (peakDb / 20) / peak
  for (let i = 0; i < left.length; i++) {
    left[i] *= gain
    right[i] *= gain
  }
  return buffer
}

export class AudioEngine {
  private context?: AudioContext
  private output?: GainNode
  private buffers = new Map<Sound, AudioBuffer>()
  private voice?: { source: AudioBufferSourceNode, gain: GainNode }
  private state: State = 'idle'
  private version = 0
  private level = 1
  private disposed = false

  async cue(state: State, options?: { suppressLoop?: boolean }): Promise<void> {
    if (this.disposed || typeof AudioContext === 'undefined') return
    const shouldLoop = state === 'responding' && !options?.suppressLoop
    if (shouldLoop && this.state === state && this.voice &&
        this.context?.state === 'running') return
    const version = ++this.version
    const eventTime = performance.now()
    this.state = state
    this.stopVoice()
    if (state === 'idle') return

    if (!this.context) {
      this.context = new AudioContext()
      this.output = this.context.createGain()
      this.output.gain.value = this.level
      this.output.connect(this.context.destination)
    }
    const context = this.context
    if (context.state !== 'running') {
      try {
        await context.resume()
      } catch (error) {
        if (version === this.version) throw error
        return
      }
    }
    if (version !== this.version || context.state !== 'running') return

    let buffer = this.buffers.get(state)
    if (!buffer) {
      buffer = synthesize(context, state)
      this.buffers.set(state, buffer)
    }
    // Anchor playback to the visual event even if autoplay delayed resume().
    const elapsed = (performance.now() - eventTime) / 1000
    if (state !== 'responding' && elapsed >= buffer.duration) return
    const source = context.createBufferSource()
    source.buffer = buffer
    source.loop = state === 'responding' && !options?.suppressLoop
    const gain = context.createGain()
    const now = context.currentTime
    gain.gain.setValueAtTime(0, now)
    gain.gain.linearRampToValueAtTime(1, now + 0.005)
    source.connect(gain).connect(this.output!)
    source.onended = () => {
      source.disconnect()
      gain.disconnect()
      if (this.voice?.source === source) this.voice = undefined
    }
    source.start(now, elapsed % buffer.duration)
    this.voice = { source, gain }
  }

  setLevel(volume: number, muted: boolean): void {
    this.level = muted || !Number.isFinite(volume) ? 0 : Math.max(0, Math.min(1, volume))
    if (this.output && this.context) {
      this.output.gain.setTargetAtTime(this.level, this.context.currentTime, 0.004)
    }
  }

  private stopVoice(): void {
    if (!this.voice || !this.context) return
    const { gain, source } = this.voice
    this.voice = undefined
    const now = this.context.currentTime
    gain.gain.cancelAndHoldAtTime(now)
    gain.gain.linearRampToValueAtTime(0, now + 0.008)
    source.stop(now + 0.008)
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.version++
    this.stopVoice()
    this.output?.disconnect()
    if (this.context && this.context.state !== 'closed') void this.context.close()
    this.context = undefined
    this.output = undefined
    this.buffers.clear()
  }
}


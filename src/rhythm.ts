// Frequencies are in cycles per second. The renderer integrates the blended
// frequency so a change of state never restarts the heart.
export const cadence = {
  idle: .53, listening: .72, thinking: 1.02, responding: .86, alert: 1.32,
} as const;

export type CoreState = keyof typeof cadence;

export function beatPhase(seconds: number, state: CoreState = 'idle') {
  return seconds * cadence[state];
}

export function contraction(phase: number) {
  return Math.pow(.5 + .5 * Math.cos(2 * Math.PI * phase), 4);
}

export function overtone(phase: number) {
  return Math.pow(.5 + .5 * Math.cos(2 * Math.PI * (2 * phase + .37)), 6);
}

// Normalized amplitude of the responding buffer synthesized in audio.ts.
function noteEnvelope(age: number, duration: number) {
  if (age < 0 || age >= duration) return 0;
  return Math.min(1, age / .008) * (1 - age / duration) ** 2;
}

export function respondingEnvelope(secondsSinceOnset: number) {
  const t = Math.max(0, secondsSinceOnset) % .8;
  return noteEnvelope(t, .15) + (.042 / .075) * noteEnvelope(t - .31, .12);
}

# Assistant audio cues

Call `audio.cue(state)` at the visual transition. Each audible cue combines a short contact transient, damped resonances, filtered air and a centered bass impact. The stereo position changes between strikes. Quiet reflections arrive at 17 and 31 ms, giving the contact a sense of distance. Each 44.1 kHz stereo buffer is synthesized locally and cached on first use.

| State | Gesture | Length | Sample peak at unity gain |
| --- | --- | --- | --- |
| idle | Fade the current sound out over 8 ms, then silence | — | — |
| listening | A wooden contact on the left opens into soft glass on the right | 460 ms | −12 dBFS |
| thinking | Two short, dry wooden taps cross the stereo field | 320 ms | −16 dBFS |
| responding | A low membrane pulse followed by a lighter wooden accent | 800 ms loop | −18 dBFS |
| alert | Two descending alloy strikes with a low impact beneath the ring | 500 ms | −9 dBFS |

Responding starts its accents at 0 and 310 ms of each 800 ms cycle, matching the visual envelope in `src/rhythm.ts`. Repeating the cue during playback preserves its phase. Each channel fades to zero at the buffer boundary.

The first audible cue requests playback. Call it from a user gesture when possible. After a delayed unlock, the engine skips expired one-shots and joins a responding loop at the elapsed phase. A newer cue or disposal invalidates pending playback.

Use `audio.setLevel(volume, muted)` before or during playback, with volume from 0 to 1. Muting preserves response timing. Call `audio.dispose()` when the view is destroyed.

## Verification

The module passed strict TypeScript compilation targeting ES2022. A temporary headless Chromium harness exercised a real AudioContext with networking disabled.

- Scanned both channels of each generated buffer. The four sample peaks matched the table within 0.01 dB. Stereo difference energy was 12.5–18.8 dB below sum energy across the four cues. Both channels ended at zero.
- Checked both responding accents. Their left-channel peaks fell within 25 ms of their visual onsets, and the second accent was quieter. Repeating the state kept the same source alive into the second cycle.
- Measured live output with an AnalyserNode. Each one-shot produced a signal and ended naturally. Mute and idle brought the measured peak below 0.000001, and unmute restored the loop.
- Delayed a real context resume during competing state changes. Only the newest cue played. An expired one-shot, idle transition and disposal each suppressed pending playback.

Speaker and headphone listening in the packaged application remain unverified.

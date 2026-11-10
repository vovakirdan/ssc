import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { VoiceCapture, voiceFormat, playableVoiceFormats } from './recorder';
import { encodeWav } from './wav';
let stopTrack: ReturnType<typeof vi.fn>;
class FakeRecorder {
  static isTypeSupported(value: string) { return value.startsWith('audio/webm'); }
  state = 'inactive'; mimeType = 'audio/webm;codecs=opus';
  ondataavailable: ((event: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null; onerror: (() => void) | null = null;
  start() { this.state = 'recording'; }
  stop() { this.state = 'inactive'; queueMicrotask(() => { this.ondataavailable?.({ data: new Blob(['fixture audio'], { type: this.mimeType }) }); this.onstop?.(); }); }
}
beforeEach(() => { stopTrack = vi.fn(); Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [{ stop: stopTrack }] }) } }); vi.stubGlobal('MediaRecorder', FakeRecorder); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it('records only after explicit start and stops all tracks before returning a draft', async () => {
  const capture = new VoiceCapture(vi.fn()); expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
  expect(await capture.start(['audio/webm;codecs=opus'])).toBe(true);
  const draft = await capture.stop(); expect(stopTrack).toHaveBeenCalled(); expect(draft?.mime).toBe('audio/webm;codecs=opus'); capture.cancel();
});
it('cancels a microphone permission result which arrives after exit', async () => {
  let resolve!: (stream: MediaStream) => void;
  vi.mocked(navigator.mediaDevices.getUserMedia).mockReturnValue(new Promise(value => { resolve = value; }));
  const capture = new VoiceCapture(vi.fn()), starting = capture.start(['audio/webm;codecs=opus']); capture.cancel();
  resolve({ getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream);
  expect(await starting).toBe(false); expect(stopTrack).toHaveBeenCalled();
});
it('cancellation settles a pending stop without keeping microphone tracks active', async () => {
  const capture = new VoiceCapture(vi.fn()); await capture.start(['audio/webm;codecs=opus']);
  const stopping = capture.stop(); capture.cancel(); expect(await stopping).toBeNull(); expect(stopTrack).toHaveBeenCalled();
});
it('automatically ends recording after five minutes without sending it', async () => {
  vi.useFakeTimers(); const limit = vi.fn(), capture = new VoiceCapture(limit); await capture.start(['audio/webm;codecs=opus']);
  await vi.advanceTimersByTimeAsync(300_000); expect(stopTrack).toHaveBeenCalled(); expect(limit).toHaveBeenCalledOnce(); capture.cancel();
});
it('selects only a shared recording MIME and advertises playback without microphone access', () => {
  expect(voiceFormat(['audio/mp4'])).toBeNull(); expect(voiceFormat(['audio/webm;codecs=opus'])).toBe('audio/webm;codecs=opus');
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation(value => value === 'audio/wav' ? 'probably' : '');
  expect(playableVoiceFormats()).toEqual(['audio/wav']); expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
});
it('creates bounded mono PCM WAV bytes without metadata fields', async () => {
  const blob = encodeWav([new Float32Array([0, .5, -1, 1])], 16_000);
  expect(blob.type).toBe('audio/wav'); expect(blob.size).toBe(44 + 8);
});

it('records PCM fallback through a self-hosted worklet and closes every resource', async () => {
  const addModule = vi.fn().mockResolvedValue(undefined), close = vi.fn().mockResolvedValue(undefined), disconnect = vi.fn(), portClose = vi.fn();
  const nodes: { port: { onmessage: ((event: { data: Float32Array }) => void) | null; close(): void }; connect(): void; disconnect(): void }[] = [];
  class Context {
    sampleRate = 48_000; destination = {}; audioWorklet = { addModule };
    createGain() { return { gain: { value: 1 }, connect: vi.fn(), disconnect }; }
    createMediaStreamSource() { return { connect: vi.fn(), disconnect }; }
    resume = vi.fn().mockResolvedValue(undefined); close = close;
  }
  class Worklet {
    port = { onmessage: null, close: portClose }; connect = vi.fn(); disconnect = disconnect;
    constructor() { nodes.push(this); }
  }
  vi.stubGlobal('AudioContext', Context); vi.stubGlobal('AudioWorkletNode', Worklet);
  const capture = new VoiceCapture(vi.fn());
  expect(await capture.start(['audio/wav'])).toBe(true);
  expect(addModule).toHaveBeenCalledOnce(); expect(addModule.mock.calls[0][0]).not.toMatch(/^(blob:|data:)/);
  const pcm = new Float32Array(96).fill(.25); nodes[0].port.onmessage!({ data: pcm });
  const draft = await capture.stop();
  expect(draft?.mime).toBe('audio/wav'); expect(draft?.blob.size).toBe(44 + 32 * 2);
  expect(pcm.every(sample => sample === 0)).toBe(true); expect(stopTrack).toHaveBeenCalled(); expect(close).toHaveBeenCalled(); expect(portClose).toHaveBeenCalled();
  capture.cancel();
});
it('normalizes AAC playback support to the canonical three-format wire list', () => {
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation(value => value === 'audio/mp4;codecs=mp4a.40.2' ? 'maybe' : '');
  expect(playableVoiceFormats()).toEqual(['audio/mp4']);
});

import workletUrl from './recorder-worklet.js?url&no-inline';
import { encodeWav } from './wav';
export const MAX_VOICE_MILLISECONDS = 5 * 60_000;
export interface VoiceDraft { blob: Blob; mime: string; extension: string; duration: number }
const formats = ['audio/webm;codecs=opus', 'audio/mp4;codecs=mp4a.40.2', 'audio/mp4'];
export function playableVoiceFormats(): string[] {
  if (typeof document === 'undefined') return [];
  const audio = document.createElement('audio');
  const result: string[] = [];
  if (audio.canPlayType('audio/webm;codecs=opus')) result.push('audio/webm;codecs=opus');
  if (audio.canPlayType('audio/mp4') || audio.canPlayType('audio/mp4;codecs=mp4a.40.2')) result.push('audio/mp4');
  if (audio.canPlayType('audio/wav')) result.push('audio/wav');
  return result;
}
export function voiceFormat(peer: readonly string[]): string | null {
  if (typeof MediaRecorder === 'undefined' || !MediaRecorder.isTypeSupported) return null;
  return formats.find(format => peer.some(value => value.toLowerCase() === format.toLowerCase() || value.toLowerCase() === format.split(';')[0]) && MediaRecorder.isTypeSupported(format)) || null;
}
export class VoiceCapture {
  private stream: MediaStream | null = null;
  private recorder: MediaRecorder | null = null;
  private context: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private processor: ScriptProcessorNode | null = null;
  private worklet: AudioWorkletNode | null = null;
  private resamplePosition = 0;
  private recorderMime = '';
  private gain: GainNode | null = null;
  private chunks: Blob[] = [];
  private pcm: Float32Array[] = [];
  private samples = 0;
  private sampleRate = 16_000;
  private epoch = 0;
  private started = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private finishing: Promise<VoiceDraft | null> | null = null;
  private settleStop: (() => void) | null = null;
  constructor(private readonly limit: (draft: VoiceDraft | null) => void) {}
  async start(peer: readonly string[]): Promise<boolean> {
    this.cancel(); const epoch = this.epoch;
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('microphone_unavailable');
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true }, video: false });
    if (epoch !== this.epoch || document.visibilityState === 'hidden') { stream.getTracks().forEach(track => track.stop()); return false; }
    this.stream = stream; this.started = performance.now();
    const mime = voiceFormat(peer);
    try {
      if (mime) {
        const recorder = new MediaRecorder(stream, { mimeType: mime, audioBitsPerSecond: 64_000 }); this.recorder = recorder; this.recorderMime = mime;
        recorder.ondataavailable = event => { if (epoch === this.epoch && event.data.size) this.chunks.push(event.data); };
        recorder.onerror = () => { if (epoch === this.epoch) { this.cancel(); this.limit(null); } };
        recorder.start(1000);
      } else {
        const Audio = globalThis.AudioContext || (globalThis as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
        if (!Audio) throw new Error('audio_context_unavailable');
        try { this.context = new Audio({ sampleRate: 16_000 }); } catch { this.context = new Audio(); }
        const context = this.context; this.sampleRate = context.sampleRate;
        this.gain = context.createGain(); this.gain.gain.value = 0;
        if (context.audioWorklet && typeof AudioWorkletNode !== 'undefined') {
          await context.audioWorklet.addModule(workletUrl);
          if (epoch !== this.epoch) return false;
          const node = new AudioWorkletNode(context, 'ssc-voice-capture', { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [1] });
          this.worklet = node;
          node.port.onmessage = event => { if (event.data instanceof Float32Array) { if (epoch === this.epoch) this.pushPcm(event.data); event.data.fill(0); } };
          this.source = context.createMediaStreamSource(stream); this.source.connect(node); node.connect(this.gain);
        } else {
          this.processor = context.createScriptProcessor(4096, 1, 1);
          this.processor.onaudioprocess = event => { if (epoch === this.epoch) this.pushPcm(event.inputBuffer.getChannelData(0)); };
          this.source = context.createMediaStreamSource(stream); this.source.connect(this.processor); this.processor.connect(this.gain);
        }
        this.gain.connect(context.destination);
        await context.resume();
        if (epoch !== this.epoch) return false;
      }
      this.timer = setTimeout(() => { void this.stop().then(draft => { if (epoch === this.epoch) this.limit(draft); }).catch(() => { if (epoch === this.epoch) { this.cancel(); this.limit(null); } }); }, MAX_VOICE_MILLISECONDS);
      return true;
    } catch { if (epoch === this.epoch) this.cancel(); throw new Error('microphone_unavailable'); }
  }
  async stop(): Promise<VoiceDraft | null> {
    if (this.finishing) return this.finishing;
    if (!this.stream) return null;
    clearTimeout(this.timer);
    const duration = Math.min(MAX_VOICE_MILLISECONDS, Math.max(0, performance.now() - this.started));
    const epoch = this.epoch, recorder = this.recorder;
    this.finishing = (async () => {
      let blob: Blob;
      if (recorder) {
        await new Promise<void>(resolve => {
          this.settleStop = resolve; recorder.onstop = () => resolve(); recorder.onerror = () => resolve();
          if (recorder.state === 'inactive') resolve(); else recorder.stop();
          this.stream?.getTracks().forEach(track => track.stop());
        });
        blob = new Blob(this.chunks, { type: recorder.mimeType || this.recorderMime });
      } else {
        this.stream?.getTracks().forEach(track => track.stop());
        blob = encodeWav(this.pcm, 16_000);
      }
      if (epoch !== this.epoch || !blob.size) return null;
      const mime = blob.type || 'audio/wav';
      return { blob, mime, extension: mime.startsWith('audio/webm') ? 'webm' : mime.startsWith('audio/mp4') ? 'm4a' : 'wav', duration };
    })().finally(() => { if (epoch === this.epoch) { this.release(); this.finishing = null; this.settleStop = null; } });
    return this.finishing;
  }
  cancel() { this.epoch++; this.settleStop?.(); this.settleStop = null; this.finishing = null; clearTimeout(this.timer); if (this.recorder?.state !== 'inactive') try { this.recorder?.stop(); } catch { /* Already stopped. */ } this.release(); }
  private pushPcm(input: Float32Array) {
    const step = this.sampleRate / 16_000, remaining = Math.max(0, 16_000 * 300 - this.samples);
    const length = Math.min(remaining, Math.max(0, Math.ceil((input.length - this.resamplePosition) / step)));
    if (!length) return;
    const copy = new Float32Array(length); let position = Math.max(0, this.resamplePosition);
    for (let i = 0; i < length; i++, position += step) {
      const left = Math.floor(position), right = Math.min(left + 1, input.length - 1);
      copy[i] = input[left] * (1 - (position - left)) + input[right] * (position - left);
    }
    this.resamplePosition = Math.max(0, position - input.length); this.samples += copy.length; this.pcm.push(copy);
  }
  private release() {
    this.stream?.getTracks().forEach(track => track.stop()); this.stream = null;
    if (this.recorder) { this.recorder.ondataavailable = null; this.recorder.onerror = null; this.recorder.onstop = null; this.recorder = null; }
    this.source?.disconnect(); this.source = null;
    if (this.processor) { this.processor.onaudioprocess = null; this.processor.disconnect(); this.processor = null; }
    if (this.worklet) { this.worklet.port.onmessage = null; this.worklet.port.close(); this.worklet.disconnect(); this.worklet = null; }
    this.gain?.disconnect(); this.gain = null;
    if (this.context) { void this.context.close().catch(() => undefined); this.context = null; }
    for (const chunk of this.pcm) chunk.fill(0); this.pcm = []; this.samples = 0; this.resamplePosition = 0; this.chunks = []; this.recorderMime = '';
  }
}

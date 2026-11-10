export function encodeWav(chunks: readonly Float32Array[], sampleRate: number, outputRate = 16_000): Blob {
  const length = chunks.reduce((sum, part) => sum + part.length, 0);
  const source = new Float32Array(length); let offset = 0;
  for (const chunk of chunks) { source.set(chunk, offset); offset += chunk.length; }
  const count = Math.floor(length * outputRate / sampleRate), buffer = new ArrayBuffer(44 + count * 2), view = new DataView(buffer);
  const text = (position: number, value: string) => { for (let i = 0; i < value.length; i++) view.setUint8(position + i, value.charCodeAt(i)); };
  text(0, 'RIFF'); view.setUint32(4, buffer.byteLength - 8, true); text(8, 'WAVE'); text(12, 'fmt '); view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true); view.setUint32(24, outputRate, true); view.setUint32(28, outputRate * 2, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true); text(36, 'data'); view.setUint32(40, count * 2, true);
  for (let i = 0; i < count; i++) {
    const index = i * sampleRate / outputRate, left = Math.min(Math.floor(index), Math.max(0, source.length - 1)), right = Math.min(left + 1, source.length - 1);
    const value = Math.max(-1, Math.min(1, source[left] * (1 - (index - left)) + source[right] * (index - left)));
    view.setInt16(44 + i * 2, Math.round(value * (value < 0 ? 32768 : 32767)), true);
  }
  source.fill(0); const blob = new Blob([buffer], { type: 'audio/wav' }); new Uint8Array(buffer).fill(0); return blob;
}

export const MEDIA_WARNING = 'Содержимое и метаданные могут раскрыть местоположение, дату, время и личность';
const photos = new Set(['image/jpeg', 'image/png', 'image/webp', 'image/gif']);
const audio = new Set(['audio/webm', 'audio/ogg', 'audio/mp4', 'audio/mpeg', 'audio/wav', 'audio/x-wav', 'audio/aac']);
const video = new Set(['video/mp4', 'video/webm', 'video/ogg']);
export function formatBytes(size: number): string { return size >= 1024 * 1024 ? `${(size / (1024 * 1024)).toFixed(1)} МиБ` : size >= 1024 ? `${Math.ceil(size / 1024)} КиБ` : `${size} Б`; }
export function previewKind(mime: string, kind: string): 'photo' | 'audio' | 'video' | null {
  const base = mime.split(';')[0].trim().toLowerCase();
  if (kind === 'photo' && photos.has(base)) return 'photo';
  if (kind === 'audio' && audio.has(base) && document.createElement('audio').canPlayType(mime)) return 'audio';
  if (kind === 'video' && video.has(base) && document.createElement('video').canPlayType(mime)) return 'video';
  return null;
}
export function safeFileName(name: string): string { return Array.from(name, character => character === '/' || character === '\\' || character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127 ? '_' : character).join(''); }

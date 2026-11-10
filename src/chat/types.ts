export type MediaKind = 'photo' | 'video' | 'audio' | 'file';
export type ReactionEmoji = '👍' | '❤️' | '😂' | '😮' | '😢' | '🔥';
export const REACTIONS: readonly ReactionEmoji[] = ['👍', '❤️', '😂', '😮', '😢', '🔥'];
export const MAX_FILE_BYTES = 100 * 1024 * 1024;
export const MAX_CACHE_BYTES = 200 * 1024 * 1024;
export const AUTO_ACCEPT_BYTES = 5 * 1024 * 1024;
export const CHUNK_BYTES = 16 * 1024;
export type ChatEvent =
  | { type: 'text'; id: string; text: string }
  | { type: 'attachment_offer'; id: string; name: string; mime: string; size: number; sha256: string; kind: MediaKind }
  | { type: 'attachment_accept' | 'attachment_reject' | 'attachment_complete' | 'attachment_cancel' | 'receipt'; id: string }
  | { type: 'attachment_chunk'; id: string; offset: number; data: string }
  | { type: 'attachment_progress'; id: string; received: number }
  | { type: 'reaction'; target: string; emoji: ReactionEmoji | null }
  | { type: 'bye'; reason: 'exit' | 'expired' }
  | { type: 'capabilities'; audio: string[] };
export type AttachmentStatus = 'offered' | 'waiting' | 'receiving' | 'sending' | 'complete' | 'rejected' | 'cancelled' | 'failed' | 'forgotten';
export interface Attachment {
  id: string; name: string; mime: string; size: number; sha256: string; kind: MediaKind;
  status: AttachmentStatus; progress: number; file?: Blob; url?: string;
}
export interface ChatItem {
  id: string; own: boolean; text?: string; attachment?: Attachment;
  status: 'pending' | 'sent' | 'delivered' | 'failed'; timestamp: number;
  reactions: { own: ReactionEmoji | null; peer: ReactionEmoji | null };
}
export interface ConversationSnapshot {
  messages: ChatItem[]; closed: boolean; notice: string | null; cacheBytes: number; peerAudioFormats: string[];
}
export interface CaptureStatus { platform: string; enabled: boolean; limitation: string | null }
export interface ChatTransport {
  send(event: ChatEvent): Promise<void>;
  isReady(): boolean;
  onPeerExit(reason: 'exit' | 'expired' | 'invalid'): void;
  createHasher(id: string): Promise<HashSink>;
}
export interface HashSink { update(bytes: Uint8Array): Promise<void>; finish(): Promise<string>; abort(): void }

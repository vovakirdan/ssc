export interface Message {
    id: string;
    text: string;
    timestamp: Date;
    isOwn: boolean;
    // Поля для связанных сообщений (части длинного сообщения)
    groupId?: string; // ID группы сообщений
    partIndex?: number; // Индекс части (0, 1, 2...)
    totalParts?: number; // Общее количество частей
    // Медиа-поля
    media?: {
      id: string;
      name: string;
      mime: string;
      size: number;
      dataUrl?: string; // data:<mime>;base64,<...>
      progress?: number; // 0..1 для отправки
    }
  }
  export type ConnectionStatus = 'connected' | 'problem' | 'recovering' | 'disconnected';
  
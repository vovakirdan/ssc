export interface Message {
    id: string;
    text: string;
    timestamp: Date;
    isOwn: boolean;
    // Поля для связанных сообщений (части длинного сообщения)
    groupId?: string; // ID группы сообщений
    partIndex?: number; // Индекс части (0, 1, 2...)
    totalParts?: number; // Общее количество частей
  }
  export type ConnectionStatus = 'connected' | 'problem' | 'recovering' | 'disconnected';
  
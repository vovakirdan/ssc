export interface Message {
    id: string;
    text: string;
    timestamp: Date;
    isOwn: boolean;
  }
  export type ConnectionStatus = 'connected' | 'problem' | 'recovering' | 'disconnected';
  
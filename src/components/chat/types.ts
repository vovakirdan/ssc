export interface Message {
  id: string;
  text: string;
  timestamp: Date;
  isOwn: boolean;
  status?: 'pending' | 'sent' | 'failed';
}

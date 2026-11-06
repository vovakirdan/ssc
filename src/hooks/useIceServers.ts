import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '@/hooks/use-toast';
import { errorMessage } from '@/lib/session';

export interface ServerConfig {
  id: string;
  type: 'stun' | 'turn';
  url: string;
  username?: string;
  credential?: string;
}
export const useIceServers = () => {
  const [isLoading, setIsLoading] = useState(false);
  const { toast } = useToast();
  const setIceServers = async (servers: ServerConfig[]) => {
    setIsLoading(true);
    try { await invoke('set_ice_servers', { servers }); return true; }
    catch (failure) {
      toast({ title: 'Ошибка', description: errorMessage(failure, 'Не удалось применить ICE серверы'), variant: 'destructive' });
      return false;
    } finally { setIsLoading(false); }
  };
  const getIceServers = (): Promise<ServerConfig[]> => invoke<ServerConfig[]>('get_ice_servers');
  return { setIceServers, getIceServers, isLoading };
};

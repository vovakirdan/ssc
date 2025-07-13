import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '@/hooks/use-toast';

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

  // Загружаем серверы из localStorage и синхронизируем с Rust при инициализации
  useEffect(() => {
    const syncServersWithRust = async () => {
      const savedSettings = localStorage.getItem('ssc-settings');
      if (savedSettings) {
        try {
          const settings = JSON.parse(savedSettings);
          if (settings.servers && Array.isArray(settings.servers)) {
            await setIceServers(settings.servers);
          }
        } catch (error) {
          console.error('Error syncing servers with Rust:', error);
        }
      }
    };

    syncServersWithRust();
  }, []);

  // Функция для установки серверов в Rust
  const setIceServers = async (servers: ServerConfig[]) => {
    setIsLoading(true);
    try {
      await invoke('set_ice_servers', { servers });
      console.log('ICE servers successfully set in Rust');
      return true;
    } catch (error) {
      console.error('Failed to set ICE servers:', error);
      toast({
        title: "Ошибка",
        description: "Не удалось установить ICE серверы",
        variant: "destructive"
      });
      return false;
    } finally {
      setIsLoading(false);
    }
  };

  // Функция для получения текущих серверов из Rust
  const getIceServers = async (): Promise<ServerConfig[]> => {
    try {
      const servers = await invoke<ServerConfig[]>('get_ice_servers');
      return servers;
    } catch (error) {
      console.error('Failed to get ICE servers:', error);
      return [];
    }
  };

  return {
    setIceServers,
    getIceServers,
    isLoading
  };
};

// Обновите Settings компонент
// В вашем Settings.tsx добавьте:

// import { useIceServers } from '@/hooks/useIceServers';

// const Settings = ({ onBack }: SettingsProps) => {
//   // ... existing code ...
  
//   const { setIceServers: syncIceServers } = useIceServers();

//   const handleSave = async () => {
//     // Проверяем, что есть хотя бы один сервер
//     if (!settings.servers || settings.servers.length === 0) {
//       toast({
//         title: "Ошибка",
//         description: "Необходимо добавить хотя бы один STUN или TURN сервер",
//         variant: "destructive"
//       });
//       return;
//     }

//     // Проверяем, что все серверы имеют корректные URL
//     const invalidServers = settings.servers.filter(server => !server.url || server.url.trim() === '');
//     if (invalidServers.length > 0) {
//       toast({
//         title: "Ошибка",
//         description: "Все серверы должны иметь корректный URL",
//         variant: "destructive"
//       });
//       return;
//     }

//     // Сохраняем настройки в localStorage
//     localStorage.setItem('ssc-settings', JSON.stringify(settings));
    
//     // Синхронизируем с Rust
//     const success = await syncIceServers(settings.servers);
    
//     if (success) {
//       console.log('Settings saved and synced with Rust:', settings);
//       toast({
//         title: "Успешно",
//         description: "Настройки сохранены и применены"
//       });
//       onBack();
//     }
//   };

//   // ... rest of component
// };
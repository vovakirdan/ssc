import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface ServerConfig {
  id: string;
  type: 'stun' | 'turn';
  url: string;
  username?: string;
  credential?: string;
  status?: 'validating' | 'valid' | 'invalid' | 'idle';
}

export interface SettingsData {
  servers: ServerConfig[];
  offerTTL: number;
}

export const useSettings = () => {
  const [settings, setSettings] = useState<SettingsData>({
    servers: [
      {
        id: '1',
        type: 'stun',
        url: 'stun:stun.l.google.com:19302'
      }
    ],
    offerTTL: 5
  });
  const [loading, setLoading] = useState(true);

  // Загружаем настройки при инициализации
  useEffect(() => {
    const loadSettings = async () => {
      try {
        // Инициализируем настройки в Rust
        await invoke('initialize_settings');
        
        // Загружаем из localStorage
        const savedSettings = localStorage.getItem('ssc-settings');
        if (savedSettings) {
          const parsedSettings = JSON.parse(savedSettings);
          const validatedSettings: SettingsData = {
            servers: Array.isArray(parsedSettings.servers) ? parsedSettings.servers : [
              {
                id: '1',
                type: 'stun',
                url: 'stun:stun.l.google.com:19302'
              }
            ],
            offerTTL: typeof parsedSettings.offerTTL === 'number' ? parsedSettings.offerTTL : 5
          };
          setSettings(validatedSettings);
        }
      } catch (error) {
        console.error('Error loading settings:', error);
      } finally {
        setLoading(false);
      }
    };

    loadSettings();
  }, []);

  const saveSettings = async (newSettings: SettingsData) => {
    try {
      // Сохраняем в Rust
      const success = await invoke('save_settings', {
        settings: {
          servers: newSettings.servers.map(server => ({
            id: server.id,
            type: server.type,
            url: server.url,
            username: server.username || null,
            credential: server.credential || null,
          })),
          offer_ttl: newSettings.offerTTL * 60, // Конвертируем минуты в секунды
        }
      }) as boolean;

      if (success) {
        // Также сохраняем в localStorage
        localStorage.setItem('ssc-settings', JSON.stringify(newSettings));
        setSettings(newSettings);
        return true;
      }
      return false;
    } catch (error) {
      console.error('Error saving settings:', error);
      return false;
    }
  };

  const validateServer = async (server: ServerConfig) => {
    try {
      const isValid = await invoke('validate_server', {
        server: {
          id: server.id,
          type: server.type,
          url: server.url,
          username: server.username || null,
          credential: server.credential || null,
        }
      }) as boolean;
      return isValid;
    } catch (error) {
      console.error('Error validating server:', error);
      return false;
    }
  };

  return {
    settings,
    loading,
    saveSettings,
    validateServer,
    setSettings
  };
};
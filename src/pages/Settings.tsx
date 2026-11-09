import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Slider } from '@/components/ui/slider';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ArrowLeft, Shield, Server, Clock, Zap, Eye, EyeOff, Plus, Trash2, Info, CheckCircle, XCircle, Loader, Glasses } from 'lucide-react';
import { useToast } from '@/hooks/use-toast';
import StarBorder from '@/components/StarBorder';
import ShinyText from '@/components/text/ShinyText';
import AnimatedContent from '@/components/AnimatedContent';
import FadeContent from '@/components/FadeContent';
import { invoke } from '@tauri-apps/api/core';
import { useIceServers } from '@/hooks/useIceServers';
import { savedNetworkMode, type AccessStatus, type NetworkMode } from '@/lib/preferences';

interface SettingsProps {
  onBack: () => void;
}

interface ServerConfig {
  id: string;
  type: 'stun' | 'turn';
  url: string;
  username?: string;
  credential?: string;
  status?: 'validating' | 'valid' | 'invalid' | 'idle';
}

interface SettingsData {
  networkMode: NetworkMode;
  servers: ServerConfig[];
  offerTTL: number;
}

const TTL_VALUES = [1, 2, 5, 10, 60]; // minutes

const Settings = ({ onBack }: SettingsProps) => {
  const [settings, setSettings] = useState<SettingsData>({
    networkMode: 'project',
    servers: [
      {
        id: '1',
        type: 'stun',
        url: 'stun:stun.l.google.com:19302'
      }
    ],
    offerTTL: 5
  });
  const { setIceServers: syncIceServers } = useIceServers();

  const [accessStatus, setAccessStatus] = useState<AccessStatus | null>(null);
  const [checkingAccess, setCheckingAccess] = useState(false);
  const accessReference = accessStatus?.state === 'active' ? accessStatus.access_reference : undefined;
  const copyAccessReference = async () => {
    if (!accessReference) return;
    try { await navigator.clipboard.writeText(accessReference); toast({ title: 'Номер доступа скопирован' }); }
    catch { toast({ title: 'Не удалось скопировать', description: 'Выделите номер доступа и скопируйте его вручную.', variant: 'destructive' }); }
  };
  const [resetAccessOpen, setResetAccessOpen] = useState(false);
  const resetLocalAccess = async () => {
    try { await invoke('reset_access'); setAccessStatus({ state: 'inactive' }); setResetAccessOpen(false); }
    catch { toast({ title: 'Ошибка', description: 'Не удалось сбросить локальную активацию', variant: 'destructive' }); }
  };
  const retryAccess = async () => {
    setCheckingAccess(true);
    try { setAccessStatus(await invoke<AccessStatus>('initialize_access')); }
    catch { setAccessStatus({ state: 'error' }); }
    finally { setCheckingAccess(false); }
  };
  useEffect(() => {
    let disposed = false;
    void invoke<AccessStatus>('get_access_status').then(status => { if (!disposed) setAccessStatus(status); }).catch(() => { if (!disposed) setAccessStatus({ state: 'error' }); });
    return () => { disposed = true; };
  }, []);

  const [showCredentials, setShowCredentials] = useState<{[key: string]: boolean}>({});
  const [easterEgg, setEasterEgg] = useState(false);
  const [clickCount, setClickCount] = useState(0);
  const { toast } = useToast();

  useEffect(() => {
    // Загружаем настройки из localStorage
    const savedSettings = localStorage.getItem('ssc-settings');
    if (savedSettings) {
      try {
        const parsedSettings = JSON.parse(savedSettings);
        // Ensure servers array exists and has valid structure
        const validatedSettings: SettingsData = {
          networkMode: savedNetworkMode(parsedSettings),
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
      } catch (error) {

        // Keep default settings if parsing fails
      }
    }
  }, []);

  const validateServer = async (server: ServerConfig) => {
    // Обновляем статус на "проверяется"
    updateServer(server.id, { status: 'validating' });

    try {
      // Для TURN серверов проверяем наличие учетных данных
      if (server.type === 'turn' && (!server.username || !server.credential)) {
        throw new Error('TURN server requires username and credential');
      }


      // Проверяем доступность сервера
      const isAvailable = await invoke('check_ice_server_availability', {
        config: {
          id: server.id,
          type: server.type,
          url: server.url,
          username: server.username,
          credential: server.credential
        }
      });

      if (!isAvailable) {

        throw new Error('Server is not accessible');
      }
      
      updateServer(server.id, { status: 'valid' });
      
      toast({
        title: "Сервер доступен",
        description: `${server.type.toUpperCase()} сервер успешно проверен`,
      });
    } catch (error) {
      updateServer(server.id, { status: 'invalid' });
      
      toast({
        title: "Ошибка проверки",
        description: `Не удалось подключиться к ${server.type.toUpperCase()} серверу`,
        variant: "destructive"
      });
    }
  };

  const getServerStatusIcon = (status?: string) => {
    switch (status) {
      case 'validating':
        return <Loader className="w-4 h-4 text-yellow-500 animate-spin" />;
      case 'valid':
        return <CheckCircle className="w-4 h-4 text-green-500" />;
      case 'invalid':
        return <XCircle className="w-4 h-4 text-red-500" />;
      default:
        return null;
    }
  };

  const handleSave = async () => {
    // Проверяем, что есть хотя бы один сервер
    if (settings.networkMode === 'custom' && (!settings.servers || settings.servers.length === 0)) {
      toast({
        title: "Ошибка",
        description: "Необходимо добавить хотя бы один STUN или TURN сервер",
        variant: "destructive"
      });
      return;
    }

    // Проверяем, что все серверы имеют корректные URL
    const invalidServers = settings.servers.filter(server => !server.url || server.url.trim() === '');
    if (settings.networkMode === 'custom' && invalidServers.length > 0) {
      toast({
        title: "Ошибка",
        description: "Все серверы должны иметь корректный URL",
        variant: "destructive"
      });
      return;
    }

    // Синхронизируем с Rust
    let success = false;
    if (settings.networkMode === 'custom') success = await syncIceServers(settings.servers);
    else {
      try { await invoke('set_network_mode', { mode: settings.networkMode }); success = true; }
      catch { toast({ title: 'Ошибка', description: 'Не удалось применить режим подключения', variant: 'destructive' }); }
    }
    
    if (success) {
      localStorage.setItem('ssc-settings', JSON.stringify(settings));

      toast({
        title: "Успешно",
        description: "Настройки сохранены и применены"
      });
      onBack();
    }
  };

  const handleReset = async () => {
    const defaultSettings: SettingsData = {
      networkMode: 'project',
      servers: [
        {
          id: '1',
          type: 'stun' as const,
          url: 'stun:stun.l.google.com:19302'
        }
      ],
      offerTTL: 5
    };
    try { await invoke('set_network_mode', { mode: 'project' }); }
    catch { toast({ title: 'Ошибка', description: 'Не удалось применить режим подключения', variant: 'destructive' }); return; }
    setSettings(defaultSettings);
    localStorage.setItem('ssc-settings', JSON.stringify(defaultSettings));
    toast({
      title: "Сброшено",
      description: "Настройки возвращены к значениям по умолчанию"
    });
  };

  const addServer = (type: 'stun' | 'turn') => {
    const newServer: ServerConfig = {
      id: Date.now().toString(),
      type,
      url: type === 'stun' ? 'stun:' : 'turn:',
      status: 'idle',
      ...(type === 'turn' && { username: '', credential: '' })
    };
    
    setSettings(prev => ({
      ...prev,
      servers: [...(prev.servers || []), newServer]
    }));
  };

  const removeServer = (id: string) => {
    setSettings(prev => ({
      ...prev,
      servers: (prev.servers || []).filter(server => server.id !== id)
    }));
  };

  const updateServer = (id: string, updates: Partial<ServerConfig>) => {
    setSettings(prev => ({
      ...prev,
      servers: (prev.servers || []).map(server => 
        server.id === id ? { ...server, ...updates } : server
      )
    }));
  };

  const toggleCredentialsVisibility = (id: string) => {
    setShowCredentials(prev => ({
      ...prev,
      [id]: !prev[id]
    }));
  };

  const handleTTLChange = (value: number[]) => {
    setSettings(prev => ({
      ...prev,
      offerTTL: TTL_VALUES[value[0]]
    }));
  };

  const getTTLSliderValue = () => {
    return TTL_VALUES.findIndex(val => val === settings.offerTTL);
  };

  const handleVersionClick = () => {
    setClickCount(prev => prev + 1);
    if (clickCount >= 6) {
      setEasterEgg(true);
      setTimeout(() => setEasterEgg(false), 3000);
      setClickCount(0);
    }
  };

  return (
    <TooltipProvider>
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4">
        <div className="max-w-2xl mx-auto space-y-6">
          <div className="flex items-center space-x-4">
            <Button
              variant="ghost"
              size="icon"
              onClick={onBack}
              className="text-slate-400 hover:text-white hover:bg-slate-800"
            >
              <ArrowLeft className="w-5 h-5" />
            </Button>
            <h1 className="text-2xl font-bold text-white">Настройки</h1>
          </div>

          {/* Серверы WebRTC */}
          <Card className="bg-slate-800/50 border-slate-700">
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center">
                  <Server className="w-5 h-5 mr-2 text-cyan-500" />
                  <CardTitle className="text-white">WebRTC Серверы</CardTitle>
                  
                  <Dialog>
                    <DialogTrigger asChild>
                      <Button variant="ghost" size="sm" className="ml-2 p-1 h-auto">
                        <Info className="w-4 h-4 text-slate-400 hover:text-white" />
                      </Button>
                    </DialogTrigger>
                    <DialogContent className="bg-slate-800 border-slate-700 text-white max-w-4xl max-h-[90vh] overflow-y-auto">
                      <DialogHeader>
                        <DialogTitle className="text-cyan-500 text-xl">STUN/TURN серверы</DialogTitle>
                        <DialogDescription className="text-slate-300 text-base leading-relaxed">
                          Подробная информация о серверах для P2P соединения
                        </DialogDescription>
                      </DialogHeader>
                      <div className="space-y-4 text-slate-200 leading-relaxed">
                        <div className="space-y-3">
                          <p>
                            <strong className="text-cyan-400">STUN</strong> (Session Traversal Utilities for NAT) - помогает определить ваш публичный IP-адрес и тип NAT для прямого P2P соединения. Отличный вариант в рамках одной сети.
                          </p>
                          <p>
                            <strong className="text-purple-400">TURN</strong> (Traversal Using Relays around NAT) - ретранслирует трафик через промежуточный сервер, когда прямое соединение невозможно.
                          </p>
                          <p>
                            Эти серверы необходимы для установки <strong className="text-emerald-400">безопасного</strong> P2P соединения между устройствами через интернет.
                          </p>
                        </div>

                        <div className="bg-slate-700/50 p-4 rounded-lg border border-slate-600">
                          <h4 className="font-semibold text-yellow-400 mb-2">Рекомендации по использованию</h4>
                          <p className="mb-2">
                            По умолчанию настроен STUN сервер. Если прямое соединение не устанавливается, добавьте TURN сервер выбранного вами провайдера.
                          </p>
                          <p>
                            Адрес и учётные данные можно получить у своего провайдера или настроить свои <strong className="text-purple-400">TURN</strong> сервера на одном из ресурсов:
                          </p>
                        </div>

                        <div className="bg-slate-700/30 p-4 rounded-lg">
                          <h4 className="font-semibold text-blue-400 mb-3">Рекомендуемые провайдеры TURN серверов</h4>
                          <ul className="space-y-2">
                            <li className="flex items-center">
                              <span className="w-2 h-2 bg-emerald-400 rounded-full mr-3"></span>
                              <a 
                                href="https://www.expressturn.com/" 
                                target="_blank" 
                                rel="noopener noreferrer"
                                className="text-blue-300 hover:text-blue-200 underline decoration-dotted"
                              >
                                expressturn.com
                              </a>
                              <span className="text-slate-400 ml-2">- 10 GB бесплатно</span>
                            </li>
                            <li className="flex items-center">
                              <span className="w-2 h-2 bg-emerald-400 rounded-full mr-3"></span>
                              <a 
                                href="https://www.metered.ca/" 
                                target="_blank" 
                                rel="noopener noreferrer"
                                className="text-blue-300 hover:text-blue-200 underline decoration-dotted"
                              >
                                metered.ca
                              </a>
                              <span className="text-slate-400 ml-2">- 20 GB бесплатно</span>
                            </li>
                          </ul>
                        </div>

                        <div className="bg-slate-700/30 p-4 rounded-lg">
                          <h4 className="font-semibold text-orange-400 mb-3">Информация о задержках</h4>
                          <p className="mb-3 text-slate-300">
                            Вы можете поделиться своими серверами с другими пользователями. При этом неважно, будут ли сервера одинаковые при подключении. 
                            Учитывайте, что чем дальше от вас сервер и чем больше узлов, тем больше задержка.
                          </p>
                          
                          <div className="space-y-2">
                            <h5 className="font-medium text-slate-200">Приблизительные задержки:</h5>
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-sm">
                              <div className="flex justify-between bg-green-900/20 px-3 py-2 rounded border-l-2 border-green-500">
                                <span>Прямое соединение (P2P)</span>
                                <span className="text-green-400 font-mono">20-50ms</span>
                              </div>
                              <div className="flex justify-between bg-blue-900/20 px-3 py-2 rounded border-l-2 border-blue-500">
                                <span>TURN (тот же регион)</span>
                                <span className="text-blue-400 font-mono">70-150ms</span>
                              </div>
                              <div className="flex justify-between bg-yellow-900/20 px-3 py-2 rounded border-l-2 border-yellow-500">
                                <span>TURN (другой континент)</span>
                                <span className="text-yellow-400 font-mono">200-400ms</span>
                              </div>
                              <div className="flex justify-between bg-orange-900/20 px-3 py-2 rounded border-l-2 border-orange-500">
                                <span>Два TURN (разные регионы)</span>
                                <span className="text-orange-400 font-mono">250-500ms</span>
                              </div>
                              <div className="flex justify-between bg-red-900/20 px-3 py-2 rounded border-l-2 border-red-500 sm:col-span-2">
                                <span>Два TURN (разные континенты)</span>
                                <span className="text-red-400 font-mono">400-800ms</span>
                              </div>
                            </div>
                          </div>
                        </div>

                        <div className="bg-slate-700/50 p-4 rounded-lg border border-slate-600">
                          <p className="text-sm text-slate-400 italic">
                            <Glasses className="w-4 h-4 text-yellow-400 inline-block mr-2" />
                            Совет: Для наилучшей производительности используйте серверы, расположенные ближе к вашему географическому местоположению.
                          </p>
                        </div>
                      </div>
                    </DialogContent>
                  </Dialog>
                </div>
              </div>
              
              <CardDescription className="text-slate-400">
                Настройка STUN/TURN серверов для P2P соединения
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="network-mode">Способ подключения</Label>
                <select id="network-mode" aria-label="Способ подключения" value={settings.networkMode}
                  onChange={event => setSettings(previous => ({ ...previous, networkMode: event.target.value as NetworkMode }))}
                  className="w-full rounded-md border border-slate-600 bg-slate-800 p-3 text-slate-100">
                  <option value="project">TURN проекта</option>
                  <option value="custom">Свои серверы</option>
                  <option value="disabled">Без TURN</option>
                </select>
                {settings.networkMode === 'project' && <div className="space-y-2 rounded-lg border border-slate-600 p-3 text-sm text-slate-300">
                  <p>TURN проекта настраивается автоматически. Его временные реквизиты остаются в приложении и не сохраняются в этих настройках.</p>
                  <p role="status">{accessStatus?.state === 'active' ? 'Установка активирована. Доступ проверяется при подключении.' : accessStatus?.state === 'error' ? (accessStatus.error?.message || 'Не удалось проверить активацию.') : 'Для первого запуска нужен персональный установщик с активацией.'}</p>
                  {accessReference && <div className="space-y-2">
                    <Label htmlFor="access-reference">Номер доступа</Label>
                    <Input id="access-reference" aria-label="Номер доступа" value={accessReference} readOnly className="font-mono text-xs" />
                    <p className="text-xs text-slate-400">Передайте этот номер владельцу сервиса, если потребуется отозвать доступ установки. Это не код сверки собеседника.</p>
                    <Button type="button" variant="secondary" onClick={() => { void copyAccessReference(); }}>Скопировать номер доступа</Button>
                  </div>}
                  <Button type="button" variant="secondary" onClick={() => { void retryAccess(); }} disabled={checkingAccess}>
                    {checkingAccess ? 'Проверяем…' : 'Повторить активацию'}
                  </Button>
                  <Dialog open={resetAccessOpen} onOpenChange={setResetAccessOpen}>
                    <DialogTrigger asChild><Button type="button" variant="ghost">Сбросить локальную активацию</Button></DialogTrigger>
                    <DialogContent>
                      <DialogHeader><DialogTitle>Удалить локальный допуск?</DialogTitle>
                        <DialogDescription>Ключ и данные активации на этом компьютере будут удалены. Для повторной активации понадобится новый персональный установщик. Другие копии ключа и допуск на сервере этим действием не отзываются. Свои TURN-серверы останутся в настройках.</DialogDescription>
                      </DialogHeader>
                      <Button type="button" variant="destructive" onClick={() => { void resetLocalAccess(); }}>Удалить локальный допуск</Button>
                    </DialogContent>
                  </Dialog>
                </div>}
                {settings.networkMode === 'disabled' && <p className="text-sm text-slate-300">Только прямое соединение: в некоторых сетях подключение будет недоступно.</p>}
                {settings.networkMode !== 'custom' && <p className="text-sm text-slate-400">Список своих серверов ниже сохранится и используется только в режиме «Свои серверы».</p>}
              </div>
              {/* Предупреждение если нет TURN серверов */}
              {settings.networkMode === 'custom' && (!settings.servers || settings.servers.filter(s => s.type === 'turn').length === 0) && (
                <div className="p-4 bg-orange-900/30 border border-orange-500/50 rounded-lg">
                  <div className="flex items-start space-x-3">
                    <div className="w-2 h-2 bg-orange-500 rounded-full mt-2 flex-shrink-0"></div>
                    <div>
                      <p className="text-orange-300 font-medium mb-1">Внимание!</p>
                      <p className="text-orange-200 text-sm">
                        Не добавлено ни одного TURN сервера. Связь из под разных сетей не гарантирована. 
                        Настоятельно рекомендуется добавить хотя бы один TURN сервер.
                      </p>
                    </div>
                  </div>
                </div>
              )}
              
              {(settings.servers || []).map((server) => (
                <AnimatedContent
                key={server.id}
                ease="bounce.out"
                >
                <div className="p-4 bg-slate-700/50 rounded-lg border border-slate-600">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                      <Label className="text-slate-300 font-semibold">
                        {server.type.toUpperCase()} Сервер
                      </Label>
                      {getServerStatusIcon(server.status)}
                    </div>
                    <div className="flex items-center space-x-2">
                      {server.url && server.url !== `${server.type}:` && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => validateServer(server)}
                          disabled={server.status === 'validating'}
                          className="text-blue-400 hover:text-blue-300 hover:bg-blue-900/20"
                        >
                          Проверить
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => removeServer(server.id)}
                        className="text-red-400 hover:text-red-300 hover:bg-red-900/20"
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>

                  <div className="space-y-3">
                    <div>
                      <Label className="text-slate-300 text-sm">URL</Label>
                      <Input
                        value={server.url}
                        onChange={(e) => updateServer(server.id, { url: e.target.value, status: 'idle' })}
                        placeholder={server.type === 'stun' ? 'stun:stun.l.google.com:19302' : 'turn:your-turn-server.com:3478'}
                        className="bg-slate-700 border-slate-600 text-white placeholder:text-slate-400"
                      />
                    </div>

                    {server.type === 'turn' && (
                      <>
                        <div className="flex items-center justify-between">
                          <Label className="text-slate-300 text-sm">Аутентификация</Label>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => toggleCredentialsVisibility(server.id)}
                            className="text-slate-400 hover:text-white"
                          >
                            {showCredentials[server.id] ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                          </Button>
                        </div>
                        
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                          <div>
                            <Label className="text-slate-300 text-xs">Username</Label>
                            <Input
                              type={showCredentials[server.id] ? "text" : "password"}
                              value={server.username || ''}
                              onChange={(e) => updateServer(server.id, { username: e.target.value, status: 'idle' })}
                              placeholder="username"
                              className="bg-slate-700 border-slate-600 text-white placeholder:text-slate-400"
                            />
                          </div>

                          <div>
                            <Label className="text-slate-300 text-xs">Credential</Label>
                            <Input
                              type={showCredentials[server.id] ? "text" : "password"}
                              value={server.credential || ''}
                              onChange={(e) => updateServer(server.id, { credential: e.target.value, status: 'idle' })}
                              placeholder="credential"
                              className="bg-slate-700 border-slate-600 text-white placeholder:text-slate-400"
                            />
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                </div>
                </AnimatedContent>
              ))}
            <FadeContent
                blur={true}
            >
              <div className="flex flex-col sm:flex-row gap-2">
                <StarBorder
                  onClick={() => addServer('stun')}
                  as="button"
                  color="cyan"
                  speed="5s"
                  className="flex-1 border-slate-600 text-slate-600 py-1"
                >
                  <Plus className="w-4 h-4 mr-2" />
                  Добавить STUN
                </StarBorder>
                
                <StarBorder
                  onClick={() => addServer('turn')}
                  as="button"
                  color="purple"
                  speed="5s"
                  className="flex-1 border-slate-600 text-slate-600 py-1"
                >
                  <Plus className="w-4 h-4 mr-2" />
                  Добавить TURN
                </StarBorder>
              </div>
            </FadeContent>
            </CardContent>
          </Card>

          {/* TTL настройки */}
          <Card className="bg-slate-800/50 border-slate-700">
            <CardHeader>
              <CardTitle className="text-white flex items-center">
                <Clock className="w-5 h-5 mr-2 text-emerald-500" />
                Время жизни оффера
              </CardTitle>
              <CardDescription className="text-slate-400">
                Как долго QR-код остается активным
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-4">
                <div className="flex justify-between text-sm text-slate-400">
                  <span>1 мин</span>
                  <span>2 мин</span>
                  <span>5 мин</span>
                  <span>10 мин</span>
                  <span>60 мин</span>
                </div>
                
                <Slider
                  value={[getTTLSliderValue()]}
                  onValueChange={handleTTLChange}
                  max={4}
                  min={0}
                  step={1}
                  className="w-full"
                />
                
                <p className="text-center text-slate-300 font-semibold">
                  Текущее время жизни: {settings.offerTTL} мин
                </p>
              </div>
            </CardContent>
          </Card>

          {/* Информация о приложении */}
          <Card className="bg-slate-800/50 border-slate-700">
            <CardHeader>
              <CardTitle className="text-white flex items-center">
                <Shield className="w-5 h-5 mr-2 text-purple-500" />
                О приложении
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex justify-between items-center">
                <span className="text-slate-300">Версия</span>
                <span 
                  className="text-slate-400 cursor-pointer hover:text-white transition-colors"
                  onClick={handleVersionClick}
                >
                  <ShinyText text="v1.0.0-phantom" />
                </span>
              </div>

              <div className="flex justify-between items-center">
                <span className="text-slate-300">Шифрование</span>
                <span className="text-emerald-500 flex items-center">
                  <Zap className="w-4 h-4 mr-1" />
                  End-to-End
                </span>
              </div>

              <div className="flex justify-between items-center">
                <span className="text-slate-300">Логирование</span>
                <span className="text-red-500">Отключено</span>
              </div>

              <div className="flex justify-between items-center">
                <span className="text-slate-300">Анонимность</span>
                <span className="text-cyan-500">100%</span>
              </div>

              {easterEgg && (
                <div className="p-4 bg-purple-900/30 border border-purple-500/50 rounded-lg animate-fade-in">
                  <p className="text-purple-300 text-sm text-center">
                    🕵️ "Лучший способ сохранить секрет - забыть о том, что он у вас есть"
                  </p>
                  <p className="text-purple-400 text-xs text-center mt-1">
                    - Неизвестный хакер
                  </p>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Кнопки действий */}
          <div className="flex flex-col sm:flex-row gap-4">
            <Button
              onClick={handleSave}
              className="flex-1 bg-gradient-to-r from-emerald-500 to-cyan-500 hover:from-emerald-600 hover:to-cyan-600 text-white font-semibold"
            >
              Сохранить настройки
            </Button>
            
            <Button
              onClick={handleReset}
              variant="outline"
              className="flex-1 border-slate-600 text-slate-600 hover:bg-slate-700 hover:text-white"
            >
              Сбросить
            </Button>
          </div>

          <p className="text-center text-slate-500 text-sm">
            Все настройки сохраняются локально и не передаются третьим лицам
          </p>
        </div>
      </div>
    </TooltipProvider>
  );
};

export default Settings;

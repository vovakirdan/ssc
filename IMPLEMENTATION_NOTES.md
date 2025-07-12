# Инструкции по реализации изменений STUN/TURN серверов

## Выполненные изменения

### 1. Backend (Rust/Tauri)

#### Добавлены новые типы в `src-tauri/src/webrtc_peer.rs`:
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub id: String,
    pub r#type: String, // 'stun' or 'turn'
    pub url: String,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsData {
    pub servers: Vec<ServerConfig>,
    pub offer_ttl: u64,
}
```

#### Добавлена глобальная переменная для хранения настроек:
```rust
static CURRENT_SETTINGS: Lazy<Mutex<Option<SettingsData>>> = Lazy::new(|| Mutex::new(None));
```

#### Обновлена функция `rtc_config()` для использования настроек:
- Теперь функция получает настройки из глобальной переменной `CURRENT_SETTINGS`
- Группирует STUN и TURN серверы для оптимизации
- Использует дефолтные настройки, если пользовательские не найдены

#### Добавлены новые функции:
- `initialize_settings()` - инициализация настроек при старте
- `get_settings()` - получение настроек
- `save_settings()` - сохранение настроек
- `validate_server()` - проверка доступности сервера

#### Обновлен `src-tauri/src/signaling.rs`:
- Добавлены команды для работы с настройками
- Добавлена команда `initialize_settings`

#### Обновлен `src-tauri/src/lib.rs`:
- Добавлены новые команды в `invoke_handler`

### 2. Frontend (React)

#### Обновлен `src/pages/Settings.tsx`:
- Добавлен импорт `invoke` из Tauri API (временно закомментирован)
- Обновлена функция `validateServer()` для использования Rust функции
- Обновлена функция `handleSave()` для сохранения в Rust
- Временно используется моковая проверка серверов

#### Обновлен `src/pages/GenerateQR.tsx`:
- TTL теперь загружается из настроек localStorage
- Добавлен useEffect для загрузки TTL при монтировании компонента
- Исправлены зависимости в useEffect

## Что нужно доработать

### 1. Исправить импорты Tauri API
Проблема: TypeScript не может найти модули `@tauri-apps/api/core` и `react`

Возможные решения:
1. Проверить установку зависимостей: `npm install`
2. Перезапустить TypeScript сервер в IDE
3. Проверить версии пакетов в `package.json`

### 2. Создать хук useSettings
Создать файл `src/hooks/use-settings.tsx` с хуком для работы с настройками:
```typescript
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export const useSettings = () => {
  // Логика работы с настройками
};
```

### 3. Интегрировать хук в компоненты
- Использовать `useSettings` в `Settings.tsx`
- Использовать `useSettings` в `GenerateQR.tsx`

### 4. Реализовать реальную проверку серверов
Заменить моковую проверку в `validateServer()` на реальные вызовы Rust функций.

### 5. Добавить обработку ошибок
- Добавить обработку ошибок при сохранении настроек
- Добавить обработку ошибок при проверке серверов
- Добавить уведомления пользователю о статусе операций

## Команды для тестирования

1. Сборка проекта:
```bash
npm run tauri build
```

2. Запуск в режиме разработки:
```bash
npm run tauri dev
```

3. Проверка TypeScript:
```bash
npx tsc --noEmit
```

## Структура настроек

Настройки сохраняются в формате:
```json
{
  "servers": [
    {
      "id": "1",
      "type": "stun",
      "url": "stun:stun.l.google.com:19302"
    },
    {
      "id": "2", 
      "type": "turn",
      "url": "turn:your-turn-server.com:3478",
      "username": "username",
      "credential": "password"
    }
  ],
  "offerTTL": 5
}
```

## Примечания

1. TTL сохраняется в минутах в localStorage, но передается в секундах в Rust
2. Серверы группируются по типу для оптимизации WebRTC конфигурации
3. При отсутствии настроек используются дефолтные значения
4. Проверка серверов создает временное WebRTC соединение для валидации
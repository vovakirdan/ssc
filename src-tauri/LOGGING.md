# Настройка логирования

## Обзор

Система логирования в приложении настроена таким образом, что в продакшене логирование полностью отключено и не может быть включено. В режиме разработки логирование можно настраивать.

## Конфигурация

### Основные настройки

- `LOGGING_ENABLED` - основная константа логирования
  - `true` в debug режиме
  - `false` в release режиме (неизменяемо)

### Настройки для разработки

В файле `src/config.rs` в модуле `dev`:

- `ENABLE_LOGGING` - полное включение/отключение логирования в режиме разработки
  - `true` - логирование включено (по умолчанию)
  - `false` - логирование отключено

## Как отключить логирование в режиме разработки

1. Откройте файл `src-tauri/src/config.rs`
2. Найдите строку:
   ```rust
   pub const ENABLE_LOGGING: bool = true;
   ```
3. Измените на:
   ```rust
   pub const ENABLE_LOGGING: bool = false;
   ```
4. Пересоберите приложение

## Безопасность

- В продакшене логирование полностью отключено на уровне компиляции
- Невозможно включить логирование в продакшене через переменные окружения или другие способы
- Все настройки логирования проверяются на этапе компиляции

## Типы логов

- `log()` - основные логи приложения
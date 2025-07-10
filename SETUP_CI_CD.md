# Настройка CI/CD Pipeline

## Быстрый старт

1. **Скопируйте файлы workflow** в ваш репозиторий:
   ```bash
   mkdir -p .github/workflows
   cp ci-cd.yml .github/workflows/
   ```

2. **Создайте dev ветку**:
   ```bash
   git checkout -b dev
   git push -u origin dev
   ```

3. **Настройте версию** в `package.json`:
   ```json
   {
     "version": "1.0.0"
   }
   ```

4. **Закоммитьте и запушьте**:
   ```bash
   git add .
   git commit -m "Add CI/CD pipeline"
   git push
   ```

## Подробная настройка

### 1. Структура веток

Рекомендуемая структура:
- `main` - стабильная версия, релизы
- `dev` - разработка, pre-release
- `feature/*` - отдельные фичи

### 2. Настройка GitHub Secrets (опционально)

Для подписи приложений добавьте секреты в Settings → Secrets and variables → Actions:

#### Windows
- `WINDOWS_CERTIFICATE` - путь к .pfx файлу
- `WINDOWS_CERTIFICATE_PASSWORD` - пароль от сертификата

#### macOS
- `MACOS_CERTIFICATE` - Developer ID Application сертификат
- `MACOS_CERTIFICATE_PASSWORD` - пароль от сертификата

#### Android
- `ANDROID_KEYSTORE` - keystore файл
- `ANDROID_KEYSTORE_PASSWORD` - пароль от keystore
- `ANDROID_KEY_ALIAS` - алиас ключа

### 3. Настройка Android (если нужно)

Для сборки Android приложений:

1. **Установите Android SDK**:
   ```bash
   # На локальной машине
   tauri android init
   ```

2. **Настройте переменные окружения**:
   ```bash
   export ANDROID_HOME=/path/to/android/sdk
   export ANDROID_SDK_ROOT=/path/to/android/sdk
   ```

3. **Проверьте конфигурацию** в `src-tauri/tauri.conf.json`:
   ```json
   {
     "bundle": {
       "android": {
         "minSdkVersion": 24,
         "versionCode": 1
       }
     }
   }
   ```

### 4. Настройка подписи (опционально)

#### Windows
1. Получите код-подписывающий сертификат
2. Экспортируйте в .pfx формат
3. Добавьте в GitHub Secrets

#### macOS
1. Получите Developer ID Application сертификат
2. Добавьте в GitHub Secrets
3. Настройте notarization (для App Store)

#### Android
1. Создайте keystore:
   ```bash
   keytool -genkey -v -keystore my-release-key.keystore -alias alias_name -keyalg RSA -keysize 2048 -validity 10000
   ```
2. Добавьте в GitHub Secrets

## Тестирование

### 1. Тест в dev ветке
```bash
git checkout dev
# Внесите изменения в код
git add .
git commit -m "Test CI/CD"
git push
```

Проверьте:
- [ ] Workflow запустился
- [ ] Cargo check прошел
- [ ] Сборка завершилась успешно
- [ ] Pre-release создан

### 2. Тест в main ветке
```bash
git checkout main
git merge dev
git push
```

Проверьте:
- [ ] Workflow запустился
- [ ] Все платформы собрались
- [ ] Релиз создан
- [ ] Артефакты прикреплены

## Мониторинг

### GitHub Actions
- Перейдите в Actions в вашем репозитории
- Отслеживайте статус сборок
- Просматривайте логи при ошибках

### Релизы
- Все релизы создаются автоматически
- Pre-release для dev ветки
- Стабильные релизы для main ветки

### Артефакты
- Доступны в разделе Actions
- Автоматически удаляются через 30 дней
- Можно скачать для тестирования

## Troubleshooting

### Частые проблемы

#### 1. Сборка не запускается
- Проверьте, что изменения в `src/` или `src-tauri/`
- Убедитесь, что ветка `main` или `dev`

#### 2. Cargo check падает
- Проверьте синтаксис Rust кода
- Убедитесь, что все зависимости установлены

#### 3. Android сборка падает
- Проверьте Android SDK
- Убедитесь, что `minSdkVersion` корректный

#### 4. Кеш не работает
- Проверьте, что `Cargo.lock` и `package-lock.json` в репозитории
- Убедитесь, что файлы не игнорируются в `.gitignore`

### Логи и отладка

#### Просмотр логов
1. Перейдите в Actions
2. Выберите workflow run
3. Нажмите на job
4. Просмотрите логи step'ов

#### Отладка кеша
```bash
# Принудительный сброс кеша
# Измените версию в package.json
```

#### Локальная отладка
```bash
# Проверьте сборку локально
npm run build
cd src-tauri
cargo build --release
```

## Оптимизация

### Ускорение сборки
1. **Используйте кеширование** (уже настроено)
2. **Разбивайте изменения** на мелкие коммиты
3. **Тестируйте в dev ветке** перед main

### Сокращение времени
1. **Параллельная сборка** для разных платформ
2. **Условная сборка** только при изменениях
3. **Кеширование зависимостей**

### Мониторинг производительности
- Отслеживайте время сборки
- Анализируйте узкие места
- Оптимизируйте зависимости

## Безопасность

### Секреты
- Все секреты хранятся в GitHub Secrets
- Не коммитьте секреты в код
- Регулярно ротируйте ключи

### Сборка
- Выполняется в изолированных контейнерах
- Нет доступа к секретам в PR
- Автоматическая очистка артефактов

### Подпись
- Используйте надежные сертификаты
- Храните ключи в безопасном месте
- Регулярно обновляйте сертификаты

## Поддержка

### Полезные ссылки
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Tauri Documentation](https://tauri.app/docs)
- [Rust Documentation](https://doc.rust-lang.org/)

### Сообщество
- [Tauri Discord](https://discord.gg/tauri)
- [GitHub Issues](https://github.com/tauri-apps/tauri/issues)

### Создание issue
При проблемах:
1. Опишите проблему подробно
2. Приложите логи сборки
3. Укажите версии зависимостей
4. Предоставьте минимальный пример
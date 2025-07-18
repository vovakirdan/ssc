# ZeroID: Абсолютно секретный чат

![Rust](https://img.shields.io/badge/-Rust-000000?style=flat-square&logo=rust)

**ZeroID**  — абсолютно секретный чат для безопасного общения без регистрации, авторизации и отслеживания. Ваше устройство и есть сервер, поэтому никто не сможет получить доступ к вашей переписке или истории сообщений.

## Оглавление 📚

- [Ключевые особенности](#ключевые-особенности-)
- [Поддерживаемые платформы](#поддерживаемые-платформы-)
- [Скриншоты](#скриншоты-)
- [Технический стек](#технический-стек-️) 
  - [Frontend](#frontend-)
  - [Backend (Core)](#backend-core-️)
- [Безопасность](#безопасность-)
  - [Что уже сделано для безопасности](#что-уже-сделано-для-безопасности-и-зачем-это-нужно)
  - [Что ещё предстоит сделать для безопасности](#что-ещё-нужно-сделать-для-безопасности)
- [Структура проекта](#структура-проекта-)
- [Сборка и запуск](#сборка-и-запуск-)
- [Безопасность](#безопасность-)
- [Будущие планы](#будущие-планы-)
- [Contributing](#contributing-)

## Ключевые особенности 🚀

- 🔒 **P2P-шифрование** на основе WebRTC и криптографии X25519, ChaCha20-Poly1305, HKDF.
- 🚫 **Отсутствие серверов**: приложение полностью децентрализовано и работает без внешних бэкендов.
- 🕵️‍♂️ **Полная анонимность**: никакой регистрации, никакой авторизации, никакой истории сообщений на сторонних серверах.
- ⚡ **Высокая скорость и надёжность**: соединение peer-to-peer с минимальной задержкой.

## Поддерживаемые платформы 💻

- 🖥️ Windows
- 🍎 macOS
- 🐧 Linux

Поддержка Android и iOS запланирована в будущем.

## Скриншоты 📸

Управление приложением одинаково для всех платформ.

### 🏠 Приветственный экран
![Приветственный экран](screenshots/win_welcome.png)

Добро пожаловать в ZeroID! Здесь вы можете начать безопасное общение. Выберите способ подключения:
- **Создать QR-код** — для инициации соединения
- **Сканировать QR-код** — для подключения к собеседнику

### Создать/принять оффер

На данном этапе вы можете создать оффер или принять оффер от собеседника.
![Создать/принять оффер](screenshots/win_choose_way.png)

### 🔗 Создание QR-кода
![Создание QR-кода](screenshots/android_generateQR.png)

Создайте уникальный QR-код для вашего собеседника. Этот код содержит зашифрованную информацию для установки защищённого соединения. Покажите его собеседнику для сканирования.
> Вы так же можете поделиться ссылкой созданного QR-кода для установки соединения.

### 📱 Сканирование QR-кода
![Сканирование QR-кода](screenshots/win_scanQR.png)

Отсканируйте QR-код или введите полученный оффер, созданный вашим собеседником. Приложение автоматически установит защищённое peer-to-peer соединение.

### Передача ответа
![Передача ответа](screenshots/win_generate_answer.png)

После сканирования QR-кода или ввода оффера, приложение автоматически сгенерирует ответное сообщение с зашифрованным ответом. Передайте его пригласившему вас собеседнику.


### 🔐 Проверка отпечатка
![Проверка отпечатка](screenshots/win_check_fingerprint.png)

После установки соединения сравните отпечатки безопасности (SAS) с собеседником. Если они совпадают — соединение защищено от атак "человек-посередине".

### 💬 Безопасный чат
![Основной чат](screenshots/win_chat.png)
![Android](screenshots/android_chat.png)

Теперь вы можете общаться в абсолютно безопасном режиме! Все сообщения шифруются end-to-end с использованием ChaCha20-Poly1305. Никаких серверов, никакой истории сообщений.

### 🔌 Отключение
![Отключение](screenshots/android_disconnect.png)

Завершите сессию одним кликом. Все ключи шифрования автоматически очищаются из памяти, обеспечивая полную безопасность.


ZeroID также доступен на мобильных устройствах с тем же уровнем безопасности и удобства использования.

## Технический стек 🛠️

### Frontend 🌐

- [React](https://react.dev/) (TypeScript)
- [Radix UI](https://www.radix-ui.com/) - UI-компоненты и хуки
- [Tailwind CSS](https://tailwindcss.com/) - стилизация
- [Vite](https://vitejs.dev/) - сборщик
- [React Router](https://reactrouter.com/) - маршрутизация
- [React Hook Form](https://react-hook-form.com/) - управление формами
- FSD-архитектура

### Backend (Core) ⚙️

- [Rust](https://www.rust-lang.org/) - системный язык программирования
- [WebRTC](https://webrtc.org/) - peer-to-peer соединения
- [Ring](https://github.com/briansmith/ring) - криптографические примитивы (X25519 ECDH)
- [ChaCha20-Poly1305](https://github.com/RustCrypto/AEADs) - аутентифицированное шифрование
- [HKDF](https://github.com/RustCrypto/KDFs) - генерация ключей
- [Tauri](https://tauri.app/) - фреймворк для создания десктоп-приложений
- [Tokio](https://tokio.rs/) - асинхронная среда выполнения
- [Serde](https://serde.rs/) - сериализация/десериализация

## Ключевые аспекты безопасности 🔒

### Что уже сделано для безопасности и зачем это нужно

1. **Сквозное шифрование – ChaCha20-Poly1305 (AEAD)**
   
   *Как реализовано:* каждое сообщение шифруется алгоритмом ChaCha20-Poly1305; для шифрования и расшифрования создаются два независимых объекта `ChaCha20Poly1305` (sealing/opening).
   
   *Преимущества:* одновременно обеспечиваются конфиденциальность, целостность и аутентификация сообщений; взломать трафик без знания ключа невозможно, а изменённые пакеты отбрасываются автоматически.

2. **Диффи-Хеллман X25519 + HKDF для разделения направлений**
   
   *Как реализовано:* стороны обмениваются 32-байтовыми публичными ключами X25519, получают общий секрет и пропускают его через HKDF-Sha256. Из 64-байтового вывода выделяются два независимых ключа: «мой → твой» и «твой → мой».
   
   *Преимущества:* даже при захвате одного направления (например, из-за утечки ключа отправителя) трафик в обратную сторону остаётся защищён; стойкость X25519 подтверждена криптоанализом.

3. **SAS-отпечаток (Short Authentication String)**
   
   *Как реализовано:* первые 48 бит одного из производных ключей хэшируются SHA-256 и выводятся в виде 12-символьного hex-кода. Пользователи могут визуально сравнить строки.
   
   *Преимущества:* простая ручная проверка исключает «человека-посередине» после установки канала; если SAS различается, сессию можно сразу разорвать.

4. **Нулевая «память» ключей (Zeroization)**
   
   *Как реализовано:* собственные ключи упакованы в структуру `ZeroizedKey` с деривацией `ZeroizeOnDrop`; временные буферы (`shared`, `okm`, k1/k2) обнуляются сразу после использования; счётчики и SAS стираются в `Drop` у `CryptoCtx`.
   
   *Преимущества:* при дампе памяти, свопе или краше шансы восстановить закрытые ключи сводятся к минимуму, что особенно важно на десктопах и мобильных устройствах.

5. **Одноразовый nonce на каждое сообщение**
   
   *Как реализовано:* к счётчику `send_n`/`recv_n` добавляется 64-битовый инкремент; он встраивается в 96-битовый nonce ChaCha20-Poly1305.
   
   *Преимущества:* отсутствие повторных nonce предотвращает катастрофическое «key + nonce reuse», при котором AEAD теряет стойкость.

6. **Базовая защита от повторов (replay)**
   
   *Как реализовано:* хранится `last_accepted_recv`; сообщения с тем же или меньшим номером игнорируются.
   
   *Преимущества:* злоумышленник не сможет бесконечно реплейтить последний пакет для DoS или социального обмана («Я не получал твоё сообщение»).

7. **Лимит на распаковку входных SDP-блоков**
   
   *Как реализовано:* при GZIP-распаковке используется `take(256 KiB)`, что ограничивает объём данных, которые можно извлечь из сжатого ввода.
   
   *Преимущества:* нейтрализует класс zip-bomb атак, когда 10-килобайтный архив разворачивается в гигабайты и кладёт процесс.

8. **Автоматическая очистка состояния при разрыве**
   
   *Как реализовано:* в `emit_disconnected` и `disconnect` обнуляются все глобальные `Mutex<Option<…>>` с ключами, криптоконтекстом и канальным состоянием.
   
   *Преимущества:* исключаем «живые» ключи в памяти после закрытия окна или перехода устройства в спящий режим.

9. **Передача DH-ключа только после открытия канала**
   
   *Как реализовано:* публичный X25519 отправляется первым полезным сообщением по уже установленному DataChannel; до этого никаких пользовательских данных нет.
   
   *Преимущества:* снижает окно для атак на скрипты сигнализации и не раскрывает ключи раньше времени.

10. **Минимизация сторонних зависимостей**
    
    *Как реализовано:* используются проверенные библиотеки `ring`, `chacha20poly1305` (RustCrypto) и `webRTC-rs`; весь чувствительный код локализован в одном модуле.
    
    *Преимущества:* меньше потенциальных поверхностей атаки и легче проводить аудит.

### Что ещё нужно сделать для безопасности

**Категория: управление nonce-ами и переполнение**
**Описание проблемы:** `send_n` и `recv_n` продолжают монотонно расти вплоть до `u64::MAX`, а при перезапуске сессии снова стартуют с 1. Переполнение или повторный запуск длинного чата создадут одинаковые nonce-ы.
**Риск:** повтор nonce в ChaCha20-Poly1305 делает возможным раскрытие и подмену сообщений.

---

**Категория: защита от повторов (replay)**
**Описание проблемы:** хранится только `last_accepted_recv`. Если злоумышленник пошлёт сообщение с seq +2, а затем повторит +1, второй пакет будет принят.
**Риск:** частичный replay, переупорядочивание сообщений, возможные DoS-сценарии.

---

**Категория: целостность сигнального канала и аутентификация DH**
**Описание проблемы:** SDP-блоки (OFFER/ANSWER) всё ещё передаются без подписи, а X25519-публичный ключ отсылается первым сообщением по неаутентифицированному каналу.
**Риск:** MITM может подменить ICE-параметры или провести классический «человек-посередине» до того, как пользователи сверят SAS.

---

**Категория: управление идентификаторами**
**Описание проблемы:** `random_id()` остаётся 64-битовым. Это уже «достаточно», но коллизия наступит при ≈ 4 млрд вызовов (день рождения).
**Риск:** при гипотетически огромном числе сессий возможно столкновение ID (помешает роутингу сигнализации).

---

**Категория: блокирующие Mutex в async-коде**
**Описание проблемы:** применяются `std::sync::Mutex`; внутри критических секций есть обращение к `emit` (I/O через Tauri).
**Риск:** в редких условиях UI может «подвиснуть», а при будущем расширении (например, логирование в файл) появится дедлок.


## Структура проекта 📂

```
src/
├── App.css
├── App.tsx
├── components/
│   ├── FingerprintModal.tsx
│   └── ui/ (компоненты интерфейса)
├── hooks/
│   ├── use-mobile.tsx
│   └── use-toast.ts
├── lib/
│   └── utils.ts
├── main.tsx
└── pages/
    ├── Chat.tsx
    ├── GenerateQR.tsx
    ├── Index.tsx
    ├── NotFound.tsx
    ├── ScanQR.tsx
    └── Welcome.tsx

Ядро (Core):
src/
├── lib.rs
├── main.rs
├── signaling.rs
└── webrtc_peer.rs
```

## Сборка и запуск 🚧

Для сборки приложения требуется установить [пререквизиты Tauri](https://tauri.app/start/prerequisites/).

**Команды сборки:**

```bash
make build             # для Linux/macOS/Windows
make build-ios         # iOS (скоро)
make build-android     # Android (скоро)
```

После сборки приложение запускается локально. Нет необходимости в дополнительных развёртываниях.

## Безопасность 🔑

ZeroID реализует следующие подходы для обеспечения конфиденциальности и безопасности:

- 🔐 **End-to-End Encryption** с использованием ChaCha20-Poly1305.
- 🔑 **Key Exchange** на основе алгоритма ECDH (X25519).
- 📱 **СAS (Short Authentication String)** для безопасного подтверждения связи.
- 📵 **Отсутствие хранения истории сообщений** — данные существуют только на устройствах пользователей.

## Будущие планы 📅

- 📢 Групповые чаты
- 📁 Передача файлов
- 🎙️ Голосовые и видеосообщения
- 📱 Поддержка мобильных платформ (Android и iOS)

## Contributing 🤝

ZeroID открыт к вкладу сообщества.

- Rust код должен соответствовать стандартам Rust.
- Для frontend используется подход Feature-Sliced Design (FSD).
- Перед отправкой PR убедитесь, что код чистый и снабжён комментариями.

---

ZeroID — это ваше право на приватность и безопасность в сети!


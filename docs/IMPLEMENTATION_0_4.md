# Выполнение SSC0.4.0

Основа: утверждённый план UI/UX, encrypted media и protocolv4. Baseline0.3.1 b8ca695. Мутации выполняются только в ssc и необходимых delivery-конфигах ssc-connect; пользовательские профили/registry/другие сервисы сохраняются.

## Зафиксированные параметры

- Фото/видео/файлы/голос,100MiB на вложение; маленькие фото/голос до5MiB автоматически, остальное после принятия.
- Нет постоянного кэша приложения; logical buffer200MiB, explicit save, не обещать физическую RAM/secure erase ОС.
- Обновление всех клиентов: v4 без downgrade, старые v3 требуют обновления.
- Зашифрованы payload и metadata/receipts/reactions/bye; media идут только по DataChannel, не через HTTP-хранилище.
- Один file stream в каждом направлении,16KiB части,256KiB окно неподтверждённых байтов; text/control обслуживаются между частями.
- Shared IDs32hex и SHA256 файла64hex. Hash потоковый в общем Rust, не100MiB arrayBuffer для digest.
- Themes Auto/Dark/Light; только theme preference сохраняется. Dust только декоративный: реальный plaintext очищается сразу.

## Ответственность и состояние

| Часть | Владелец | Состояние |
|---|---|---|
| Общий Rust/WASM v4, типы/валидация/hasher/векторы | managed_client | В работе |
| Native transport/IPC/generation/bye/capture | personal_installers | В работе |
| Общие UI/theme/help/voice | activation_ui | В работе |
| Общая media model, browser/native adapters, integration | root | В работе |
| Roadmap/10x docs, CSP, packaging, release/CI/CD | root | В работе |

## Очерёдность проверки

1. Core/native/model/voice/UI unit и boundary gates, строгий типовой контроль.
2. Реальный локальный helper+TURN+browser и native mixed роли; файлы/голос/реакции/выход, cancel/expiry/bounds.
3. Independent review безопасности и соответствия плану после возврата ownership от работников.
4. Public/private CI на точных SHA, настоящий VPS relay и реальные browser engines; отдельно помечать физические/Windows ограничения.
5. Только после gates: релиз0.4.0 native packages, затем новый static archive и actual CD/byte proof; сохранить immutable assets старых вкладок.

Секреты/URL приглашений/SAS/содержимое файлов не входят в diagnostics, screenshots, test artifacts и отчёты. Тестовые ресурсы помечаются и удаляются без затрагивания пользовательских данных.

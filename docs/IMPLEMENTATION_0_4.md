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
| Общий Rust/WASM v4, типы/валидация/hasher/векторы | managed_client | Готово: 39 tests, native/WASM lint, actual WASM vectors и 100MiB hash |
| Native transport/IPC/generation/bye/capture | personal_installers | Готово: 90 tests, lint, release build |
| Общие UI/theme/help/voice | activation_ui | Готово: shared UI, безопасный выход в шапке, таймер, темы, 97 npm tests |
| Общая media model, browser/native adapters, integration | root | Готово локально: Chromium/Firefox/WebKit, native mixed roles, восемь передач 1/100MiB |
| Roadmap/10x docs, CSP, packaging, release/CI/CD | root | Документы готовы; публикация и внешняя приёмка выполняются |

## Проверенные границы

- Поправка0.4.1: два настоящих browser↔browser прогона через VPS на0.4.0 остановились при передаче1MiB. Контрольная замена только ожидания исходящего буфера5→30s в тестовом JS позволила завершить передачу/скачать исходные1048576 байт с точным SHA256. Это отдельный эксперимент с кандидатом, не приёмка неизменённого релиза0.4.0. Ограничения TURN/сервера не менялись.
- В0.4.1 ожидание буфера и native SCTP send ограничены30s; web использует существующий AbortSignal для немедленной отмены при выходе. Старый5s порог воспроизводимо падает на регрессии медленного освобождения буфера. Пределы очередей, nonces и криптографическая проверка сохранены. Внешняя приёмка0.4.1 выполняется отдельно.

- Локальный настоящий TURN: native ↔ Chromium, обе роли и оба направления, четыре передачи по 1MiB и четыре по 100MiB. SHA256 и исходное Unicode-имя совпали; сохранение проверено в отдельном тестовом Downloads. Текст и реакции передавались во время большой передачи; authenticated Bye проверен в обе стороны.
- После переноса выхода и таймера повторены четыре передачи по 1MiB, подтверждение выхода и Chromium media suite. Для нового commit ещё выполняется внешняя матрица.
- Запись Chromium проверена с синтетическим микрофоном. Физические микрофоны, Windows capture protection и телефонная запись этим не подтверждены.
- Проверка 100MiB при ограниченной скорости требует отдельного стабильного стенда без Vite HMR; её не считаем пройденной. Положительная функциональная проверка выполнена на локальном TURN без ограничения скорости.

## Очерёдность проверки

1. Core/native/model/voice/UI unit и boundary gates, строгий типовой контроль.
2. Реальный локальный helper+TURN+browser и native mixed роли; файлы/голос/реакции/выход, cancel/expiry/bounds.
3. Independent review безопасности и соответствия плану после возврата ownership от работников.
4. Public/private CI на точных SHA, настоящий VPS relay и реальные browser engines; отдельно помечать физические/Windows ограничения.
5. Только после gates: релиз0.4.0 native packages, затем новый static archive и actual CD/byte proof; сохранить immutable assets старых вкладок.

Секреты/URL приглашений/SAS/содержимое файлов не входят в diagnostics, screenshots, test artifacts и отчёты. Тестовые ресурсы помечаются и удаляются без затрагивания пользовательских данных.

# Still 2.0 — отчёт локальной проверки

Дата: 1 октября 2026. Результат зафиксирован по просьбе пользователя;
дополнительные проверки остановлены. Локальная сборка и автоматический
build/test gate готовы. Полный runtime release gate не пройден.

## Архитектура и продукт

В нативной оболочке SwiftUI/AppKit доступны Spaces Drift, Fog и Ocean.
Space выбирает самостоятельную реализацию Scene; Preset выбирает seed и
параметры внутри этой Scene. У каждого пространства четыре встроенных пресета,
свои четыре пользовательских параметра и собственные custom presets.
`⌘1/2/3` выбирают пространства; `⌘N`, `⌘S`, `⌘F`, `⌘,` управляют вариацией,
сохранением пресета, fullscreen и показом панели. Панель скрывается через
2,5 секунды бездействия; ввод с клавиатуры также продлевает её видимость.

| Scene | GPU-модель | Параметры |
| --- | --- | --- |
| Drift | Compute частиц, curl/vector field, два history targets, instanced trails | Density, Speed, Flow, Light |
| Fog | Самостоятельный fullscreen проход: warped 3D fBm, интеграция плотности и рассеяния вдоль ограниченного луча | Density, Drift, Depth, Light |
| Ocean | Индексная perspective-сетка, направленные волны, vertex displacement, depth target, аналитические normals с фильтрацией по размеру пикселя, Fresnel/specular | Calm, Scale, Flow, Light |

Core хранит общие descriptors/parameters, timing, input, quality и state codecs.
GPU-контракт находится в `still-scenes`; сцены владеют buffers/textures,
bind groups, shaders, pipelines и simulation state. Общий renderer владеет
Metal device/surface и compositor. Контракт не требует CPU simulation,
compute pass, fullscreen shader или mesh. Scene возвращает линейный color
view; compositor выполняет exposure/tone mapping один раз.

Crossfade длится 0,65 секунды и удерживает максимум две полные Scene.
При повторном переключении предыдущая outgoing Scene освобождается до
создания следующей. По окончании перехода её ресурсы и compositor bindings
сбрасываются. Auto-quality хранит отдельную историю адаптации каждой Scene;
Eco/Balanced/High соответствуют разным техническим бюджетам каждой реализации.
Бюджеты и lifecycle описаны в [architecture.md](architecture.md).

State schema v3 сохраняет выбранную Scene и независимые параметры, seed,
выбранный Preset и custom presets каждой Scene. Миграция v0/v1/v2 сохраняет
Drift. Device quality/cadence/power и window/fullscreen preferences сохраняются
нативно. Повреждённый или будущий state сохраняется в резервной копии и не
блокирует запуск. Неподдерживаемая GPU, shader/pipeline errors и Rust panic
возвращают контролируемый статус; native UI показывает ошибку рендеринга.

Исходная GPU-реализация Drift, оба её WGSL shaders, параметры и пресеты
побайтно сохранены: `build/audit-2026-10-01/drift-preservation.json`.
Итоговый каталог всех 12 пресетов проверен через настоящие Metal pipelines:
[visual-review-2.0.md](visual-review-2.0.md).

## Автоматические проверки

`STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh` прошёл: formatting, Clippy с
`-D warnings`, 41 Rust test (15 core, 8 FFI, 3 renderer, 15 scenes), WGSL
Naga validation, настоящий Metal encode всех сцен/пресетов/quality tiers,
resize/zero size и повторное создание GPU outputs. Тесты также покрывают
registry и идентификаторы, validation параметров, schema migration,
per-scene persistence, Auto history, timing 60/120 Hz, FFI null/config/input
errors, быстрый выбор Scene и повторяемость seeds.

Xcode Release build, arm64, strip, dSYM и строгая проверка hardened-runtime
ad-hoc signature прошли. UUID: `AC803BCC-7AD8-3AD7-B460-ABD69D8F1888`.
Полный журнал: `build/audit-2026-10-01/final-gate.log`.
Исходники и бинарный файл закреплены в `final-source-manifest.json`;
SHA-256 исполняемого файла:
`75e47c2c7c6857017a575029e777b3c8652d92b1670a558397ece4a1982c545d`.

## Измерения и soak

Часовой прогон этой сборки завершился нормально: 3 634,226 секунды активного
display time, 217 524 кадра, 59,854 FPS по всему интервалу, exit status 0,
пустой журнал ошибок. Сцены переключались раз в минуту.

| Scene | CPU update+encode p95 | Lifetime FPS без crossfade | Metal memory, sampled steady range |
| --- | ---: | ---: | ---: |
| Drift | 0,818 ms | 59,976 | 77,703–78,547 MiB |
| Fog | 0,643 ms | 59,795 | 51,563–52,406 MiB |
| Ocean | 0,617 ms | 59,935 | 56,813–57,656 MiB |

Но `continuous_hour_pass=false`: кроме известного внешнего снимка памяти
(пауза 720 ms), в конце зарегистрирована отдельная пауза 3 614,67 ms,
причина которой не установлена. RSS после прогрева: 114,156–134,891 MiB;
медиана конечного окна выросла на 14,945 MiB относительно начального.
Metal first/last medians каждой Scene совпали. Неограниченный рост RAM не
опровергнут; leak-free статус не заявляется.

Финальные GPU p95 для этого SHA-256 не сняты. Повторная native lifecycle
проверка и leak scan остановлены до запуска по просьбе пользователя.
Предыдущий native lifecycle прогон дал 26/26 успешных проверок другой сборки;
он не подменяет повторную проверку текущего бинарного файла. Подробные
измерения и методы: [performance.md](performance.md). Бинарные файлы и сырые
логи сохранены для продолжения проверки без потери результата.

## Ограничения и локальная сборка

Тестовый Mac: MacBook Air M5, 16 GB, macOS 26.5.2, экран 60 Hz.
120 Hz поддержаны timing/display-driven архитектурой и unit tests; аппаратное
измерение 120 Hz отсутствует. Не проверены minimum macOS 14, внешний дисплей
с отключением, настоящий системный sleep/wake и ручная VoiceOver-навигация.
Synthetic NSWorkspace sleep/wake notifications проверяют notification path,
но не заменяют физический sleep. Computer Use не предоставил доступ к Still
для свежего ручного прохода. Метрики CPU/RSS/Metal не являются измерением
энергопотребления в ваттах.

Локальное приложение:
`build/DerivedData/Build/Products/Release/Still.app`.
Рядом находится matching dSYM. Для Developer ID signing и notarization нужны
сертификат Developer ID Application с private key и credentials notarytool;
затем требуются подпись, submit, staple и проверки codesign/spctl. Этих
credentials нет; публичная публикация не выполнялась и не входит в цель.

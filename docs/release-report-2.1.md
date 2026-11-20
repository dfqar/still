# Still 2.1 — Glass и Field

Дата: 4 октября 2026. Добавлены два самостоятельных процедурных пространства
в существующее нативное macOS-приложение. Локальная Release-сборка готова;
исторические ограничения полного release gate 2.0 остаются открытыми.

## Что добавлено

**Glass** рисует медленно меняющуюся трёхмерную стеклянную форму. Отдельный
проход создаёт процедурный фон, затем оптический shader находит вход и выход
луча, рассчитывает преломление, поглощение по толщине, отражение Fresnel и
небольшую цветовую дисперсию. Параметры: Form, Motion, Refraction, Light.

**Field** интегрирует непрерывные линии в процедурном векторном поле на GPU.
Compute-проход заново строит пути из фиксированных стартовых точек; отдельный
render-проход превращает сегменты в тонкие ленты со сглаженными краями.
Параметры: Density, Motion, Bend, Light. Между кадрами не хранится история
движения частиц; CPU не строит и не считывает геометрию.

| Space | Встроенные пресеты |
| --- | --- |
| Glass | Clear Form, Smoked Lens, Liquid Fold, Quiet Prism |
| Field | Soft Current, Contour, Long Form, Woven Silence |

У каждого пресета собственные seed и четыре значения параметров. Сцены
подключены к общему registry, нативным Spaces и Presets, custom presets,
плавным параметрам, вариациям, Auto-quality и crossfade длительностью 0,65 s.
`⌘4` выбирает Glass, `⌘5` — Field. Приложение теперь содержит пять пространств
и двадцать встроенных пресетов.

State schema остаётся v3, C ABI — v2. Новые записи создаются при первом
выборе соответствующего пространства; состояние 2.0 сохраняет Drift, Fog и
Ocean. GPU-код и shaders этих трёх пространств побайтно совпадают с манифестом
2.0. Перед работой сохранены исходники и прежнее приложение в
`build/glass-field-2026-10-04/`.

## Бюджеты GPU

| Space / tier | Внутренний target, MP cap | Работа |
| --- | ---: | --- |
| Glass Eco / Balanced / High | 0,55 / 0,95 / 1,50 | 28 / 44 / 64 шага поиска поверхности; 12 шагов уточнения выхода |
| Field Eco / Balanced / High | 0,75 / 1,40 / 2,20 | максимум 128 / 240 / 360 линий; 24 / 40 / 56 сегментов |

Glass владеет двумя color targets, Field — одним и фиксированным storage
buffer 328 320 bytes. Формат Rgba16Float используется при поддержке
rendering/filtering, иначе Rgba8Unorm. Resize и изменение качества пересоздают
targets; нулевой viewport освобождает их. Максимум две Scene живут во время
перехода. Детали: [architecture.md](architecture.md), ADR 018.

## Проверки

`STILL_REQUIRE_METAL_TESTS=1 scripts/test.sh` прошёл: formatting, Clippy с
`-D warnings`, **45 тестов** (15 core, 9 FFI, 3 renderer, 18 scenes), Naga
validation WGSL, настоящий Metal encode всех двадцати пресетов во всех трёх
quality tiers, resize/zero size и повторное создание outputs. Проверены
каталог C ABI, быстрые переключения, независимые параметры, seeds и custom
presets Glass/Field после export/import состояния.

Все восемь новых пресетов отрендерены реальными Metal pipelines с production
compositor и просмотрены: [visual-review-2.1.md](visual-review-2.1.md).
Каталог: `build/perf/catalog-glass-field/`.

Повторная нативная lifecycle-диагностика итоговой сборки завершилась с
**42/42 успешными проверками**: выбор всех пяти пространств, resize,
fullscreen/windowed, minimize/restore, synthetic sleep/wake notifications,
быстрые переключения и закрытие окна во время реального crossfade. На wake
диагностика возвращает приложение на передний план, чтобы проверить
возобновление кадров при активном окне. Журнал включает активность приложения,
видимость/occlusion окна и ошибки renderer.

В первом разработческом прогоне две проверки Field не подтвердили рост
счётчика кадров. Этот результат сохранён отдельно в
`build/perf/lifecycle-2-1/`; успешный повтор относится к итоговому бинарному
файлу и находится в `build/perf/lifecycle-2-1-final/`.

## Измерения итоговой сборки

Каждое новое пространство проверено отдельным 75-секундным прогоном в
Balanced, viewport 2360 × 1520, с 30-секундной Instruments Metal System Trace
после 15 секунд прогрева. Тестовый Mac: MacBook Air M5, 16 GB, macOS 26.5.2,
экран 60 Hz.

| Scene | Кадры / lifetime FPS | CPU update+encode p95 | GPU p95 | Metal memory в trace |
| --- | ---: | ---: | ---: | ---: |
| Glass | 4 490 / 59,991 | 1,064 ms | 7,766 ms | 59,828 MiB |
| Field | 4 477 / 59,928 | 1,090 ms | 0,659 ms | 56,500 MiB |

GPU trace содержит 1 827 кадров Glass и 1 822 кадра Field. Максимальный
интервал между представленными кадрами за весь прогон: 33,332 ms и 46,604 ms
соответственно; пауз больше 100 ms нет. Оба процесса завершились с кодом 0 и
пустым stderr. Все три итоговых runtime-прогона записали SHA-256 текущего
пакета. Сырые данные находятся в `build/perf/glass-2-1-final/`,
`build/perf/field-2-1-final/` и `build/perf/lifecycle-2-1-final/`.

RSS после 15 секунд прогрева: Glass 93,844–120,484 MiB, Field
107,641–122,078 MiB. Эти короткие окна включают подключение и отключение
Instruments; Metal memory внутри каждого trace постоянна. По ним не делается
вывод об отсутствии утечек или длительном поведении RAM. Cadence lifecycle
прогона включает намеренные остановки и не используется как FPS benchmark.

Короткие прогоны и synthetic sleep/wake не заменяют часовой soak и физический
системный sleep. Не повторялись проверка macOS 14, аппаратные 120 Hz, отключение
внешнего дисплея и ручная VoiceOver-навигация. Не заявляется исправление
необъяснённой паузы и роста RSS из часового прогона 2.0; его исходные данные
сохранены в [release-report-2.0.md](release-report-2.0.md).

## Локальный пакет

Release-приложение: `build/DerivedData/Build/Products/Release/Still.app`.
Версия 2.1, build 3, arm64, stripped, hardened-runtime ad-hoc signature;
строгая проверка `codesign --verify --deep --strict` прошла. Matching dSYM UUID:
`E21F7407-746D-39E9-BCD7-B4C2F1A22921`.

SHA-256 исполняемого файла:
`e5e87b39bbbdebdaf3e0a219b7ab65d1fc60500a482c7190368d364be6424c7b`.
Журналы: `build/glass-field-2026-10-04/gate.log`, `final-package.log`,
`artifact-audit.json`, `source-manifest.json`. Developer ID signing,
notarization и публичная публикация не выполнялись.

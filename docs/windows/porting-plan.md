# Порядок переноса и критерии готовности

## Подготовка Windows-машины

Начальная платформа: x64 Windows, MSVC toolchain, реальный DX12 adapter.
Минимальная поддерживаемая Windows для продукта выбирается в W0 по требованиям
зафиксированного Windows App SDK и испытанным драйверам; не обещать весь диапазон
Windows по одному факту компиляции.

- Rust 1.98.1 MSVC, rustfmt и Clippy из `rust-toolchain.toml`.
- Visual Studio / Build Tools с Desktop development with C++, MSVC x64 и Windows SDK.
- x64 PowerShell 7 для существующих build/ABI scripts.
- Для WinUI: .NET 10 LTS SDK и WinUI tooling, затем точные версии в global.json,
  project package references и NuGet lock file.
- Для измерений: GPU profiling/validation tooling, доступное выбранной системе.

Скопировать исходники с Cargo.lock. `.toolchains/` с Mac переносить не нужно:
это локальная установка, не общий runtime проекта. Также не переносить target,
DerivedData, .vs, bin/obj и build-кэши как часть исходников. Исходный архив
должен содержать macOS shell и Xcode project: они остаются regression baseline.
В текущей рабочей папке нет Git metadata; workflow начнёт выполняться после
размещения этих файлов в GitHub-репозитории, сам по себе он ещё не запускался.

Готовый source-only комплект можно пересобрать на машине с Python 3:

```sh
python3 scripts/package-source.py
```

Результат `build/Still-source-windows.zip` содержит исходники обеих оболочек,
docs, исходное Windows-предложение, Cargo.lock, build/CI tooling и внутренний
SOURCE-MANIFEST с SHA-256 каждого файла. Кэши и пользовательские Xcode settings
исключаются, CRC и manifest проверяются при упаковке. Это исходники, не DLL bundle.

Из корня проекта:

```powershell
rustup show
cargo xtask check-rust
./scripts/build-windows.ps1
```

Последняя команда повторяет Rust checks, собирает release DLL, генерирует header,
комплектует `build/windows/win-x64`, проверяет загрузку DLL и пишет SHA256SUMS.
Если Rust checks только что прошли на том же исходном состоянии, можно
`./scripts/build-windows.ps1 -SkipChecks`; DLL build и ABI smoke всё равно выполняются.

Дополнительные команды:

| Команда | Назначение |
| --- | --- |
| `cargo xtask headers` | Только актуальный C header |
| `cargo xtask rust` | Native release staticlib + cdylib и header |
| `cargo xtask windows` | Windows x64 MSVC engine bundle, без UI и ABI smoke |
| `cargo xtask check-rust` | fmt, Clippy -D warnings, workspace tests |
| `cargo xtask check` | Общие проверки и Rust build; на macOS также Xcode/package audit |
| `cargo xtask package` | Только macOS, требует Xcode; Windows-упаковку не имитирует |

Windows staging ожидает default native target/output; переменные
CARGO_BUILD_TARGET и CARGO_TARGET_DIR должны быть сняты. Не переопределять target
в локальной Cargo config для этого маршрута. Cross-check с Mac:

```sh
rustup target add x86_64-pc-windows-msvc
cargo check --locked --workspace --all-targets --target x86_64-pc-windows-msvc
```

Это проверка Rust-компиляции. Для настоящей DLL нужны Windows MSVC linker/SDK;
для Present нужен настоящий WinUI host и GPU.

## W0 — настоящая панель и DX12

Цель — снять главный технический риск до переноса всех окон и настроек.

1. Создать минимальный C#/WinUI solution в `platforms/windows`, .NET 10, x64,
   SwapChainPanel + простой XAML overlay. Зафиксировать SDK/AppSDK/min-Windows.
2. Получить ISwapChainPanelNative через правильный C#/WinRT interop; проверить
   IID, AddRef/Release и успешный/неуспешный attach.
3. Создать wgpu DX12 Surface до compatible adapter/device. Сделать отдельный
   Windows entry point, не переиспользовать аргумент NSView.
4. Вывести тестовый цвет/градиент через настоящее Present. Проверить SDR/sRGB,
   opaque/FIFO/latency2, format/view-format и отсутствие двойной gamma.
5. Сделать latest resize + DPI compensation; проверить 100/125/150/175/200%,
   два монитора, нулевой размер и возвращение к ненулевому.
6. Подключить Rendering callback и owner-thread teardown; проверить unsubscribe,
   повторный Loaded/Unloaded, быстрое закрытие во время resize.

**Выход W0:** воспроизводимый solution, locked packages, DLL/PDB/hash, native
logs с adapter/driver/scale, успешные реальный Present и teardown; документирован
выбранный DPI/COM integration path. На выходе обновить interop.md и BUILD-STATUS.
Наличие enum SwapChainPanel в wgpu само по себе W0 не закрывает.

## W1 — общий runtime и все сцены

Проверить на созданной панели по очереди Drift, Fog, Ocean, Glass, Field, Frost,
все 24 built-in presets и Eco/Balanced/High. Затем rapid switching/crossfade,
seed variations, zero-size, detach/recreate и Auto history на каждую сцену.

Выделить runtime из `still-ffi` в `still-runtime`, если для Windows lifecycle
потребуется второй orchestration path. Цель — один engine для обоих hosts;
не дублировать frame/state/quality код в C# или windows.rs. Провести отдельный
совместимый рефакторинг, без изменения ABI/state/сценовых WGSL.

**Выход W1:** одинаковые IDs, диапазоны/пресеты и состояние v3 на обеих платформах,
DX12 shader validation без ошибок, не больше двух активных сценовых наборов
ресурсов при переходе, успешный macOS gate.

## W2 — нативный интерфейс и хранение

Сгенерировать полные bindings, добавить владеющий serial controller и typed
buffers. Controls строить из scene/parameter catalogs; Frost получает native
color pickers и группировку light/material, диапазоны берутся из Rust.

Перенести выбор space/preset, variation, user presets, параметры, качество,
полноэкранный режим через AppWindow, скрытие overlay/cursor, keyboard shortcuts,
Narrator/focus/keyboard navigation и reduced-motion policy. Сглаживание ambient
не добавлять в UI. Сохранить русло общих input snapshots и code-only ambient API.

Native services: atomic content file, device/UI settings, corrupt/future backup,
local paths, совместная session logging directory. Проверить миграции старого
content и interchange macOS ↔ Windows без device-specific keys в content.

**Выход W2:** пользовательский функционал текущих шести сцен доступен нативно;
параметры и состояние authoritative в Rust, доступность и файлы принадлежат C#.

## W3 — lifecycle и восстановление

Реализовать явную машину состояний из architecture.md, unsubscribe Rendering при
остановке; minimize/restore, hide/show, deactivate/activate, screen off/on,
sleep/wake, display disconnect, cross-monitor, fullscreen, battery saver.
Повторить несколько циклов каждой операции, с каждой сценой.

Surface Lost/Outdated/Timeout отдельно от device loss. Для device loss создать
новые GPU objects и сцены из сохранённого content, не потерять user settings;
ограничить попытки, дать понятную ошибку при невозможности восстановления.
Учитывать late callbacks и закрытие во время recovery.

**Выход W3:** подтверждённые logs без зависаний/use-after-free, нет постоянных
кадров в остановленном состоянии, после resume не ускоряется симуляция.

## W4 — GPU, цвет и производительность

Проверить Intel integrated, AMD и NVIDIA хотя бы на доступных представителях;
в отчёте явно указать непроверенные семейства. Целевые 60/120 Hz, несколько
разрешений и DPI, windowed/fullscreen, power modes. WARP/software execution
может помогать CI, но не подтверждает производительность реального GPU.

Сравнить все сцены с macOS references с допуском к драйверному float/noise
различию; искать gamma, inverted Y, aspect ratio, blending/history ошибки.
Оценивать p95/p99 CPU/GPU, actual present cadence, максимальные frame gaps,
RSS и GPU memory. Отдельно записывать transition bursts. Не делать вывод
только по среднему FPS; не прятать stalls за capped simulation dt.

Провести длительный цикл всех сцен и повторных suspend/attach. Каждый отчёт
связывается с DLL/app hash, версией OS, GPU и драйвера. Прежние macOS soak и
lifecycle ограничения сохраняются в исторических отчётах, не закрываются портом.

**Выход W4:** численные критерии целевого железа зафиксированы и пройдены;
нет нарастающего числа GPU objects или накопления callbacks, длинные gaps
объяснены и оценены.

## W5 — приложение и доставка

Выбрать и проверить MSIX либо unpackaged delivery по требованиям продукта;
зафиксировать deployment Windows App SDK и .NET, runtime dependencies, DLL search
path, debug symbols и их соответствие app/DLL, подпись и обновления.
Проверить clean machine install/launch/uninstall, отсутствие developer paths
и исходных shaders в нежелательных diagnostic exports, хранение content и crash logs.

**Выход W5:** воспроизводимый installable Windows artifact с реальной оболочкой,
своим release report и закрытой acceptance matrix. Только здесь можно объявить
Windows-версию готовой. ARM64, HDR и выделенный render thread — отдельные этапы.

## Матрица минимальной приёмки

| Область | Что должно быть проверено на Windows |
| --- | --- |
| DLL / C ABI | Native loading, Cdecl, size_t, layouts, UTF-8, null/error/buffer contracts |
| Panel | Native IID, refcounts, UI-affinity, Present, reattach, dispose без retained panel |
| Scenarios | 6 сцен × 4 presets × 3 явных качества + Auto и переходы |
| Input / ambient | Y/velocity, exit/capture loss, atomic latest input, clear, fast attack/release, no UI smoothing |
| Resize / DPI | Ноль, маленький/большой viewport, fractional scale, monitor transfer, fullscreen |
| Lifecycle | Minimize/hide/deactivate/screen off/sleep и обратные переходы, repeat cycles |
| Recovery | Surface errors, device recreation, failure при initialization и при close |
| Color / cadence | SDR gradient, gamma, 60/120 Hz, no fixed-sleep simulation, gaps |
| Storage / UI | v3+миграции, corrupt/future backups, custom presets, accessibility |
| Delivery | Clean machine, locked deps, подпись/символы/хэши, runtime dependencies |

Windows CI, добавленный при подготовке, закрывает только Rust build/test и
DLL ABI smoke. GPU tests сейчас могут пропускать создание renderer на headless
host; это не считать зелёным DX12 gate. Переменная STILL_REQUIRE_METAL_TESTS
относится к существующему macOS release gate; обязательный Windows GPU runner
нужно добавить отдельно после W0.

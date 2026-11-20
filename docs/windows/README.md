# Still → Windows: точка входа

Подготовлено 22 октября 2026 для текущего **Still 2.2**, ABI **2**, состояния **v3**.
Основа — предоставленный Windows-план; он сохранён полностью в
[источнике](sources/windows-proposal.txt). Архитектурные решения ниже сверены с
текущим кодом и документацией поставщиков. При расхождении использовать эти
документы и действующий C-заголовок, а не примерный API из исходного предложения.

**Статус: репозиторий подготовлен к началу переноса. Работающего Windows-приложения
и проверенного подключения WinUI-панели пока нет.** Первый обязательный шаг на
Windows — W0, маленькая настоящая панель с DX12, COM-владением и корректным DPI.

## Что читать

1. [Архитектура](architecture.md): стек, владельцы, сцены, кадр, потоки, lifecycle,
   DPI, цвет, качество, хранение и логи.
2. [Interop и C ABI](interop.md): реальный API, P/Invoke, COM, размеры структур,
   порядок создания/уничтожения и обнаруженные ограничения wgpu 0.20.1.
3. [План переноса и проверки](porting-plan.md): подготовка машины, команды,
   этапы W0–W5, критерии готовности и известные риски.
4. [Сверка исходного предложения](source-map.md): все 32 раздела, принятые решения
   и необходимые поправки к историческому тексту.
5. [Отчёт подготовки](verification.md): что проверено здесь и что требует Windows.

Для общей части проекта: [основные ADR](../architecture.md),
[ambient API](../ambient-control-api.md), [Frost](../ambient-foundation.md),
[производительность и ограничения](../performance.md).

## Зафиксированный стек

| Уровень | Решение |
| --- | --- |
| Нативная оболочка | C#, .NET 10 LTS, WinUI 3, Windows App SDK |
| Область изображения | `SwapChainPanel` внутри XAML Grid; XAML-контролы поверх |
| Общий движок | Существующий Rust workspace, без переноса сцен в C# |
| Графика Windows | wgpu, только DX12; общие WGSL |
| Межъязыковая граница | C ABI + P/Invoke, opaque `StillEngine*` |
| Библиотека | **`still_ffi.dll`** — реальное имя артефакта crate `still-ffi` |
| Кадр первой версии | `CompositionTarget.Rendering`, один сериализованный UI-поток |
| Время | Общий `FrameClock`: 120 шагов/с, максимум 4 догоняющих шага |
| Начальная цель | `win-x64` / `x86_64-pc-windows-msvc`; ARM64 после первой версии |
| Цвет | SDR, непрозрачный вывод, sRGB, FIFO/VSync, 2 желаемых кадра latency |
| Версии общей части | Rust 1.98.1, wgpu 0.20.1, naga 0.20.0, Cargo.lock |

Точный .NET SDK, Windows App SDK/NuGet, минимальная Windows и способ упаковки
фиксируются на этапе W0. До этого здесь нет фиктивного WinUI solution с
непроверенными версиями. `csbindgen` — выбранное направление генерации P/Invoke
при создании оболочки; сейчас источником ABI служит cbindgen-заголовок.

## Что уже подготовлено в репозитории

- AppKit handles изолированы в `still-renderer/src/platform.rs` под macOS cfg;
  Windows выбирает DX12. Старый AppKit attach на Windows возвращает ошибку.
- `still-ffi` выпускает staticlib для macOS и cdylib для Windows.
- `cargo xtask headers` генерирует C-заголовок отдельно от сборки приложения;
  `check-rust` выполняет общий Rust-набор проверок без Xcode.
- `cargo xtask windows` на Windows x64 MSVC собирает и комплектует DLL/PDB,
  заголовок и файлы закрепления зависимостей.
- PowerShell-сборка проверяет загрузку настоящей DLL и базовый ABI без GPU,
  создаёт SHA-256; добавлен Windows CI workflow для этого же маршрута.
- Rust toolchain больше не устанавливает Apple target на каждой платформе;
  добавлены исключения для Visual Studio/.NET build outputs.
- Сцены, WGSL, параметры, пресеты, состояние и macOS UI сохранены.

## Что ещё предстоит

WinUI solution и полноценный P/Invoke wrapper; Windows surface API с явным
COM-владением; выбор адаптера, совместимого с уже созданной surface; DPI matrix;
подписка/отписка кадров и lifecycle; полное восстановление после device loss;
native settings и логи; DX12-проверки всех сцен; приложение и упаковка.

Runtime пока находится в `still-ffi`, вместе с orchestration. Это известный долг,
не переносить его в C#. Выделение `still-runtime` планируется после W0, с
сохранением существующего ABI и поведения macOS.

## Быстрый старт на Windows

На Windows x64: MSVC C++ build tools + Windows SDK, Rust MSVC, PowerShell 7;
для будущей оболочки — .NET 10 и WinUI tooling. Из корня проекта:

```powershell
rustup show
./scripts/build-windows.ps1
```

Получится `build/windows/win-x64/`. Это комплект движка для интеграции,
**не Still.exe**. Успешный CI без реальной панели не закрывает W0 и GPU acceptance.

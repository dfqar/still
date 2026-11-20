# Сверка исходного Windows-предложения

Источник — полный пользовательский текст в
[sources/windows-proposal.txt](sources/windows-proposal.txt). Он сохранён без
изменений для истории; это предложение, а не описание уже работающего порта.
Встроенные `:chatgpt-content-reference{…}` — старые placeholders, не действующие
ссылки. Ниже фиксируется принятое прочтение применительно к Still 2.2.

## Все 32 раздела

| № исходника | Тема | Решение и место в актуальных документах |
| --- | --- | --- |
| 1 | Общая архитектура | Принята: WinUI/C# → C ABI → Rust/wgpu DX12; [architecture](architecture.md) |
| 2 | Кто за что отвечает | Принята таблица ownership; анализ звука уточнён: в надстройке |
| 3 | SwapChainPanel | Принят; COM pointer/QI/AddRef/teardown и ограничения в [interop](interop.md) |
| 4 | DX12 на Windows | Принят как единственный выбираемый backend Windows; сцены backend-neutral |
| 5 | FFI | C ABI/PInvoke, opaque handle; реальные имена и layouts в interop, csbindgen при W0 |
| 6 | Создание приложения | Принят surface-before-compatible-adapter; текущий legacy create ещё иной |
| 7 | Render loop | Rendering на UI-потоке сначала; worker thread только после измерений |
| 8 | Остановка Rendering | Реальная отписка при suspend/close; ранний return недостаточен |
| 9 | Drift | Существующий GPU compute переносится; particle budgets не расширяются |
| 10 | Frame pipeline | Общий scene prepare/encode + compositor + present; не все сцены имеют compute |
| 11 | Pointer interaction | Native adapter → текущий normalized pointer; proposed pressure/phase пока не API |
| 12 | Pointer / force field | Сохранить существующее воздействие через shader field; новые режимы не обязательны |
| 13 | Vector field | Сохранить текущий WGSL/curl-noise; упрощённые формулы — объяснение, не замена алгоритма |
| 14 | Trails | Существующие ping-pong targets и exponential time decay Drift; не добавлять всем сценам |
| 15 | UI и renderer | XAML UI отдельно от wgpu сцен; native input/accessibility |
| 16 | Resize и DPI | Latest resize в физических pixels + проверенная composition matrix на W0 |
| 17 | Surface configuration | SDR/sRGB, opaque, FIFO, latency2, sample1; capabilities + gamma validation |
| 18 | 60 / 120 Гц | Cadence от callback; фактическую частоту измерять, не обещать по API |
| 19 | Fixed simulation | Уже 120 Hz / max4 ticks в общем FrameClock; sleep gap сбрасывается |
| 20 | Device/surface lost | SurfaceError по wgpu 0.20.1; отдельное device recreation; ложные event/enum исправлены |
| 21 | Данные C# / Rust | Rust authoritative; native shell владеет presentation, путями/файлами |
| 22 | Rust layout | Сохранить текущие crates; platform boundary выделен, runtime extraction — W1 |
| 23 | Windows project | `platforms/windows`, planned Rendering/Interop/ViewModels/Services; solution после W0 |
| 24 | FFI DLL | cdylib добавлен, фактическое имя still_ffi.dll; x64 сначала, ARM64 позже |
| 25 | Ошибки FFI | Сохранить действующие 0…6 статусы и error/event drain; не копировать примерный enum |
| 26 | Логи | Общий session directory; bounded diagnostics, без disk work/render callbacks в кадре |
| 27 | Settings | Native atomic I/O, Rust state codec v3; content отдельно от device/UI settings |
| 28 | Quality tiers | Существующие Eco/Balanced/High/Auto и per-scene budgets; пример 100k+ не принят |
| 29 | Lifecycle | Explicit states, unsubscribe/suspend/reset/resume, power/display events; W3 |
| 30 | Fullscreen | AppWindow/presenter в C#, Rust получает viewport |
| 31 | HDR | Отложен до проверенных SDR/DPI/cadence/multi-monitor; pin wgpu сохраняется |
| 32 | Итоговый стек | Принят с поправками к реальным именам, текущей версии и объёму реализации |

## Существенные поправки

- Текст местами говорит о начале Still 0.1 и будущем macOS-порте. Сейчас macOS
  уже существует, продукт 2.2, шесть сцен и 24 presets. Windows переносит этот
  общий engine, не начинает Drift заново.
- `still_engine.dll`, `still_render_frame` и примерные crate names не являются
  текущими артефактами/API. Реальные имена сохранены и описаны.
- ISwapChainPanelNative WinUI 3 подтверждён по IDL: **63aad0b8-…**; сырой
  IInspectable/managed object не подходит. Закреплённый d3d12 ComPtr::from_raw
  делает **AddRef**. Контракт borrowed vs owned нельзя угадывать по названию.
- wgpu 0.20.1 поддерживает target SwapChainPanel, но это не доказательство
  корректного DPI/recovery/lifetime всего host. Эти проверки — W0.
- Текущий adapter создаётся без compatible surface. Это нужно изменить для
  Windows-пути до production attach, не скрывать за лозунгом «wgpu общий».
- `SurfaceContentsLost` документирован у UWP CompositionTarget; его нельзя
  автоматически считать событием выбранной WinUI 3 SwapChainPanel;
  `Occluded` отсутствует в закреплённом SurfaceError. Окклюзия — native policy.
- HDR/wide-gamut возможности другой версии wgpu не переносим в pin 0.20.1.
  Unorm fallback требует корректного transfer function, не молчаливой замены sRGB.
- Pointer ABI сейчас x/y −1…+1 с Y вверх; richer InputEvent в предложении — будущее.
- Preset/content формат и миграции уже реализованы. Quality — device policy,
  не повод переписывать preset schema или вводить новые particle ceilings.
- «Audio processing/modulation в Rust» из общей таблицы уточняется последующим
  решением пользователя: анализ/группировка звуков принадлежит надстройке;
  текущая основа принимает широкие visual snapshots без audio channels.
- `csbindgen`, tracing pipeline, полное device recreation, WinUI solution и
  native packaging — следующие работы, а не уже включённые зависимости/функции.

## Первичные источники для перепроверки

- [Microsoft: .NET support policy](https://dotnet.microsoft.com/en-us/platform/support/policy)
  — выбор .NET 10 LTS; конкретный SDK фиксировать при создании solution.
- [Microsoft: WinUI SwapChainPanel](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.swapchainpanel)
  — interop, scale, UI restrictions, opaque/backdrop ограничения.
- [Microsoft: CompositionTarget.Rendering](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.media.compositiontarget.rendering)
  — cadence/UI callback и необходимость отписки для остановки rendering.
- [Microsoft: UWP CompositionTarget.SurfaceContentsLost](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.media.compositiontarget.surfacecontentslost)
  — исходный recovery callback нельзя автоматически считать WinUI 3 panel API.
- [Microsoft: WinUI native interop IDL](https://github.com/microsoft/microsoft-ui-xaml/blob/4bcbff6d6b94c43c2e723cb0f08b51e473fbd235/dxaml/xcp/dxaml/idl/com/microsoft.ui.xaml.media.dxinterop.idl)
  — IID и vtable, проверено 22.10.2026; выбранный App SDK header перепроверяется в W0.
- [Microsoft: C#/WinRT COM interop](https://github.com/microsoft/CsWinRT/blob/master/docs/interop.md)
  — IObjectReference и modern WinRT wrapping.
- [wgpu 0.20.1 source](https://github.com/gfx-rs/wgpu/tree/v0.20.1)
  и фактические Cargo.lock + registry source `wgpu-hal 0.21.1`, `d3d12 0.20.0`
  — использовать API закреплённой версии, не текущей latest документации.

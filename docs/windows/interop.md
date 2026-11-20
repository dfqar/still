# Windows interop и действующий ABI

Контракт действующего API: [`include/still.h`](../../include/still.h),
генерируется из [`still-ffi`](../../crates/still-ffi/src/lib.rs).
Примерные `still_create` / `still_render_frame` из исходного предложения не
существуют: не создавать второй параллельный API с такими именами.

## DLL и вызовы

`still-ffi` собирается как `staticlib` и `cdylib`. На Windows MSVC результат —
**`still_ffi.dll`**, PDB для того же build и import library; в комплект переносим
DLL/PDB/header. На macOS сохраняется `libstill_ffi.a` для Xcode. Библиотеку
не переименовывать в `still_engine.dll` без единого изменения packaging/bindings.

C# получает opaque `StillEngine*` как `nint`. C ABI — Cdecl, `uint32_t → uint`,
`int32_t/StillResult → int`, `size_t → nuint`, `float → float`, `double → double`.
`bool` в структурах нет: flags — uint32. StructLayout.Sequential, стандартное
выравнивание, без произвольного Pack=1. Входные строки UTF-8 с завершающим NUL;
buffer exports декодировать по возвращённой длине. Любой массив/буфер/строка
должен жить всё время синхронного вызова; Rust не хранит caller input pointers.

| Структура ABI 2 | Размер на x64, байт |
| --- | ---: |
| StillEngineConfig | 16 |
| StillPointerEvent | 32 |
| StillSceneDescriptor | 32 |
| StillParameterDescriptor | 40 |
| StillAmbientFrame | 48 |
| StillAmbientLightControl | 36 |
| StillAmbientControlFrame | 120 |
| StillAmbientSmoothing | 24 |
| StillModulationFrame | 32 |

Версионные структуры передаются с `abi_version = 2` и правильным `struct_size`.
Reserved fields заполнять нулями. Вложенный StillAmbientLightControl не имеет
собственного заголовка; это inline struct, не указатель на объект или managed array.
Quality values: Auto=0, Eco=1, Balanced=2, High=3.

| Группа | Существующие exports |
| --- | --- |
| Lifetime | `still_engine_create`, `still_engine_destroy` |
| Surface | `still_engine_surface_attach` (**AppKit-only**), `still_engine_surface_detach`, `still_engine_resize` |
| Cadence/lifecycle | `still_engine_frame_tick`, `still_engine_frame_tick_animated`, `still_engine_suspend`, `still_engine_resume`, `still_engine_display_refresh_set`, `still_engine_power_state_set` |
| Scenes/parameters | `still_scene_count`, `still_scene_descriptor`, `still_engine_scene_select`, scene/parameter catalog exports, parameter set/get |
| Presets/state | preset catalog/select, user preset save/delete, new variation, state import/export |
| Quality | `still_engine_quality_set` |
| Ambient | `still_engine_ambient_update`, `still_engine_ambient_controls_update`, `still_engine_ambient_smoothing_set` |
| Modulation | `still_engine_modulation_update` |
| Diagnostics | metrics export, `still_engine_error_drain`, `still_engine_event_drain` |

`still_parameter_count/descriptor` — legacy Drift-каталог четырёх параметров.
Для Windows UI использовать per-scene JSON catalogs, иначе Frost и остальные
сцены будут показаны неверно. У scene IDs (`drift`, `fog`, `ocean`, `glass`,
`field`, `frost`) и preset IDs разные namespaces; нельзя подменять ID локализованным именем.
В JSON-каталоге `supportsModulation` указывает на поддержку временной модуляции.
Для непрерывных кадров используйте `still_engine_frame_tick_animated`, для
одиночной отрисовки `still_engine_frame_tick`. Поведение палитры и время жизни
входа описаны в [документации API](../ambient-control-api.md#transient-scene-modulation).

Native wrapper сначала генерирует P/Invoke из актуального контракта
(`csbindgen` — направление из плана; ещё не подключён), затем добавляет
безопасный владеющий controller. COM-адаптер не выводится автоматически из
`void*`: его контракт пишется и проверяется явно.

## Ошибки и буферы

Существующие значения StillResult сохраняются:

| Значение | Имя |
| --- | --- |
| 0 | Ok |
| 1 | InvalidArgument |
| 2 | InvalidState |
| 3 | Renderer |
| 4 | BufferTooSmall |
| 5 | Panic |
| 6 | NotFound |

Отдельных DeviceLost/InvalidHandle кодов сейчас нет. Не менять значения по
списку примеров из исходного документа. Коды описывают класс ошибки; детали
получает native host. Wrapper заранее обнуляет выходной handle. При проверке
версии/размера config и GPU initialization failure текущий create тоже обнуляет
out_engine; null config/out pointers возвращают InvalidArgument до этой записи.
Диагностика инициализации сейчас идёт в stderr. Более богатая init-диагностика
может быть добавлена совместимым Windows API на W0.

Export: сначала buffer=null, capacity=0, out_length; при BufferTooSmall выделить
достаточно памяти и повторить на том же сериализованном owner. Не считать
двухпроходный вызов drain потерей сообщения: при BufferTooSmall сообщение
сохраняется. JSON exports возвращают длину UTF-8 **без NUL**, достаточно ровно
out_length байт. Drain возвращает длину **с завершающим NUL**, при чтении текста
его исключить; длина 0 означает отсутствие сообщения. Не использовать strlen
для произвольного JSON-буфера.

`catch_unwind` защищает C boundary от Rust unwinding panic. Он не делает
произвольный указатель безопасным и не лечит access violation, abort или
параллельное уничтожение engine. При Panic wrapper прекращает обычные кадры,
собирает diagnostics и переходит в контролируемый recovery/failed state.

## SwapChainPanel: что подтверждено

В wgpu **0.20.1 уже существует** `SurfaceTargetUnsafe::SwapChainPanel`.
Нельзя передать в него C# object, HWND или исходный IInspectable pointer.
Нужен native **ISwapChainPanelNative** интерфейс панели.

Проверено по [WinUI IDL](https://github.com/microsoft/microsoft-ui-xaml/blob/4bcbff6d6b94c43c2e723cb0f08b51e473fbd235/dxaml/xcp/dxaml/idl/com/microsoft.ui.xaml.media.dxinterop.idl):
IID **`63aad0b8-7c24-40ff-85a8-640d944cc325`**, IUnknown + SetSwapChain.
Он совпадает с объявлением в закреплённом `wgpu-hal 0.21.1/src/dx12/types.rs`.
Не путать его с `f92f19d2-…` из Windows.UI.Xaml/UWP interop.
При реализации сверить IID ещё раз с header **выбранного Windows App SDK**.

Правильный C# маршрут — получить native object через C#/WinRT и QueryInterface/
`IObjectReference.As` для этого IID. Не подменять его непроверенным
`Marshal.GetIUnknownForObject(panel)` из старых CLR примеров. См.
[руководство Microsoft C#/WinRT](https://github.com/microsoft/CsWinRT/blob/master/docs/interop.md).

Тонкая, но важная деталь владения в **наших закреплённых версиях**:
`wgpu-hal` оборачивает переданный интерфейс в `d3d12::ComPtr::from_raw`, а
**d3d12 0.20.0 внутри from_raw вызывает AddRef**. Здесь borrowed native interface
получает дополнительную ссылку для Surface; это не consume-reference контракт.
Нельзя по одному имени from_raw предполагать семантику других COM crates.
Источники для сверки: Cargo.lock, `wgpu-hal/src/dx12/mod.rs`, `d3d12/src/com.rs`;
публичный [код соответствующей wgpu линии](https://github.com/gfx-rs/wgpu/blob/v0.20.1/wgpu-hal/src/dx12/mod.rs).

Следовательно, adapter должен:

1. Удерживать собственную QI/IObjectReference-ссылку и managed panel живыми
   на время attach; передавать правильный interface pointer как borrowed.
2. Позволить wgpu удерживать свою AddRef-ссылку до уничтожения Surface.
3. При failure освобождать только принадлежащие adapter ссылки; Surface cleanup
   не должен сопровождаться вторым Release на той же принадлежащей wgpu ссылке.
4. Отвязывать swapchain на UI-потоке до освобождения панели; проверить, что
   reconfigure вызывает SetSwapChain именно на разрешённом потоке.

**В текущем Still этот Windows attach ещё не реализован.** Перечисленное —
проверенная спецификация для W0, а не инструкция вызвать старый AppKit export.
При обновлении wgpu или COM wrapper контракт владения проверяется заново.

## Каким должен быть Windows surface API

Добавить отдельно именованный Windows create/attach entry point с version/size
header. Минимальный payload: native panel interface, physical width/height,
composition scale X/Y. Имя и структура фиксируются после W0; не выдавать
пример за уже существующий export. Existing ABI 2 остаётся совместимым;
расширение может быть аддитивным.

Он должен создать Surface **до** выбора adapter или гарантировать полноценную
безопасную переинициализацию renderer и всех сцен. Требуются проверка capabilities,
политика sRGB, DPI compensation и откат при любом этапе ошибки. Повторный attach
не должен сохранить scene resources от старого device или скрытую panel reference.

Один compositor не может решить сам DPI всей XAML-панели. В существующем
DX12 backend wgpu-hal SetSwapChain выполняется, но composition-scale setter
для панели отсутствует. W0 должен доказать корректный физический backbuffer +
матрицу преобразования, возможно через узкий native shim или контролируемый
patch. Не обновлять сразу весь wgpu ради одной этой возможности без macOS gate.

Сам `still_engine_surface_attach(engine, ns_view, …)` остаётся macOS API и на
Windows предсказуемо возвращает Renderer error с UnsupportedSurface (при
валидном engine). Его аргумент не меняет смысл в зависимости от платформы.

## Потоки и безопасное закрытие C# controller

Первая версия: create, attach, resize/configure, frame, settings, diagnostics,
detach и destroy — один UI owner; никаких параллельных P/Invoke. Audio/media
producer пишет только latest snapshot в managed mailbox, не вызывает engine
из audio callback. В кадре сначала settings, затем snapshot и pointer.

Безопасное закрытие: пометить Closing → отписать Rendering → запретить producer
и queued calls → дождаться in-flight frame → state export → suspend →
SetSwapChain(null) на UI → surface_detach → destroy → release adapter references.
Идемпотентный Dispose обнуляет handle и допускает повторный вызов.

SafeHandle finalizer может выполниться на постороннем потоке: не возлагать на
него обычный WinUI/COM detach. Нужен явный Dispose на owner thread, finalizer
только согласно проверенному контракту аварийного освобождения. Все подписки
снимаются, включая статический CompositionTarget.Rendering.

## Проверки interop

`scripts/test-windows-abi.ps1` загружает **реальную DLL** через NativeLibrary и
вызывает C exports: descriptor/registry/ABI/размер, invalid-config и null-handle.
GPU не создаётся, поэтому этот тест подходит для hosted Windows CI.

Дополнительно в W0: validate COM IID, balanced refs на success/failure,
повторные attach/detach, отсутствие удержанной панели после close, UI thread
SetSwapChain, отсутствие use-after-free на быстром resize/close, scale matrix,
реальный Present и compatible adapter. Compile-only проверка этих свойств
не подтверждает.

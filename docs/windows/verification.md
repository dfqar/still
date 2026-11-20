# Отчёт подготовки — 22.10.2026

Это проверка подготовки исходников Still 2.2 к Windows-переносу. Она не является
release report Windows-приложения. Полный порядок дальнейшей работы —
[porting-plan.md](porting-plan.md).

## Изменения

- Native surface creation вынесен в `still-renderer/src/platform.rs`:
  macOS выбирает Metal, Windows — DX12; старый NSView attach не принимает
  Windows handles. Пустые surface capabilities теперь дают ошибку вместо
  индексации пустого списка.
- Сохранена staticlib, добавлена cdylib; ABI declarations/статусы/структуры
  прежние, в C header добавлено только описание macOS attach.
- Разделены headers / check-rust / macOS package / Windows engine bundle.
  Windows packaging не вызывает Xcode и не выдаётся за Still.exe.
- Добавлены Windows build/ABI PowerShell scripts, Windows engine CI и
  source-only ZIP packager с внутренним SHA-256 manifest.
- Снят обязательный Apple target из общего toolchain, добавлены Windows/.NET
  build ignores. Rust 1.98.1, Cargo.lock, wgpu/naga pins сохранены.
- Добавлен Windows handoff в `platforms/windows`; README и ADR дополнены.
  Оригинальный Windows-текст сохранён без изменения байтов; все 32 раздела
  сверены с текущими API и продуктом.

## Проверено на этой машине

Host: macOS arm64, реальный Metal device.

| Проверка | Результат |
| --- | --- |
| `cargo fmt --all -- --check` | Пройдена |
| Native Clippy, workspace/all-targets, warnings denied | Пройдена |
| Workspace tests с STILL_REQUIRE_METAL_TESTS=1 | **53 passed**: core 18, ffi 11, renderer 3, scenes 21 |
| Metal scenes/presets/quality/resize integration | Пройдена в существующем обязательном gate |
| Release staticlib + cdylib, header generation | Пройдена |
| Release Xcode build, strip, signature и artifact audit | Пройдены |
| `cargo check … --target x86_64-pc-windows-msvc` | Пройден для workspace/all-targets |
| Clippy для Windows MSVC target, warnings denied | Пройден для workspace/all-targets |
| Независимый `cargo xtask headers` | Пройден |
| Загрузка реальной macOS cdylib через C ABI | Пройдена: registry, layouts, invalid-config, null handle, engine lifecycle, ambient update/clear |
| C11 compile-time размеры восьми ABI structs | Пройдены через clang |
| ABI declarations до/после | Совпадают после удаления comments/whitespace |
| macOS UI + still-core + still-scenes/WGSL | **52 исходных файла побайтово сохранены** |
| Source ZIP | CRC и SHA-256 каждого файла проверяются packager; локальный комплект собран |

Результаты локально: `build/windows-prep-2026-10-22/`:
`macos-gate.log`, `windows-check.log`, `windows-clippy.log`, `header.log`,
`abi-smoke-macos.json`, `abi-layout.c`, `artifact-audit.json`,
`protected-before.json`, `source-before.tar.gz`.
Build logs и локальные toolchains не включены в переносимый source ZIP.

Идентичность пересобранного macOS executable:

```text
SHA-256 b27ebd0cfea350f36150af7df0c9614fbd8acb83b50a1f34d1e08f135715d75c
Binary / dSYM UUID 76C7A0BC-F58B-3CA7-A4D5-956FECCE69D8
```

Версия приложения по-прежнему 2.2/build 5; этот hash отличает сборку подготовки
от более ранних измерений build 5. Новых performance/soak/lifecycle измерений
для этой сборки не проводилось. Старые ограничения отчётов остаются открытыми.

## Требует настоящей Windows

На Mac проверены Rust target и C ABI macOS-библиотеки. **Не запускались**
MSVC linker/нативная Windows DLL, PowerShell build/ABI scripts, GitHub Windows CI,
C#/WinUI solution, SwapChainPanel Present и DX12 runtime. Workflow подготовлен
в исходниках; у текущей рабочей папки нет Git metadata/remote для его запуска.

На Windows предстоят: W0 COM/DPI/format/Present, Windows create-before-compatible-
adapter path, все сцены на DX12, lifecycle, device recreation, native I/O/UI,
производительность и installable packaging. Green hosted CI с пропущенным GPU
созданием не считается закрытым Windows GPU gate.

Нет автоматического обновления wgpu, новой физики/particle budgets, HDR,
ARM64, именованных аудиоканалов или UI-настройки ambient smoothing.

# Windows host

Будущая оболочка Still: **C# / .NET 10 LTS / WinUI 3 / Windows App SDK**,
`SwapChainPanel` → общий Rust/wgpu DX12. Начальная архитектура — x64.

Начать с [документации переноса](../../docs/windows/README.md), затем выполнить
[W0 — проверку interop](../../docs/windows/porting-plan.md#w0--настоящая-панель-и-dx12).
В этой папке пока нет WinUI solution: реальное создание панели, COM-владение,
DPI и Present должны быть проверены на Windows до переноса полного интерфейса.

Подготовка Rust DLL уже находится в workspace. В x64 PowerShell 7 на Windows:

```powershell
./scripts/build-windows.ps1
```

Результат — `build/windows/win-x64/still_ffi.dll`, PDB, C-заголовок, закреплённые
зависимости и SHA-256. Это комплект движка для интеграции, не готовый Still.exe.

Планируемая структура после W0:

```text
Still.Windows.sln
Still.Windows/
  App.xaml
  MainWindow.xaml             # Grid + SwapChainPanel + XAML overlay
  Rendering/                  # panel lifetime, cadence, DPI, lifecycle
  Interop/                    # generated P/Invoke + explicit COM adapter
  ViewModels/                 # descriptor-driven native controls
  Services/                   # state files, device settings, logs, power
```

Пути `macOS/` и `Still.xcodeproj` сохраняются: перенос не требует перемещения
проверенной macOS-оболочки. Версии .NET SDK и Windows App SDK, Windows min-version,
NuGet lock-файл и способ упаковки фиксируются в W0 на Windows, до дальнейшей разработки.

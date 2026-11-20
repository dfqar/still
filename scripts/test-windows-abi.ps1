#Requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$LibraryPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [IntPtr]::Size -ne 8) { throw 'This ABI smoke test requires x64 Windows.' }
$resolvedLibrary = (Resolve-Path -LiteralPath $LibraryPath).Path
# No engine or GPU is created: this checks the actual DLL loader, calling
# convention, native layout, registry, status values and null-handle handling.
if (-not ('Still.WindowsAbiSmoke' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Still {
    public static class WindowsAbiSmoke {
        [StructLayout(LayoutKind.Sequential)]
        struct Config { public uint Abi, Size, Quality, Reserved; }
        [StructLayout(LayoutKind.Sequential)]
        struct SceneDescriptor {
            public uint Abi, Size;
            public IntPtr Id, Name;
            public uint Schema, Parameters;
        }
        [StructLayout(LayoutKind.Sequential)]
        struct ModulationFrame {
            public uint Abi, Size;
            public double TimeSeconds;
            public float Motion, Turbulence, Light;
            public uint Reserved;
        }
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        delegate UIntPtr SceneCount();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        delegate int SceneAt(UIntPtr index, out SceneDescriptor descriptor);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        delegate int Create(ref Config config, out IntPtr engine);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        delegate int Tick(IntPtr engine, double timestamp);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        delegate int UpdateModulation(IntPtr engine, ref ModulationFrame frame);
        static T Export<T>(IntPtr library, string name) where T : Delegate {
            return Marshal.GetDelegateForFunctionPointer<T>(NativeLibrary.GetExport(library, name));
        }
        static void Require(bool condition, string message) {
            if (!condition) throw new InvalidOperationException(message);
        }
        public static void Run(string path) {
            IntPtr library = NativeLibrary.Load(path);
            try {
                Require(Marshal.SizeOf<Config>() == 16, "Config layout mismatch");
                Require(Marshal.SizeOf<SceneDescriptor>() == 32, "Descriptor layout mismatch");
                Require(Marshal.SizeOf<ModulationFrame>() == 32 &&
                    Marshal.OffsetOf<ModulationFrame>(nameof(ModulationFrame.TimeSeconds)).ToInt32() == 8 &&
                    Marshal.OffsetOf<ModulationFrame>(nameof(ModulationFrame.Reserved)).ToInt32() == 28,
                    "Modulation frame layout mismatch");
                var count = Export<SceneCount>(library, "still_scene_count");
                var sceneAt = Export<SceneAt>(library, "still_scene_descriptor");
                string[] ids = { "drift", "fog", "ocean", "glass", "field", "frost" };
                Require(count().ToUInt64() == (ulong)ids.Length, "Unexpected scene count");
                for (uint i = 0; i < ids.Length; ++i) {
                    SceneDescriptor descriptor;
                    Require(sceneAt(new UIntPtr(i), out descriptor) == 0, "Descriptor failed");
                    Require(descriptor.Abi == 2 && descriptor.Size == 32, "Descriptor ABI mismatch");
                    Require(Marshal.PtrToStringUTF8(descriptor.Id) == ids[i], "Registry mismatch");
                    Require(descriptor.Parameters >= 4, "Missing parameter catalog");
                }
                var create = Export<Create>(library, "still_engine_create");
                var invalid = new Config { Abi = 0, Size = 16 };
                IntPtr handle;
                Require(create(ref invalid, out handle) == 1 && handle == IntPtr.Zero,
                    "Invalid config must be rejected and clear the output handle");
                var tick = Export<Tick>(library, "still_engine_frame_tick");
                Require(tick(IntPtr.Zero, 0) == 1, "Null handle must be rejected");
                var animatedTick = Export<Tick>(library, "still_engine_frame_tick_animated");
                Require(animatedTick(IntPtr.Zero, 0) == 1, "Animated tick must reject null handles");
                var update = Export<UpdateModulation>(library, "still_engine_modulation_update");
                var modulation = new ModulationFrame { Abi = 2, Size = 32 };
                Require(update(IntPtr.Zero, ref modulation) == 1, "Modulation must reject null handles");
            } finally { NativeLibrary.Free(library); }
        }
    }
}
'@
}
[Still.WindowsAbiSmoke]::Run($resolvedLibrary)
Write-Host 'Windows DLL ABI smoke test passed; no GPU/presentation test was performed.'

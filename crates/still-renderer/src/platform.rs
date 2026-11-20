//! The only native presentation boundary. Keep native handles out of scene code.
use crate::RendererError;
use std::ffi::c_void;

pub(crate) fn backends() -> Result<wgpu::Backends, RendererError> {
    #[cfg(target_os = "macos")]
    return Ok(wgpu::Backends::METAL);

    #[cfg(target_os = "windows")]
    return Ok(wgpu::Backends::DX12);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err(RendererError::UnsupportedPlatform)
}

/// The native view and CAMetalLayer must outlive the returned surface.
#[cfg(target_os = "macos")]
pub(crate) unsafe fn appkit_surface(
    instance: &wgpu::Instance,
    view: *mut c_void,
) -> Result<wgpu::Surface<'static>, RendererError> {
    use raw_window_handle::{
        AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    let view = std::ptr::NonNull::new(view).ok_or(RendererError::Detached)?;
    Ok(unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            raw_window_handle: RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
        })?
    })
}

// Never reinterpret a Windows COM pointer as an NSView. The separately named,
// versioned Windows attach contract is specified in docs/windows/interop.md;
// it must pass the real WinUI/DX12 integration gate before being exported.
#[cfg(not(target_os = "macos"))]
pub(crate) unsafe fn appkit_surface(
    _instance: &wgpu::Instance,
    _view: *mut c_void,
) -> Result<wgpu::Surface<'static>, RendererError> {
    Err(RendererError::UnsupportedSurface)
}

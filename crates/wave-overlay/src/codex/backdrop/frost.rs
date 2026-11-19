//! Attach a host backdrop brush without allocating a capture session or frame buffers.
use std::cell::OnceCell;
use windows::{
    System::{DispatcherQueue, DispatcherQueueController},
    UI::Composition::{Compositor, Desktop::DesktopWindowTarget},
    Win32::{
        Foundation::HWND,
        System::WinRT::{
            Composition::ICompositorDesktopInterop, CreateDispatcherQueueController,
            DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions, RO_INIT_SINGLETHREADED,
            RoInitialize, RoUninitialize,
        },
    },
    core::{Interface, Result},
};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute};

thread_local! {
    static COMPOSITION: OnceCell<Result<Composition>> = const { OnceCell::new() };
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: The UI thread owns the matching successful RoInitialize call.
        unsafe { RoUninitialize() }
    }
}

struct Composition {
    compositor: Compositor,
    queue: Option<DispatcherQueueController>,
    _apartment: Apartment,
}

impl Composition {
    fn new() -> Result<Self> {
        // SAFETY: Creation is confined to the owning UI thread with its existing message pump.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
        let apartment = Apartment;
        let queue = if DispatcherQueue::GetForCurrentThread().is_ok() {
            None
        } else {
            // SAFETY: The queue is installed on this initialized UI thread.
            Some(unsafe {
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_NONE,
                })?
            })
        };
        Ok(Self {
            compositor: Compositor::new()?,
            queue,
            _apartment: apartment,
        })
    }
}

impl Drop for Composition {
    fn drop(&mut self) {
        let _ = self.compositor.Close();
        if let Some(queue) = &self.queue {
            let _ = queue.ShutdownQueueAsync();
        }
    }
}

pub(super) struct Frost(DesktopWindowTarget);

impl Frost {
    pub fn open(hwnd: windows_sys::Win32::Foundation::HWND) -> Result<Self> {
        let enable = 1i32;
        // SAFETY: hwnd is the app-owned backdrop window. DWM reads a single Win32 BOOL.
        unsafe {
            windows::core::HRESULT(DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH as u32,
                (&raw const enable).cast(),
                size_of::<i32>() as u32,
            ))
            .ok()?;
        }

        COMPOSITION.with(|composition| {
            let composition =
                composition.get_or_init(Composition::new).as_ref().map_err(Clone::clone)?;
            let compositor = &composition.compositor;
            let desktop: ICompositorDesktopInterop = compositor.cast()?;
            let visual = compositor.CreateSpriteVisual()?;
            let mut size = visual.RelativeSizeAdjustment()?;
            size.X = 1.0;
            size.Y = 1.0;
            visual.SetRelativeSizeAdjustment(size)?;
            visual.SetBrush(&compositor.CreateHostBackdropBrush()?)?;
            // Blend the system frost with the live view beneath the backdrop window.
            visual.SetOpacity(0.90)?;
            // SAFETY: The target belongs to the current UI thread and is detached before destroy.
            let target = unsafe { desktop.CreateDesktopWindowTarget(HWND(hwnd), false)? };
            target.SetRoot(&visual)?;
            Ok(Self(target))
        })
    }
}

impl Drop for Frost {
    fn drop(&mut self) {
        let _ = self.0.SetRoot(None);
    }
}

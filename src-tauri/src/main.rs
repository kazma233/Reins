#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_linux_wayland_compatibility_env() {
    let has_wayland_display = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let disable_dmabuf_already_set = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some();

    if !has_wayland_display {
        return;
    }

    // These are WebKitGTK compatibility workarounds, not backend selection.
    if !disable_dmabuf_already_set {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    if std::path::Path::new("/proc/driver/nvidia/version").exists()
        && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none()
    {
        unsafe {
            std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_wayland_compatibility_env() {}

fn main() {
    configure_linux_wayland_compatibility_env();
    reins_core::logger::init_logger();

    if let Err(error) = std::panic::catch_unwind(reins_core::run) {
        let message = if let Some(text) = error.downcast_ref::<&str>() {
            *text
        } else if let Some(text) = error.downcast_ref::<String>() {
            text.as_str()
        } else {
            "unknown panic"
        };

        reins_core::logger::log_error(format!("application panic: {message}"));
        eprintln!("application panic: {message}");
        std::panic::resume_unwind(error);
    }
}

use gpui::Modifiers;

pub const UI_FONT: &str = if cfg!(target_os = "macos") {
    ".AppleSystemUIFont"
} else {
    "Segoe UI"
};

pub const COMMAND_LABEL: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl+"
};
pub const QUIT_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-q"
} else {
    "ctrl-q"
};
pub const CLOSE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-w"
} else {
    "ctrl-w"
};
pub const MINIMIZE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-m"
} else {
    "ctrl-m"
};

pub fn command(modifiers: Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.platform
    } else {
        modifiers.control
    }
}

// Windows uses GPUI's WindowControlArea::Drag hitboxes. On macOS, GPUI 0.2.2
// needs a small AppKit bridge to drag client content below the native controls.
#[cfg(target_os = "macos")]
pub use mac::drag_window;

#[cfg(not(target_os = "macos"))]
pub fn drag_window(_: &mut gpui::Window, _: usize) {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // objc 0.2's message macros use the historical cargo-clippy cfg.
mod mac {
    use gpui::Window;
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    fn native_window(window: &Window) -> *mut Object {
        let handle = HasWindowHandle::window_handle(window).expect("Native window handle");
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            unreachable!("macOS GPUI windows use AppKit")
        };
        let view = handle.ns_view.as_ptr() as *mut Object;
        // GPUI owns both objects, and callers run on its foreground/UI thread.
        unsafe { msg_send![view, window] }
    }

    pub fn drag_window(window: &mut Window, clicks: usize) {
        if clicks == 2 {
            window.zoom_window();
            return;
        }
        let window = native_window(window);
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let event: *mut Object = msg_send![app, currentEvent];
            let _: () = msg_send![window, performWindowDragWithEvent: event];
        }
    }
}

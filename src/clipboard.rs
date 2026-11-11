/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. Android calls go through [`crate::android_bridge`] so
/// they resolve the class through the app's loader on Dioxus worker threads.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/clipboard")]
unsafe extern "Kotlin" {
    pub type ClipboardPlugin;
}

#[cfg(target_os = "android")]
const CLIPBOARD_CLASS: &str = "dev.dioxus.g3_native_plugins.clipboard.ClipboardPlugin";

#[cfg(target_os = "android")]
type ClipboardHandle = crate::android_bridge::AndroidPlugin;

#[cfg(target_os = "ios")]
type ClipboardHandle = ClipboardPlugin;

#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
#[allow(
    missing_docs,
    reason = "These native FFI declarations mirror the platform bridge rather than the Rust API."
)]
unsafe extern "Swift" {
    /// Native Apple clipboard and share-sheet bridge.
    pub type ClipboardPlugin;

    /// Copies text to the Apple platform clipboard.
    pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String) -> String;

    /// Opens the Apple platform share sheet.
    pub fn shareFromRust(this: &ClipboardPlugin, text: String) -> String;
}

#[cfg(any(target_os = "android", target_os = "ios"))]
/// Access to the system clipboard and native share sheet.
pub struct Clipboard {
    plugin: Option<ClipboardHandle>,
}

#[cfg(desktop)]
/// Access to the system clipboard, and on Windows its share sheet.
pub struct Clipboard {
    // Opened on first use rather than here: a server has no clipboard to
    // open. On Linux the copied text is served from this process for as long
    // as the handle lives, so it is kept, not reopened for each copy.
    system: Option<arboard::Clipboard>,
    #[cfg(windows)]
    share_sheet: windows_share::ShareSheet,
}

#[cfg(target_arch = "wasm32")]
/// Access to the browser clipboard and Web Share APIs.
pub struct Clipboard;

#[cfg(any(target_os = "android", target_os = "ios"))]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }

    fn get_plugin(&mut self) -> Result<&ClipboardHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = ClipboardHandle::new(CLIPBOARD_CLASS)?;

            #[cfg(target_os = "ios")]
            let created = ClipboardPlugin::new()
                .map_err(|error| format!("Failed to create ClipboardPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    /// Copies `text` to the system clipboard.
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;

        #[cfg(target_os = "android")]
        {
            plugin.call_unit_str("copyToClipboardFromRust", &text)?;
            return Ok(());
        }

        #[cfg(target_os = "ios")]
        {
            _ = copyToClipboardFromRust(plugin, text)?;
            return Ok(());
        }

        #[allow(
            unreachable_code,
            reason = "Platform-specific early returns share one cross-platform fallback."
        )]
        {
            Ok(())
        }
    }

    /// Opens the platform share sheet with `text`.
    pub fn share(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;

        #[cfg(target_os = "android")]
        {
            plugin.call_unit_str("shareFromRust", &text)?;
            return Ok(());
        }

        #[cfg(target_os = "ios")]
        {
            _ = shareFromRust(plugin, text)?;
            return Ok(());
        }

        #[allow(
            unreachable_code,
            reason = "Platform-specific early returns share one cross-platform fallback."
        )]
        {
            Ok(())
        }
    }

    /// Always true: both phones have a share sheet.
    pub fn has_share_sheet(&self) -> bool {
        true
    }
}

#[cfg(desktop)]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self {
            system: None,
            #[cfg(windows)]
            share_sheet: windows_share::ShareSheet::default(),
        }
    }

    /// Copies `text` to the system clipboard.
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        if self.system.is_none() {
            let opened = arboard::Clipboard::new()
                .map_err(|error| format!("The clipboard is not available: {error}"))?;
            self.system = Some(opened);
        }
        self.system
            .as_mut()
            .expect("opened above")
            .set_text(text)
            .map_err(|error| format!("Unable to write to the clipboard: {error}"))
    }

    /// Opens Windows' share sheet for the app's window. macOS and Linux have
    /// none here, so `text` is copied instead: see [`Self::has_share_sheet`].
    pub fn share(&mut self, text: String) -> Result<(), String> {
        #[cfg(windows)]
        return self.share_sheet.show(text);

        #[cfg(not(windows))]
        self.copy_to_clipboard(text)
    }

    /// Whether [`Self::share`] opens a share sheet rather than copying, so an
    /// app can say "copied" when it did not share.
    pub fn has_share_sheet(&self) -> bool {
        cfg!(windows)
    }
}

/// Windows' Share sheet, through the interop Win32 apps use to reach it.
///
/// The sheet asks for its data with an event once it opens, so the handler is
/// registered just before each share. The previous one is removed first, or a
/// share would answer with every earlier text as well.
#[cfg(windows)]
mod windows_share {
    use windows::{
        ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
        Foundation::TypedEventHandler,
        Win32::UI::{Input::KeyboardAndMouse::GetActiveWindow, Shell::IDataTransferManagerInterop},
        core::{HSTRING, Ref, factory},
    };

    #[derive(Default)]
    pub(super) struct ShareSheet {
        registered: Option<(DataTransferManager, i64)>,
        /// Set once an open sheet has taken the text: Windows asks for it only
        /// when the sheet is on screen, so this is how a test knows it was.
        pub(super) answered: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl ShareSheet {
        pub(super) fn show(&mut self, text: String) -> Result<(), String> {
            // Event handlers run on the UI thread, where the active window is
            // the one the click came from.
            // SAFETY: no arguments; it reads the calling thread's state.
            let window = unsafe { GetActiveWindow() };
            if window.is_invalid() {
                return Err("There is no window to share from".to_string());
            }
            let interop =
                factory::<DataTransferManager, IDataTransferManagerInterop>().map_err(describe)?;
            // SAFETY: `window` is a live top-level window of this thread.
            let manager: DataTransferManager =
                unsafe { interop.GetForWindow(window) }.map_err(describe)?;

            if let Some((previous, token)) = self.registered.take() {
                let _ = previous.RemoveDataRequested(token);
            }
            let title = HSTRING::from("Share");
            let body = HSTRING::from(text);
            let answered = std::sync::Arc::clone(&self.answered);
            let handler = TypedEventHandler::new(move |_, args: Ref<DataRequestedEventArgs>| {
                let data = args.ok()?.Request()?.Data()?;
                // Windows shows an error instead of the sheet for a
                // package without a title.
                data.Properties()?.SetTitle(&title)?;
                data.SetText(&body)?;
                answered.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });
            let token = manager.DataRequested(&handler).map_err(describe)?;
            self.registered = Some((manager, token));

            // SAFETY: as for `GetForWindow`.
            unsafe { interop.ShowShareUIForWindow(window) }.map_err(describe)
        }
    }

    fn describe(error: windows::core::Error) -> String {
        format!("Unable to open the share sheet: {}", error.message())
    }
}

#[cfg(target_arch = "wasm32")]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self
    }

    /// Copies `text` with the browser Clipboard API.
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        let window = web_sys::window().ok_or_else(|| "Window is not available".to_string())?;
        let navigator = window.navigator();
        let clipboard = js_sys::Reflect::get(&navigator, &"clipboard".into())
            .map_err(|_| "Clipboard API is not available".to_string())?;
        let write_text = js_sys::Reflect::get(&clipboard, &"writeText".into())
            .map_err(|_| "Clipboard writeText API is not available".to_string())?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| "Clipboard writeText API is not callable".to_string())?;
        write_text
            .call1(&clipboard, &text.into())
            .map_err(|_| "Unable to write to clipboard".to_string())?;
        Ok(())
    }

    /// Shares `text` with the Web Share API, falling back to copying it.
    pub fn share(&mut self, text: String) -> Result<(), String> {
        let window = web_sys::window().ok_or_else(|| "Window is not available".to_string())?;
        let navigator = window.navigator();
        let share = js_sys::Reflect::get(&navigator, &"share".into())
            .map_err(|_| "Web Share API is not available".to_string())?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| "Web Share API is not callable".to_string());
        let Ok(share) = share else {
            return self.copy_to_clipboard(text);
        };
        let share_data = js_sys::Object::new();
        js_sys::Reflect::set(&share_data, &"title".into(), &"Share".into())
            .map_err(|_| "Unable to prepare share title".to_string())?;
        js_sys::Reflect::set(&share_data, &"text".into(), &text.into())
            .map_err(|_| "Unable to prepare share text".to_string())?;
        share
            .call1(&navigator, &share_data)
            .map_err(|_| "Unable to open share sheet".to_string())?;
        Ok(())
    }

    /// Whether the browser has the Web Share API, without which
    /// [`Self::share`] copies instead. Desktop Firefox, for one, has none.
    pub fn has_share_sheet(&self) -> bool {
        web_sys::window()
            .and_then(|window| js_sys::Reflect::get(&window.navigator(), &"share".into()).ok())
            .is_some_and(|share| share.is_function())
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(test)]
#[cfg(desktop)]
mod tests {
    use super::Clipboard;

    /// Touches the real clipboard, so it is opt-in:
    /// `cargo test --features clipboard -- --ignored clipboard::tests`.
    #[test]
    #[ignore = "writes the system clipboard"]
    fn desktop_copy_reaches_the_system_clipboard() {
        let mut reader = arboard::Clipboard::new().expect("a desktop session has a clipboard");
        let before = reader.get_text().ok();

        let mut clipboard = Clipboard::new();
        let text = "g3-native-plugins clipboard test".to_string();
        clipboard.copy_to_clipboard(text.clone()).expect("copied");
        assert_eq!(reader.get_text().expect("readable"), text);

        if let Some(before) = before {
            let _ = reader.set_text(before);
        }
    }

    /// Opens Windows' share sheet over a real window and keeps it up for a few
    /// seconds, long enough to see that it holds the text.
    #[cfg(windows)]
    #[test]
    #[ignore = "opens the share sheet on screen"]
    fn windows_share_sheet_opens_from_the_active_window() {
        use std::time::{Duration, Instant};
        use windows::{
            Win32::{
                Foundation::{HWND, LPARAM, LRESULT, WPARAM},
                System::LibraryLoader::GetModuleHandleW,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG,
                    PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOW, SetForegroundWindow,
                    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
                },
            },
            core::w,
        };

        unsafe extern "system" fn procedure(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }

        // SAFETY: a window class and window owned by this test's thread, and
        // destroyed before it returns.
        unsafe {
            // The apartment a UI thread has: tao's OleInitialize gives the
            // app's main thread the same.
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
            let instance = GetModuleHandleW(None).expect("module handle");
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: w!("G3ShareSheetTest"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("G3ShareSheetTest"),
                w!("Share sheet test"),
                WS_OVERLAPPEDWINDOW,
                200,
                200,
                640,
                400,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .expect("window");
            let _ = ShowWindow(window, SW_SHOW);
            // The sheet opens only over the foreground window, and Windows
            // lets a process take the foreground only after its own input. In
            // an app the click is that input; here a tap of Alt stands in.
            {
                use windows::Win32::UI::Input::KeyboardAndMouse::{
                    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_MENU,
                };
                let key = |flags| INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_MENU,
                            dwFlags: flags,
                            ..Default::default()
                        },
                    },
                };
                let taps = [key(Default::default()), key(KEYEVENTF_KEYUP)];
                SendInput(&taps, size_of::<INPUT>() as i32);
            }
            let _ = SetForegroundWindow(window);
            assert_eq!(
                windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
                window,
                "the test window could not take the foreground; run it from a terminal on the desktop",
            );

            let mut clipboard = Clipboard::new();
            let shared = clipboard.share("A video\nhttps://youtu.be/dQw4w9WgXcQ".to_string());

            // The sheet asks for its data through the message loop.
            let until = Instant::now() + Duration::from_secs(8);
            let mut message = MSG::default();
            while Instant::now() < until {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            let _ = DestroyWindow(window);
            shared.expect("the share sheet opened");
            // Windows asks for the data only once the sheet is on screen.
            assert!(
                clipboard
                    .share_sheet
                    .answered
                    .load(std::sync::atomic::Ordering::SeqCst),
                "the sheet never asked for the text",
            );
        }
    }
}

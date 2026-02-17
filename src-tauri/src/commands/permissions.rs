use serde::Serialize;
use std::sync::mpsc;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Serialize)]
pub struct PermissionStatus {
    pub microphone: String,     // "granted" | "denied" | "not_determined"
    pub screen_recording: bool, // true if granted
}

/// Check the current status of macOS permissions.
#[tauri::command]
pub async fn check_permissions() -> Result<PermissionStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(check_permissions_macos())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(PermissionStatus {
            microphone: "granted".to_string(),
            screen_recording: true,
        })
    }
}

/// Request a permission. Triggers the native macOS dialog.
/// For microphone: shows "Allow Microphone Access" dialog.
/// For screen_recording: shows an alert directing to System Settings.
/// If already denied, opens System Settings instead (macOS won't re-show the dialog).
#[tauri::command]
pub async fn request_permission(permission_type: String) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        match permission_type.as_str() {
            "microphone" => {
                let current = check_microphone_via_objc();
                if current == "denied" {
                    // Already denied — macOS won't show dialog again, open Settings
                    open_settings_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
                    return Ok(false);
                }
                // Not determined — trigger native dialog
                let granted = request_microphone_native();
                Ok(granted)
            }
            "screen_recording" => {
                // CGRequestScreenCaptureAccess shows an alert or directs to Settings
                let granted = unsafe { CGRequestScreenCaptureAccess() };
                Ok(granted)
            }
            _ => Err(format!("Unknown permission type: {}", permission_type)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

// ====================================================================
// macOS native implementation
// ====================================================================

#[cfg(target_os = "macos")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    static _NSConcreteStackBlock: std::ffi::c_void;
    fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn objc_msgSend();
}

#[cfg(target_os = "macos")]
fn check_permissions_macos() -> PermissionStatus {
    let microphone = check_microphone_via_objc();
    let screen_recording = unsafe { CGPreflightScreenCaptureAccess() };
    PermissionStatus {
        microphone,
        screen_recording,
    }
}

/// Check mic status via ObjC runtime. Does NOT trigger any dialog.
#[cfg(target_os = "macos")]
fn check_microphone_via_objc() -> String {
    type MsgSendAuthFn = unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
    ) -> i64;

    type MsgSendStrFn = unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;

    unsafe {
        let cls = objc_getClass(b"AVCaptureDevice\0".as_ptr().cast());
        if cls.is_null() {
            return "not_determined".to_string();
        }

        let ns_string_cls = objc_getClass(b"NSString\0".as_ptr().cast());
        let str_sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr().cast());
        let new_str: MsgSendStrFn = std::mem::transmute(objc_msgSend as *const ());
        let media_type = new_str(ns_string_cls, str_sel, b"soun\0".as_ptr().cast());

        let auth_sel = sel_registerName(b"authorizationStatusForMediaType:\0".as_ptr().cast());
        let send_fn: MsgSendAuthFn = std::mem::transmute(objc_msgSend as *const ());
        let status = send_fn(cls, auth_sel, media_type);

        match status {
            3 => "granted".to_string(),
            2 | 1 => "denied".to_string(),
            _ => "not_determined".to_string(),
        }
    }
}

// --- Objective-C Block support for requestAccessForMediaType:completionHandler: ---

/// Global channel for receiving the result from the ObjC block callback.
#[cfg(target_os = "macos")]
static MIC_RESULT_TX: OnceLock<Mutex<Option<mpsc::Sender<bool>>>> = OnceLock::new();

#[cfg(target_os = "macos")]
const BLOCK_HAS_COPY_DISPOSE: i32 = 1 << 25;

#[repr(C)]
#[cfg(target_os = "macos")]
struct ObjcBlock {
    isa: *const std::ffi::c_void,
    flags: i32,
    reserved: i32,
    invoke: unsafe extern "C" fn(*mut ObjcBlock, bool),
    descriptor: *const ObjcBlockDescriptor,
}

#[repr(C)]
#[cfg(target_os = "macos")]
struct ObjcBlockDescriptor {
    reserved: u64,
    size: u64,
    copy_helper: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_void),
    dispose_helper: unsafe extern "C" fn(*mut std::ffi::c_void),
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn noop_copy(_: *mut std::ffi::c_void, _: *const std::ffi::c_void) {}
#[cfg(target_os = "macos")]
unsafe extern "C" fn noop_dispose(_: *mut std::ffi::c_void) {}

#[cfg(target_os = "macos")]
static BLOCK_DESCRIPTOR: ObjcBlockDescriptor = ObjcBlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<ObjcBlock>() as u64,
    copy_helper: noop_copy,
    dispose_helper: noop_dispose,
};

/// Called by the ObjC runtime when requestAccess completes.
#[cfg(target_os = "macos")]
unsafe extern "C" fn mic_block_invoke(_block: *mut ObjcBlock, granted: bool) {
    let lock = MIC_RESULT_TX.get_or_init(|| Mutex::new(None));
    if let Ok(mut opt) = lock.lock() {
        if let Some(tx) = opt.take() {
            let _ = tx.send(granted);
        }
    }
}

/// Trigger the native macOS "Allow Microphone Access" dialog.
/// Blocks until the user responds (up to 60 seconds).
#[cfg(target_os = "macos")]
fn request_microphone_native() -> bool {
    let (tx, rx) = mpsc::channel::<bool>();

    // Store the sender so the block callback can use it
    let lock = MIC_RESULT_TX.get_or_init(|| Mutex::new(None));
    *lock.lock().unwrap() = Some(tx);

    unsafe {
        let cls = objc_getClass(b"AVCaptureDevice\0".as_ptr().cast());
        if cls.is_null() {
            return false;
        }

        // Build NSString for AVMediaTypeAudio
        let ns_string_cls = objc_getClass(b"NSString\0".as_ptr().cast());
        let str_sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr().cast());
        type MsgSendStrFn = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
        let new_str: MsgSendStrFn = std::mem::transmute(objc_msgSend as *const ());
        let media_type = new_str(ns_string_cls, str_sel, b"soun\0".as_ptr().cast());

        // Create ObjC block on the stack
        let mut block = ObjcBlock {
            isa: &_NSConcreteStackBlock as *const _ as *const std::ffi::c_void,
            flags: BLOCK_HAS_COPY_DISPOSE,
            reserved: 0,
            invoke: mic_block_invoke,
            descriptor: &BLOCK_DESCRIPTOR,
        };

        // Call [AVCaptureDevice requestAccessForMediaType:@"soun" completionHandler:block]
        let request_sel =
            sel_registerName(b"requestAccessForMediaType:completionHandler:\0".as_ptr().cast());
        type MsgSendRequestFn = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut ObjcBlock,
        );
        let request_fn: MsgSendRequestFn = std::mem::transmute(objc_msgSend as *const ());
        request_fn(cls, request_sel, media_type, &mut block);
    }

    // Wait for the user to respond (up to 60 seconds)
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn open_settings_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).status();
}

//! Privilege-elevation helpers (Windows).
//!
//! - `is_elevated()` — does the current process have an elevated token?
//! - `relaunch_as_admin()` — re-spawn ourselves with UAC elevation. The caller
//!   exits the current (non-elevated) instance after a successful relaunch.
//!
//! Elevation is only needed for removing orphaned package folders under
//! WindowsApps (taking ownership). Watching and repairing run unelevated.

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let process = GetCurrentProcess();
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned_size: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned_size,
        );
        let _ = CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Spawn an elevated copy of this binary. `Ok(true)` = UAC accepted (caller
/// should exit), `Ok(false)` = user declined.
#[cfg(windows)]
pub fn relaunch_as_admin() -> Result<bool, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_NORMAL;

    fn to_wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let verb = to_wide(std::ffi::OsStr::new("runas"));
    let file = to_wide(exe.as_os_str());
    let params = to_wide(std::ffi::OsStr::new(""));

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_NORMAL,
        )
    };
    let code = result as isize;
    if code > 32 {
        Ok(true)
    } else if code == 5 {
        // SE_ERR_ACCESSDENIED — the user clicked "No" on the UAC prompt.
        Ok(false)
    } else {
        Err(format!("ShellExecuteW returned {code}"))
    }
}

#[cfg(not(windows))]
pub fn relaunch_as_admin() -> Result<bool, String> {
    Err("elevated relaunch is only implemented on Windows".into())
}

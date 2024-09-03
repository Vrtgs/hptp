use crate::cli::startup::exe_path;
use crate::cli::{Daemon, RunArgs};
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::MaybeUninit;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use sysinfo::{ProcessRefreshKind, RefreshKind};
use windows::core::Owned;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcessToken, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
};
use windows::Win32::UI::Shell::{ShellExecuteW, SE_ERR_ACCESSDENIED};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_BINARY};
use winreg::{RegKey, RegValue};

const STARTUP_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

const TASK_MANAGER_OVERRIDE: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

const TASK_MANAGER_OVERRIDE_ENABLED: [u8; 12] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const APP_NAME: &str = "xyz.vrtgs.hptp";

fn try_is_elevated() -> io::Result<bool> {
    let handle = {
        let mut handle = MaybeUninit::<HANDLE>::uninit();

        match unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, handle.as_mut_ptr()) } {
            // Safety: we just created and initialized this handle to the process token
            Ok(()) => Ok(unsafe { Owned::new(handle.assume_init()) }),
            Err(err) => Err(err),
        }
    };

    let handle = handle?;

    let token_info_res = {
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size: u32 = size_of::<TOKEN_ELEVATION>().try_into().unwrap();

        // Safety: handle is an owned handle just created by `OpenProcessToken`
        // and size matches `TOKEN_ELEVATION`s size
        let res = unsafe {
            GetTokenInformation(
                *handle,
                TokenElevation,
                Some(&mut elevation as *mut TOKEN_ELEVATION as *mut _),
                size,
                &mut size,
            )
        };

        res.map(|_| elevation)
    };

    Ok(token_info_res?.TokenIsElevated != 0)
}

fn ensure_admin() -> io::Result<()> {
    if !try_is_elevated()? {
        let cmd_wide = format!("\"{}\"", exe_path())
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<u16>>();

        let args_wide = {
            let cap = std::env::args_os().skip(1).map(|s| s.len() + 1).sum();
            let mut args_str = OsString::with_capacity(cap);
            let mut args = std::env::args_os().skip(1);
            if let Some(first_arg) = args.next() {
                args_str.push(first_arg);
                for arg in args {
                    args_str.push(" ");
                    args_str.push(arg);
                }
            }

            args_str.encode_wide().chain(Some(0)).collect::<Vec<u16>>()
        };

        let ret = unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
                .ok()
                .expect("Failed ot initialize COM");

            let ret = ShellExecuteW(
                HWND::default(),
                w!("runas"),
                PCWSTR::from_raw(cmd_wide.as_ptr()),
                PCWSTR::from_raw(args_wide.as_ptr()),
                None,
                SW_SHOWNORMAL,
            );

            CoUninitialize();

            ret
        };

        if ret.0 as usize <= 32 {
            let err = match ret.0 as u32 {
                SE_ERR_ACCESSDENIED => {
                    io::Error::other("failed to run as administrator, user rejected UAC prompt")
                }
                error_code => io::Error::other(format!(
                    "failed to run as administrator, error code {error_code}"
                )),
            };

            return Err(err);
        }

        std::process::exit(0);
    }
    Ok(())
}

pub fn setup_startup(daemon: Daemon, args: RunArgs) -> ! {
    let Daemon::Registry = daemon;

    ensure_admin().unwrap();

    let inner = move || -> io::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        hklm.open_subkey_with_flags(STARTUP_KEY, KEY_SET_VALUE)?
            .set_value(APP_NAME, &format!("{} run {args}", exe_path()))?;

        // this is an optional key
        if let Ok(reg) = hklm.open_subkey_with_flags(TASK_MANAGER_OVERRIDE, KEY_SET_VALUE) {
            reg.set_raw_value(
                APP_NAME,
                &RegValue {
                    vtype: REG_BINARY,
                    bytes: TASK_MANAGER_OVERRIDE_ENABLED.to_vec(),
                },
            )?;
        }

        struct CowOsStr<'a>(Cow<'a, str>);

        impl AsRef<OsStr> for CowOsStr<'_> {
            fn as_ref(&self) -> &OsStr {
                (*self.0).as_ref()
            }
        }

        Command::new(exe_path())
            .creation_flags((CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW).0)
            .arg("run")
            .args(args.args().map(CowOsStr))
            .spawn()?;

        Ok(())
    };

    inner().unwrap();

    std::process::exit(0)
}

pub fn remove_startup(daemon: Daemon) -> ! {
    let Daemon::Registry = daemon;

    ensure_admin().unwrap();

    // best effort cleanup
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let _ = hklm
        .open_subkey_with_flags(STARTUP_KEY, KEY_SET_VALUE)
        .and_then(|key| key.delete_value(APP_NAME));

    if let Ok(reg) = hklm.open_subkey_with_flags(TASK_MANAGER_OVERRIDE, KEY_SET_VALUE) {
        let _ = reg.delete_value(APP_NAME);
    }

    let sys_info = sysinfo::System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    let path = exe_path();
    let path = Path::new(&path);
    sys_info
        .processes()
        .values()
        .filter(|proc| proc.exe() == Some(path))
        .for_each(|proc| {
            let _ = proc.kill();
        });

    std::process::exit(0)
}

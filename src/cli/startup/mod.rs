use crate::cli::{Daemon, RunArgs};
use std::io;
use std::io::ErrorKind;
use std::path::Path;

// #[cfg_attr(windows, expect(unused_macros))]
#[cfg_attr(windows, allow(unused_macros))]
macro_rules! cmd {
    ($exe: literal $($arg: expr)*; option: propagate) => {
        (|| {
            use ::std::process::Stdio;
            ::std::process::Command::new($exe)
                .args([$($arg),*])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()?
                .success()
                .then_some(())
                .ok_or_else(|| ::std::io::Error::other(::std::format!("{} failed to exit successfully", [$exe $(, $arg)*].join(" "))))
        })()
    };
    ($exe: literal $($arg: expr)*; option: ignore) => {
        let _ = cmd!($exe $($arg)*; option: propagate);
    };
    ($exe: literal $($arg: expr)*) => {
        cmd!($exe $($arg)*; option: propagate).expect(concat!("failed to run ", $exe))
    };
}

fn exe_path() -> String {
    std::env::current_exe()
        .and_then(std::path::absolute)
        .expect("unable to get the current executables path")
        .into_os_string()
        .into_string()
        .expect("the current exes path contains invalid utf-8")
}

// #[cfg_attr(windows, expect(dead_code))]
#[cfg_attr(windows, allow(dead_code))]
fn rm_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    std::fs::remove_file(path).or_else(|e| match e.kind() {
        ErrorKind::NotFound => Ok(()),
        _ => Err(e),
    })
}

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "macos", path = "macos.rs")]
mod sys;

pub fn setup_startup(daemon: Daemon, args: RunArgs) -> ! {
    sys::setup_startup(daemon, args)
}

pub fn remove_startup(daemon: Daemon) -> ! {
    sys::remove_startup(daemon)
}

use crate::cli::startup::{exe_path, rm_file};
use crate::cli::{Daemon, RunArgs};
use std::fmt::{Display, Formatter};

const LAUNCHD_FILE: &str = "/Library/LaunchDaemons/xyz.vrtgs.hptp.plist";

pub fn setup_startup(daemon: Daemon, args: RunArgs) -> ! {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("must be root to set up startup");
        std::process::exit(1)
    }

    let Daemon::LaunchDaemons = daemon;

    struct LDArgs<I>(I);

    impl<I: Iterator<Item: Display> + Clone> Display for LDArgs<I> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            for arg in self.0.clone() {
                write!(f, "<string>{arg}</string>")?
            }

            Ok(())
        }
    }

    std::fs::write(
        LAUNCHD_FILE,
        format!(
            include_str!("./macos_launchd.plist"),
            program_path = exe_path(),
            args = LDArgs(std::iter::once("run").chain(args.args()))
        ),
    )
    .unwrap();

    cmd!("launchctl" "load" "-w" LAUNCHD_FILE);

    std::process::exit(0)
}

pub fn remove_startup(daemon: Daemon) -> ! {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("must be root to remove startup");
        std::process::exit(1)
    }

    let Daemon::LaunchDaemons = daemon;
    cmd!("launchctl" "unload" "-w" LAUNCHD_FILE);
    rm_file(LAUNCHD_FILE).unwrap();
    std::process::exit(0)
}

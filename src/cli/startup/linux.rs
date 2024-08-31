use crate::cli::startup::{exe_path, rm_file};
use crate::cli::{Daemon, RunArgs};
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Stdio;

pub fn setup_startup(daemon: Daemon, args: RunArgs) -> ! {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("must be root to set up startup");
        std::process::exit(1)
    }

    match daemon {
        Daemon::SystemD => systemd_setup_startup(args),
        Daemon::OpenRC => openrc_setup_startup(args),
    }

    std::process::exit(0)
}

pub fn remove_startup(daemon: Daemon) -> ! {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("must be root to remove startup");
        std::process::exit(1)
    }

    match daemon {
        Daemon::SystemD => systemd_remove_startup(),
        Daemon::OpenRC => openrc_remove_startup(),
    }

    std::process::exit(0)
}

pub fn write_file(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);

    opts.mode(mode);

    let mut file = opts.open(path)?;
    file.write_all(data)?;

    // Ensure that the data/metadata is synced and catch errors before dropping
    file.sync_all()
}

const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

const SYSTEMD_PATH: &str = "/etc/systemd/system/xyz.vrtgs.hptp.service";

const APP_NAME: &str = "xyz.vrtgs.hptp";
const OPEN_RC_PATH: &str = "/etc/init.d/xyz.vrtgs.hptp";

fn systemd_service_name() -> &'static str {
    let Some((_, name)) = SYSTEMD_PATH.rsplit_once('/') else {
        unreachable!("{SYSTEMD_PATH} should always contain a /")
    };
    name
}

fn systemd_remove_startup() {
    cmd!("rc-service" APP_NAME "start");
    cmd!("rc-update" "delete" APP_NAME "default");
    rm_file(SYSTEMD_PATH).unwrap();
}

fn openrc_remove_startup() {
    cmd!("systemctl" "stop" systemd_service_name());
    cmd!("systemctl" "disable" SYSTEMD_PATH);
    rm_file(SYSTEMD_PATH).unwrap();
    cmd!("systemctl" "daemon-reload");
}

fn systemd_setup_startup(args: RunArgs) {
    if !Path::new(SYSTEMD_PATH)
        .parent()
        .unwrap()
        .try_exists()
        .unwrap()
    {
        panic!("systemd not found on this system!")
    }

    let service_file = format!(
        include_str!("./systemd.template.service"),
        description = DESCRIPTION,
        exec_command = format_args!("{} run {args}", exe_path())
    );

    write_file(SYSTEMD_PATH.as_ref(), service_file.as_bytes(), 0o644).unwrap();

    cmd!("systemctl" "daemon-reload");
    cmd!("systemctl" "enable" "--now" SYSTEMD_PATH);
    cmd!("systemctl" "status" systemd_service_name());
}

fn openrc_setup_startup(args: RunArgs) {
    let service_file = format!(
        include_str!("./openrc-service"),
        description = DESCRIPTION,
        exe_path = exe_path(),
        args = format_args!("run {args}") // openrc has some VERY weird behaviour regarding CRLF see OpenRC/openrc#733
    )
    .replace("\r\n", "\n");

    write_file(OPEN_RC_PATH.as_ref(), service_file.as_bytes(), 0o755).unwrap();

    cmd!("rc-update" "add" APP_NAME "default");
    cmd!("rc-service" APP_NAME "start");
}

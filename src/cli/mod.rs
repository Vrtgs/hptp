use clap::Parser;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::IsTerminal;
use std::num::NonZero;
use std::str::FromStr;
use tracing::level_filters::LevelFilter;

use crate::cli::ports_array::PortsArray;
use crate::host::Host;
use crate::{build_runtime, real_main, AllowProtocol, ProgramArgs};

mod ports_array;
mod startup;

#[derive(Parser)]
#[command(name = "hptp")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "high performance tcp proxy", long_about = None)]
enum CliArgs {
    /// Run hptp as a cli tool
    Run(RunArgs),
    /// Set up and hptp to be run on the system daemon
    SetUpDaemon {
        #[clap(value_name = "the daemon to use for startup example")]
        #[cfg_attr(any(windows, target_os = "macos"), clap(default_value_t = Daemon::default()))]
        daemon: Daemon,
        #[clap(flatten)]
        args: RunArgs,
    },
    RemoveDaemon {
        #[clap(value_name = "the daemon to use for startup example")]
        #[cfg_attr(any(windows, target_os = "macos"), clap(default_value_t = Daemon::default()))]
        daemon: Daemon,
    },
}

const fn default_log_level() -> LevelFilter {
    const {
        match cfg!(debug_assertions) {
            true => LevelFilter::TRACE,
            false => LevelFilter::WARN,
        }
    }
}

#[derive(Parser)]
struct RunArgs {
    #[clap(long, value_name = "the host this proxy shall forward to")]
    host: Host,
    #[clap(long, value_name = r"the port\s this proxy shall forward")]
    ports: PortsArray,
    #[clap(long, alias = "v4")]
    ipv4: bool,
    #[clap(long, alias = "v6")]
    ipv6: bool,
    #[clap(long, default_value_t = default_log_level())]
    log: LevelFilter,
    #[clap(long, alias = "rt", default_value_t = RuntimeType::SingleThread)]
    runtime: RuntimeType,
}

impl RunArgs {
    fn allow_protocol(&self) -> AllowProtocol {
        match (self.ipv4, self.ipv6) {
            (false, false) | (true, false) => AllowProtocol::Ipv4,
            (false, true) => AllowProtocol::Ipv6,
            (true, true) => AllowProtocol::Both,
        }
    }
}

impl RunArgs {
    #[cfg_attr(target_os = "linux", expect(dead_code))]
    pub fn args(&self) -> impl Iterator<Item = Cow<'static, str>> + Clone {
        let mut allow_args = ["--v4", "--v6"].into_iter();
        match self.allow_protocol() {
            AllowProtocol::Ipv4 => {
                // skip "--v6"
                let _ = allow_args.next_back();
            }
            AllowProtocol::Ipv6 => {
                // skip "--v4"
                let _ = allow_args.next();
            }
            _ => {}
        };

        macro_rules! kwargs {
            ($($kw: literal, ($arg: expr) $({$to_str: ident})?),+ $(,)?) => {
                [$(Cow::Borrowed($kw), Cow::Owned(kwargs!(@resolve-to-str $arg $(, $to_str)?))),+]
            };
            (@resolve-to-str $arg: expr) => {
                $arg.to_string()
            };
            (@resolve-to-str $arg: expr, $ident: ident) => {
                $arg.$ident()
            };
        }

        allow_args.map(Cow::Borrowed).chain(kwargs!(
            "--host", (self.host) {as_string},
            "--ports", (self.ports),
            "--log", (self.log),
            "--rt", (self.runtime)
        ))
    }
}

impl Display for RunArgs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct ArgAllowProtocol(AllowProtocol);

        impl Display for ArgAllowProtocol {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    AllowProtocol::Ipv4 => f.write_str("--v4"),
                    AllowProtocol::Ipv6 => f.write_str("--v6"),
                    AllowProtocol::Both => f.write_str("--v4 --v6"),
                }
            }
        }

        f.write_fmt(format_args!(
            "{proto} --host {host} --ports \"{ports}\" --log {log} --rt {rt}",
            proto = ArgAllowProtocol(self.allow_protocol()),
            host = self.host.as_string(),
            ports = self.ports,
            log = self.log,
            rt = self.runtime
        ))
    }
}

macro_rules! arg_enum {
    (#[error_message($error_msg:literal)] $(#[$($attr:tt)*])* enum $name: ident  {
        $(#[$($inner_attr:tt)*])*
        $($val: ident = $str_val: literal),*
    }) => {paste::paste! {
        #[derive(Copy, Clone)]
        $(#[$($attr)*])*
        enum $name {
            $(#[$($inner_attr)*])*
            $($val),*
        }

        #[derive(thiserror::Error, Debug)]
        #[error($error_msg)]
        struct [<$name ParseError>](());

        impl FromStr for $name {
            type Err = [<$name ParseError>];

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(if s.eq_ignore_ascii_case($str_val) {
                    return Ok($name::$val);
                })*
                Err([<$name ParseError>](()))
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(match *self {
                    $($name::$val => $str_val),*
                })
            }
        }
    }};
}

arg_enum! {
    #[error_message("invalid runtime, expected either 'single-threaded' or 'multi-threaded'")]
    enum RuntimeType {
        SingleThread = "single-threaded",
        MultiThreaded = "multi-threaded"
    }
}

#[cfg(target_os = "linux")]
arg_enum! {
    #[error_message("invalid startup daemon, expected either 'systemd' or 'openrc'")]
    enum Daemon {
        SystemD = "systemd",
        OpenRC = "openrc"
    }
}

#[cfg(target_os = "macos")]
arg_enum! {
    #[error_message("invalid startup daemon, expected 'launch-daemon'")]
    #[derive(Default)]
    enum Daemon {
        #[default]
        LaunchDaemons = "launch-daemon"
    }
}

#[cfg(windows)]
arg_enum! {
    #[error_message("invalid startup daemon, expected 'registry'")]
    #[derive(Default)]
    enum Daemon {
        #[default]
        Registry = "registry"
    }
}

fn init_logging(log_lvl: LevelFilter) {
    if let Some(lvl) = log_lvl.into_level() {
        tracing_subscriber::fmt()
            .with_ansi(io::stdout().is_terminal())
            .with_max_level(lvl)
            .with_target(false)
            .compact()
            .init()
    }
}

pub fn main() -> ! {
    let args = match CliArgs::parse() {
        CliArgs::Run(run_args) => run_args,
        CliArgs::SetUpDaemon { daemon, args } => {
            init_logging(default_log_level());
            startup::setup_startup(daemon, args)
        }
        CliArgs::RemoveDaemon { daemon } => {
            init_logging(default_log_level());
            startup::remove_startup(daemon)
        }
    };

    init_logging(args.log);

    let allow = args.allow_protocol();
    let ports = args.ports.into_ports_vec();
    let host = args.host;

    tracing::info!("logging level is {}", args.log);

    tracing::info!("Listening on ip {allow} on ports {ports:?} and forwarding to {host}");

    let runtime_builder = match (
        args.runtime,
        std::thread::available_parallelism().map(NonZero::get),
    ) {
        (RuntimeType::MultiThreaded, Ok(threads @ 2..)) => {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.worker_threads(threads);
            builder
        }
        _ => tokio::runtime::Builder::new_current_thread(),
    };

    build_runtime(runtime_builder).block_on(real_main(ProgramArgs { ports, host, allow }))
}

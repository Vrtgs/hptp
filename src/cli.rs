use crate::host::Host;
use crate::{build_runtime, real_main, AllowProtocol, ProgramArgs};
use clap::Parser;
use itertools::Itertools;
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "hptp")]
#[command(version = "1.0")]
#[command(about = "high performance tcp proxy", long_about = None)]
pub struct CliArgs {
    #[clap(long, alias = "v4")]
    ipv4: bool,
    #[clap(long, alias = "v6")]
    ipv6: bool,
    #[clap(long, value_name = "the host this proxy shall forward to")]
    host: String,
    #[clap(long, short, value_name = "the ports this proxy shall forward")]
    ports: PortsArray,
    #[clap(long, default_value = "WARN")]
    log: log::LevelFilter,
    #[clap(long, default_value = "single-threaded")]
    runtime: RuntimeType,
}

#[derive(Copy, Clone)]
enum RuntimeType {
    CurrentThread,
    MultiThreaded,
}

#[derive(thiserror::Error, Debug)]
#[error("invalid runtime, expected 'single' or 'multi'")]
struct RuntimeTypeParseError(());

impl FromStr for RuntimeType {
    type Err = RuntimeTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("single-threaded") {
            Ok(RuntimeType::CurrentThread)
        } else if s.eq_ignore_ascii_case("multi-threaded") {
            Ok(RuntimeType::MultiThreaded)
        } else {
            Err(RuntimeTypeParseError(()))
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error(
"invalid ports array, \
expected [$(<ELM>),+] where <ELM> can be a port number, \
inclusive range x..y, or an exclusive range x..!=y\
"
)]
struct PortsArrayParseError(());

#[derive(Clone)]
struct PortsArray(Vec<u16>);

impl FromStr for PortsArray {
    type Err = PortsArrayParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| {
                enum OneOrRange<T> {
                    One(T),
                    RangeInclusive((T, T)),
                    RangeExclusive((T, T)),
                }

                use OneOrRange::*;

                let array = s
                    .split(',')
                    .map(str::trim)
                    .map(|s| {
                        s.parse::<u16>().ok().map(One).or_else(|| {
                            let parse_range =
                                |(s1, s2): (&str, &str)| Some((s1.parse().ok()?, s2.parse().ok()?));
                            s.split_once("..")
                                .and_then(parse_range)
                                .map(RangeInclusive)
                                .or_else(|| {
                                    s.split_once("..!=")
                                        .and_then(parse_range)
                                        .map(RangeExclusive)
                                })
                        })
                    })
                    .map(|item| {
                        Some(match item? {
                            One(num) => Box::new(std::iter::once(num)) as Box<dyn Iterator<Item = u16>>,
                            RangeInclusive((start, end)) => Box::new(start..=end),
                            RangeExclusive((start, end)) => Box::new(start..end),
                        })
                    })
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .unique()
                    .sorted()
                    .collect::<Vec<u16>>();

                Some(array)
            })
            .filter(|x| !x.is_empty())
            .ok_or(PortsArrayParseError(()))
            .map(PortsArray)
    }
}


pub fn main() -> ! {
    let args = CliArgs::parse();
    let lvl = match cfg!(debug_assertions) {
        true => log::LevelFilter::Trace,
        false => args.log,
    };

    if let Some(lvl) = lvl.to_level() {
        simple_logger::init_with_level(lvl).unwrap()
    }

    let allow = match (args.ipv4, args.ipv6) {
        (true, false) => AllowProtocol::Ipv4,
        (false, true) => AllowProtocol::Ipv6,
        (true, true) => AllowProtocol::Both,
        (false, false) => {
            eprintln!("must have at least one of --ipv4 or --ipv6 flags");
            std::process::abort()
        },
    };
    
    let ports = args.ports.0;
    let host = Host::new(args.host);

    log::trace!("logging level is {}", args.log);

    log::info!("Listening on ip {allow} on ports {ports:?} and forwarding to {host}");

    build_runtime(match args.runtime {
        RuntimeType::CurrentThread => tokio::runtime::Builder::new_current_thread(),
        RuntimeType::MultiThreaded => tokio::runtime::Builder::new_multi_thread(),
    })
    .block_on(real_main(ProgramArgs { ports, host, allow }))
}

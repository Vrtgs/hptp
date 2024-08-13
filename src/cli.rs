use clap::Parser;
use itertools::Itertools;
use std::io::IsTerminal;
use std::iter::FusedIterator;
use std::num::NonZero;
use std::str::FromStr;
use std::{io, ops};
use tracing::level_filters::LevelFilter;

use crate::host::Host;
use crate::{build_runtime, real_main, AllowProtocol, ProgramArgs};

#[derive(Parser)]
#[command(name = "hptp")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "high performance tcp proxy", long_about = None)]
pub struct CliArgs {
    #[clap(long, alias = "v4")]
    ipv4: bool,
    #[clap(long, alias = "v6")]
    ipv6: bool,
    #[clap(long, value_name = "the host this proxy shall forward to")]
    host: Host,
    #[clap(long, short, value_name = "the ports this proxy shall forward")]
    ports: PortsArray,
    #[clap(long)]
    log: Option<LevelFilter>,
    #[clap(long, alias = "rt", default_value = "single-threaded")]
    runtime: RuntimeType,
}

#[derive(Copy, Clone)]
enum RuntimeType {
    CurrentThread,
    MultiThreaded,
}

#[derive(thiserror::Error, Debug)]
#[error("invalid runtime, expected 'single-threaded' or 'multi-threaded'")]
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
#[cfg_attr(test, derive(PartialEq))]
struct PortsArrayParseError(());

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, Ord, PartialOrd, Eq, PartialEq))]
struct PortsArray(Vec<u16>);

impl FromStr for PortsArray {
    type Err = PortsArrayParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| {
                enum OneOrRange<T> {
                    One(T),
                    RangeInclusive(ops::RangeInclusive<T>),
                    RangeExclusive(ops::Range<T>),
                }
                use OneOrRange::*;

                impl Iterator for OneOrRange<u16> {
                    type Item = u16;

                    fn next(&mut self) -> Option<Self::Item> {
                        match *self {
                            One(one) => {
                                // empty
                                *self = RangeExclusive(0..0);
                                Some(one)
                            }
                            RangeInclusive(ref mut range) => range.next(),
                            RangeExclusive(ref mut range) => range.next(),
                        }
                    }
                }
                impl FusedIterator for OneOrRange<u16> {}

                let array = s
                    .split(',')
                    .map(str::trim)
                    .map(|s| {
                        s.parse::<u16>().ok().map(One).or_else(|| {
                            let parse_range =
                                |(s1, s2): (&str, &str)| Some((s1.parse().ok()?, s2.parse().ok()?));
                            s.split_once("..")
                                .and_then(parse_range)
                                .map(|(x, y)| RangeInclusive(x..=y))
                                .or_else(|| {
                                    s.split_once("..!=")
                                        .and_then(parse_range)
                                        .map(|(x, y)| RangeExclusive(x..y))
                                })
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
    let log_lvl = args.log.unwrap_or(
        const {
            match cfg!(debug_assertions) {
                true => LevelFilter::TRACE,
                false => LevelFilter::WARN,
            }
        },
    );

    if let Some(lvl) = log_lvl.into_level() {
        tracing_subscriber::fmt()
            .with_ansi(io::stdout().is_terminal())
            .with_max_level(lvl)
            .with_target(false)
            .compact()
            .init()
    }

    let allow = match (args.ipv4, args.ipv6) {
        (false, false) | (true, false) => AllowProtocol::Ipv4,
        (false, true) => AllowProtocol::Ipv6,
        (true, true) => AllowProtocol::Both,
    };

    let ports = args.ports.0;
    let host = args.host;

    tracing::info!("logging level is {}", log_lvl);

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

#[cfg(test)]
mod test_port_array {
    use super::*;

    #[test]
    // too slow on miri, and there is no unsafe to test
    #[cfg_attr(miri, ignore)]
    fn test_valid_ports_array() {
        // Test valid input strings and expected PortsArray values
        assert_eq!(
            "[80, 443, 20..24, 8080]".parse::<PortsArray>(),
            Ok(PortsArray(
                [80, 443]
                    .into_iter()
                    .chain(20..=24)
                    .chain([8080])
                    .unique()
                    .sorted()
                    .collect()
            ))
        );
        assert_eq!(
            format!("[0..{}]", u16::MAX).parse::<PortsArray>(),
            Ok(PortsArray((0..=u16::MAX).collect()))
        );
        assert_eq!(
            format!("[0..!={}]", u16::MAX).parse::<PortsArray>(),
            Ok(PortsArray((0..u16::MAX).collect()))
        );
    }

    #[test]
    fn test_invalid_ports_array() {
        // Test invalid input strings
        assert!("[]".parse::<PortsArray>().is_err());
        assert!("[80, 443, abc]".parse::<PortsArray>().is_err());
        assert!("[80..=abc]".parse::<PortsArray>().is_err());
        assert!("[80..!=100..=200]".parse::<PortsArray>().is_err());
    }
}

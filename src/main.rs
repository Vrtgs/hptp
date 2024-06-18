use crate::host::Host;
use crate::stream::ManyTcpListener;
use clap::Parser;
use itertools::Itertools;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::io;
use tokio::net::TcpStream;
use tokio::time::timeout;

mod dns_resolver;
mod host;
mod stream;

#[derive(strum::Display)]
enum AllowProtocol {
    #[strum(to_string = "0.0.0.0")]
    Ipv4,
    #[strum(to_string = "[::]")]
    Ipv6,
    #[strum(to_string = "0.0.0.0 and [::]")]
    Both,
}

enum Never {}

impl Never {
    fn never(self) -> ! {
        match self {}
    }
}

async fn listen(ports: Vec<u16>, host: Host, allow: AllowProtocol) -> io::Result<Never> {
    let mut listener = {
        let len = ports.len();
        match allow {
            prot @ (AllowProtocol::Ipv4 | AllowProtocol::Ipv6) => {
                let addr = match prot {
                    AllowProtocol::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    AllowProtocol::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                    _ => unreachable!(),
                };

                ManyTcpListener::bind(ports.into_iter().map(|port| (addr, port)), len).await?
            }
            AllowProtocol::Both => {
                ManyTcpListener::bind(
                    ports.into_iter().flat_map(|port| {
                        [
                            SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
                            SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)),
                        ]
                    }),
                    len * 2,
                )
                .await?
            }
        }
    };

    loop {
        let res = listener
            .accept()
            .await
            .inspect(|(_, local, peer)| log::info!("New connection from `{peer}` to `{local}`"));

        let Ok((mut stream, local, _)) = res else {
            log::debug!("connection failed {res:?}");
            continue;
        };

        let local_port = local.port();
        tokio::spawn(async move {
            let _res = async move {
                let mut forward_stream = timeout(Duration::from_secs(15), async {
                    TcpStream::connect(&*host.to_hosts(local_port).await?).await
                })
                .await
                .inspect_err(|_| log::debug!("connecting to {host} timed out"))??;
                io::copy_bidirectional(&mut stream, &mut forward_stream).await
            }
            .await;

            match _res {
                Ok((c, s)) => log::info!(
                    "Completed connection successfully, metrics {{ client: {c}, server: {s} }}"
                ),
                Err(e) => log::error!("Error during copy: {e}"),
            }
        });
    }
}

fn parse_array(str: impl AsRef<str>) -> Option<Vec<u16>> {
    str.as_ref()
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
                .collect();

            Some(array)
        })
}

#[derive(Parser)]
#[command(name = "hptp")]
#[command(version = "1.0")]
#[command(about = "high performance tcp proxy", long_about = None)]
struct CliArgs {
    #[clap(long)]
    ipv4: bool,
    #[clap(long)]
    ipv6: bool,
    #[clap(long, value_name = "the host this tcp proxy shall forward to")]
    host: String,
    #[clap(long, short, value_name = "the host this tcp proxy shall forward to")]
    ports: String,
}

async fn real_main() -> ! {
    let args = CliArgs::parse();
    let allow = match (args.ipv4, args.ipv6) {
        (true, false) => AllowProtocol::Ipv4,
        (false, true) => AllowProtocol::Ipv6,
        (true, true) => AllowProtocol::Both,
        (false, false) => panic!("must have at least one of --ipv4 or --ipv6 flags"),
    };

    let ports = parse_array(args.ports).expect("invalid ports allow array");

    let host = Host::new(args.host);

    log::info!("Listening on ip {allow} on ports {ports:?} and forwarding to {host}");

    listen(ports, host, allow)
        .await
        .unwrap_or_else(|err| {
            log::error!("FATAL ERROR: {err}");
            std::process::abort()
        })
        .never()
}

fn main() -> ! {
    simple_logger::init_with_level(log::Level::Trace).unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .on_thread_start(|| {
            // create thread_rng
            log::trace!("runtime thread starting...");
            rand::thread_rng();
        })
        .on_thread_stop(|| {
            log::trace!("runtime thread stopping...");
        })
        .build()
        .expect("runtime builder failed")
        .block_on(real_main())
}

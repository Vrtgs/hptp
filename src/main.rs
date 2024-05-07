use crate::host::Host;
use crate::stream::ManyTcpListener;
use clap::Parser;
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
                .await??;
                io::copy_bidirectional(&mut stream, &mut forward_stream).await
            }
            .await;

            match _res {
                Ok(m) => log::info!("Completed connection successfully, metrics {m:?}"),
                Err(e) => log::debug!("Error during copy: {e}"),
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

            let iter = s.split(',').map(str::trim).map(|s| {
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
            });

            let mut nums = vec![];
            for item in iter {
                match item? {
                    One(num) => nums.push(num),
                    RangeInclusive((start, end)) => nums.extend(start..=end),
                    RangeExclusive((start, end)) => nums.extend(start..end),
                }
            }

            nums.sort_unstable();
            nums.dedup();

            Some(nums)
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
    #[cfg(debug_assertions)]
    simple_logger::init_with_level(log::Level::Trace).unwrap();

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

    listen(ports, host, allow).await.unwrap().never()
}

fn main() -> ! {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .on_thread_start(|| {
            rand::thread_rng();
        })
        .build()
        .expect("runtime builder failed")
        .block_on(real_main())
}

use monoio::net::TcpStream;
use monoio::time::timeout;
use std::fmt::Display;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::num::NonZero;
use std::time::Duration;
use tracing::instrument;

use crate::host::Host;
use crate::stream::{ManyRecvResult, ManyTcpListener};

mod dns_resolver;
mod host;
mod stream;

#[cfg(feature = "cli")]
mod cli;
mod sock_io;

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

#[instrument(level = "error", skip_all, fields(peer = display(_peer), port = display(port)))]
async fn copy_to(host: Host, port: u16, downstream: TcpStream, _peer: SocketAddr) {
    let res = async move {
        let upstream = timeout(Duration::from_secs(15), async {
            TcpStream::connect(&*host.to_hosts(port).await?).await
        })
        .await
        .inspect(|_| tracing::trace!("Successfully connected to {host}"))
        .inspect_err(|_| tracing::debug!("Connecting to {host} timed out"))??;

        sock_io::copy_socks(downstream, upstream).await
    }
    .await;

    match res {
        Ok((c, s)) => {
            tracing::info!("connection successful, metrics {{ client: {c}, server: {s} }}")
        }
        Err(e) => tracing::error!("{e}"),
    }
}

async fn listen(ports: Vec<NonZero<u16>>, host: Host, allow: AllowProtocol) -> io::Result<Never> {
    let mut listener = {
        let len = ports.len();
        match allow {
            proto @ (AllowProtocol::Ipv4 | AllowProtocol::Ipv6) => {
                let addr = match proto {
                    AllowProtocol::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    AllowProtocol::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                    _ => unreachable!(),
                };

                ManyTcpListener::bind(ports.into_iter().map(|port| (addr, port.get())), len).await?
            }
            AllowProtocol::Both => {
                ManyTcpListener::bind(
                    ports.into_iter().flat_map(|port| {
                        [
                            SocketAddr::from((Ipv4Addr::UNSPECIFIED, port.get())),
                            SocketAddr::from((Ipv6Addr::UNSPECIFIED, port.get())),
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
            .inspect(|ManyRecvResult { local, peer, .. }| {
                tracing::info!("New connection from `{peer}` to `{local}`")
            });

        let Ok(ManyRecvResult {
            stream,
            local,
            peer,
        }) = res
        else {
            tracing::warn!("Connection failed {res:?}");
            continue;
        };

        monoio::spawn(copy_to(host, local.port(), stream, peer));
    }
}

pub struct ProgramArgs {
    ports: Vec<NonZero<u16>>,
    host: Host,
    allow: AllowProtocol,
}

pub async fn real_main(args: ProgramArgs) -> ! {
    listen(args.ports, args.host, args.allow)
        .await
        .map(Never::never)
        .unwrap_or_else(|err| panic!("{err}"))
}

pub fn set_hooks() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| &**s))
            .unwrap_or("Box<dyn Any>");

        match cfg!(debug_assertions) {
            true => {
                let location = info
                    .location()
                    .map_or(&"<unknown file>" as &dyn Display, |loc| loc);
                tracing::error!("Fatal: panicked at {location}: [{msg}]")
            }
            false => tracing::error!("Fatal: panicked at [{msg}]"),
        }
    }))
}

fn main() {
    set_hooks();

    cfg_if::cfg_if! {
        if #[cfg(feature = "cli")] {
            cli::main()
        } else {
            compile_error!("Unknown startup point")
        }
    }
}

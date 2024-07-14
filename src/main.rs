use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use tokio::io;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::instrument;

use crate::host::Host;
use crate::stream::ManyTcpListener;

mod dns_resolver;
mod host;
mod stream;

#[cfg(feature = "cli")]
mod cli;
mod sock_io;

#[cfg(any(windows, target_os = "linux"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
async fn copy_to(host: Host, port: u16, mut stream: TcpStream, _peer: SocketAddr) {
    let res = async move {
        let mut forward_stream = timeout(Duration::from_secs(15), async {
            TcpStream::connect(&*host.to_hosts(port).await?).await
        })
        .await
        .inspect(|_| tracing::trace!("Successfully connected to {host}"))
        .inspect_err(|_| tracing::debug!("Connecting to {host} timed out"))??;

        sock_io::copy_socks(&mut stream, &mut forward_stream).await
    }
    .await;

    match res {
        Ok((c, s)) => {
            tracing::info!("connection successful, metrics {{ client: {c}, server: {s} }}")
        }
        Err(e) => tracing::error!("{e}"),
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
        let res = listener.accept().await.inspect(|(_, local, peer)| {
            tracing::info!("New connection from `{peer}` to `{local}`")
        });

        let Ok((stream, local, peer)) = res else {
            tracing::warn!("Connection failed {res:?}");
            continue;
        };

        tokio::spawn(copy_to(host, local.port(), stream, peer));
    }
}

pub struct ProgramArgs {
    ports: Vec<u16>,
    host: Host,
    allow: AllowProtocol,
}

pub async fn real_main(args: ProgramArgs) -> ! {
    listen(args.ports, args.host, args.allow)
        .await
        .map(Never::never)
        .unwrap_or_else(|err| panic!("{err}"))
}

pub fn build_runtime(mut builder: tokio::runtime::Builder) -> tokio::runtime::Runtime {
    builder
        .enable_all()
        .on_thread_start(|| tracing::trace!("runtime thread starting..."))
        .on_thread_stop(|| tracing::trace!("runtime thread stopping..."))
        .build()
        .expect("runtime builder failed")
}

pub fn set_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&str>() {
            Some(s) => s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => s,
                None => "Box<dyn Any>",
            },
        };

        match cfg!(debug_assertions) {
            true => {
                let location = info
                    .location()
                    .map(|x| x as &dyn Display)
                    .unwrap_or_else(|| &"<unknown file>");
                tracing::error!("Fatal: panicked at {location}: [{msg}]")
            }
            false => tracing::error!("Fatal: panicked at [{msg}]"),
        }
    }))
}

fn main() {
    set_panic_hook();

    cfg_if::cfg_if! {
        if #[cfg(feature = "cli")] {
            cli::main()
        } else {
            compile_error!("Unknown startup point")
        }
    }
}

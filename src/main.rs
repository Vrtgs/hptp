use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Once;
use std::time::Duration;

use tokio::io;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::host::Host;
use crate::stream::ManyTcpListener;

mod dns_resolver;
mod host;
mod stream;

#[cfg(feature = "cli")]
mod cli;

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
            log::warn!("connection failed {res:?}");
            continue;
        };

        let local_port = local.port();
        tokio::spawn(async move {
            let _res = async move {
                let mut forward_stream = timeout(Duration::from_secs(15), async {
                    TcpStream::connect(&*host.to_hosts(local_port).await?).await
                })
                .await
                .inspect(|_| log::trace!("successfully connected to {host}"))
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

pub struct ProgramArgs {
    ports: Vec<u16>,
    host: Host,
    allow: AllowProtocol,
}

pub async fn real_main(args: ProgramArgs) -> ! {
    listen(args.ports, args.host, args.allow)
        .await
        .map(Never::never)
        .unwrap_or_else(|err| {
            log::error!("FATAL: {err}");
            std::process::abort()
        })
}

pub fn build_runtime(mut builder: tokio::runtime::Builder) -> tokio::runtime::Runtime {
    // make sure thread rng is init
    // and run tracing metrics
    rand::thread_rng();

    builder
        .enable_all()
        .on_thread_start(|| {
            log::trace!("runtime thread starting...");
            rand::thread_rng();
        })
        .on_thread_stop(|| {
            log::trace!("runtime thread stopping...");
        })
        .build()
        .expect("runtime builder failed")
}

pub fn set_panic_hook() {
    static SET_ONCE: Once = Once::new();

    SET_ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let location = info
                .location()
                .map(|x| x as &dyn Display)
                .unwrap_or_else(|| &"<unknown file>");
            let msg = match info.payload().downcast_ref::<&str>() {
                Some(s) => s,
                None => match info.payload().downcast_ref::<String>() {
                    Some(s) => s,
                    None => "Box<dyn Any>",
                },
            };

            log::error!("FATAL: panicked at {location}: [{msg}]")
        }))
    })
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

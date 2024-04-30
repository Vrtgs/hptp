use crate::filter::{Filter, FilterMode, Port};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

mod filter;

macro_rules! ip_blacklist {
    () => {
        include!("ip-blacklist")
            .map(std::convert::identity::<&str>)
            .map(str::parse::<IpAddr>)
            .map(Result::unwrap)
    };
}

macro_rules! port_whitelist {
    () => {
        include!("allowed-ports")
    };
}

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    simple_logger::init_with_level(log::Level::Trace).unwrap();

    let listener = TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).await.unwrap();

    let port_filter = Filter::<Port>::new(&port_whitelist!(), FilterMode::WhiteList);
    let ip_filter = Filter::<IpAddr>::new(&ip_blacklist!(), FilterMode::BlackList);
    let socket_filter = Filter::<SocketAddr>::new(ip_filter, port_filter);

    loop {
        let res = listener
            .accept()
            .await
            .and_then(|(s, peer)| {
                let port = s.local_addr()?.port();
                Ok((s, SocketAddr::new(peer.ip(), port)))
            })
            .inspect(|(_, peer)| log::info!("New connection from {peer}"))
            .map(|(stream, peer)| socket_filter.allowed(peer).then_some((stream, peer)));

        let Ok(Some((mut stream, peer))) = res else {
            log::debug!("connection failed {res:?}");
            continue;
        };

        tokio::spawn(async move {
            io::copy_bidirectional(
                &mut stream,
                &mut TcpStream::connect(("vrtgs.xyz", peer.port())).await?,
            )
            .await
        });
    }
}

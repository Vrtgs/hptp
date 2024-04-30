use crate::stream::ManyTcpListener;
use std::net::Ipv6Addr;
use tokio::io;
use tokio::net::TcpStream;

mod stream;


macro_rules! port_whitelist {
    () => {
        include!("allowed-ports")
    };
}

async fn listen(ports: &[u16]) -> io::Result<()> {
    let mut listener = ManyTcpListener::bind(
        ports.iter().map(|&port| (Ipv6Addr::UNSPECIFIED, port)),
        ports.len(),
    )
    .await?;

    loop {
        let res = listener.accept()
            .await
            .inspect(|(_, peer)| log::info!("New connection from {peer}"))
            .map(|(stream, peer)| Some((stream, peer))); // todo: ip filtering

        let Ok(Some((mut stream, _))) = res else {
            log::debug!("connection failed {res:?}");
            continue;
        };

        tokio::spawn(async move {
            let port = stream.local_addr()?.port();
            io::copy_bidirectional(
                &mut stream,
                &mut TcpStream::connect(("vrtgs.xyz", port)).await?,
            )
            .await
        });
    }
}

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    simple_logger::init_with_level(log::Level::Trace).unwrap();
    listen(&port_whitelist!()).await.unwrap()
}

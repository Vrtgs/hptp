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

async fn listen(ports: &[u16], addr: &'static str) -> io::Result<()> {
    let mut listener = ManyTcpListener::bind(
        ports.iter().map(|&port| (Ipv6Addr::UNSPECIFIED, port)),
        ports.len(),
    )
    .await?;

    loop {
        let res = listener
            .accept()
            .await
            .inspect(|(_, local, peer)| log::info!("New connection from `{peer}` to `{local}`"));

        let Ok((mut stream, _, _)) = res else {
            log::debug!("connection failed {res:?}");
            continue;
        };

        tokio::spawn(async move {
            let port = stream.local_addr()?.port();
            io::copy_bidirectional(&mut stream, &mut TcpStream::connect((addr, port)).await?).await
        });
    }
}

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    simple_logger::init_with_level(log::Level::Trace).unwrap();
    listen(&port_whitelist!(), "vrtgs.xyz").await.unwrap()
}

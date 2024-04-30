use crate::stream::ManyTcpListener;
use std::net::Ipv6Addr;
use tokio::io;
use tokio::net::TcpStream;

mod stream;

macro_rules! port_whitelist {
    () => {
        include!("../allowed-ports")
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

        let Ok((mut stream, local, _)) = res else {
            log::debug!("connection failed {res:?}");
            continue;
        };

        let local_port = local.port();
        tokio::spawn(async move {
            io::copy_bidirectional(
                &mut stream,
                &mut TcpStream::connect((addr, local_port)).await?,
            )
            .await
        });
    }
}

fn parse_array(str: &str) -> Option<Box<[u16]>> {
    str.trim()
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
                        .or_else(|| s.split_once("..!=").and_then(parse_range).map(RangeExclusive))
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

            Some(nums.into_boxed_slice())
        })
}

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    simple_logger::init_with_level(log::Level::Trace).unwrap();
    listen(
        &parse_array(&tokio::fs::read_to_string("./allowed-ports").await.unwrap())
            .expect("invalid ports allow array"),
        "vrtgs.xyz",
    )
    .await
    .unwrap()
}

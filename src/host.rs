use crate::dns_resolver::DnsResolver;
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use tokio::io;

#[derive(Copy, Clone)]
pub enum Host {
    IpAddr(IpAddr),
    Host(&'static str, &'static DnsResolver),
}

impl Host {
    pub fn new(s: String) -> Self {
        if let Ok(addr) = IpAddr::from_str(s.trim()) {
            return Host::IpAddr(addr);
        }

        Host::Host(s.leak().trim(), Box::leak(Box::<DnsResolver>::default()))
    }

    pub async fn to_hosts(self, port: u16) -> io::Result<SmallVec<SocketAddr, 1>> {
        Ok(match self {
            Host::IpAddr(ip) => smallvec::smallvec![(ip, port).into()],
            Host::Host(host, resolver) => resolver.resolve(host, port).await?,
        })
    }
}

impl Display for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            Host::IpAddr(ref ip) => <IpAddr as Display>::fmt(ip, f),
            Host::Host(host, _) => <str as Debug>::fmt(host, f),
        }
    }
}

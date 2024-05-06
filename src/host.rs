use std::fmt::{Debug, Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use smallvec::SmallVec;
use tokio::io;

#[derive(Copy, Clone)]
pub enum Host {
    IpAddr(IpAddr),
    Host(&'static str)
}

impl Host {
    pub fn new(s: String) -> Self {
        if let Ok(addr) = IpAddr::from_str(s.trim()) {
            return Host::IpAddr(addr);
        }

        Host::Host(s.leak().trim())
    }

    pub async fn to_hosts(self, port: u16) -> io::Result<SmallVec<SocketAddr, 1>> {
        match self {
            Host::IpAddr(ip) => Ok(smallvec::smallvec![(ip, port).into()]),
            Host::Host(host) => {
                Ok(tokio::net::lookup_host((host, port)).await?.collect())
            }
        }
    }
}

impl Display for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            Host::IpAddr(ref ip) => <IpAddr as Display>::fmt(ip, f),
            Host::Host(host) => <str as Debug>::fmt(host, f)
        }
    }
}
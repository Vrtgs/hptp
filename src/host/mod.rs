use std::fmt::{Debug, Display, Formatter, Write};
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::OnceLock;

use hickory_resolver::proto::ProtoError as DnsProtoError;
use hickory_resolver::Name;
use smallvec::SmallVec;

use crate::dns_resolver::DnsResolver;
use crate::host::host_bitpacked::{AlignedIp, DynamicHost, HostRpr};

mod host_bitpacked;

pub type Host = host_bitpacked::Host;

pub fn try_insert_with<T, F: FnOnce() -> T>(once_lock: &OnceLock<T>, f: F) -> Option<&T> {
    let mut run = Some(f);
    let res = once_lock.get_or_init(|| run.take().unwrap()());
    match run {
        None => Some(res),
        Some(_) => None,
    }
}

impl FromStr for Host {
    type Err = DnsProtoError;

    fn from_str(s: &str) -> Result<Self, DnsProtoError> {
        static DYNAMIC_HOST: OnceLock<Result<DynamicHost, DnsProtoError>> = OnceLock::new();
        static STATIC_HOST: OnceLock<AlignedIp> = OnceLock::new();

        const MANY_HOST_ERROR: &str = "Can only make one host per program";

        let s = s.trim();
        if let Ok(addr) = IpAddr::from_str(s) {
            let rpr = try_insert_with(&STATIC_HOST, || AlignedIp(addr)).expect(MANY_HOST_ERROR);

            return Ok(Host::from(rpr));
        }

        let host = try_insert_with(&DYNAMIC_HOST, || {
            Name::from_str(s).map(|name| DynamicHost {
                name,
                resolver: DnsResolver::default(),
            })
        })
        .expect(MANY_HOST_ERROR);

        Ok(Host::from(host.as_ref().map_err(Clone::clone)?))
    }
}

impl Host {
    pub fn as_string(self) -> String {
        match self.as_repr() {
            HostRpr::Static(ip) => ip.to_string(),
            HostRpr::Dynamic(host) => host.name.to_string(),
        }
    }

    pub async fn to_hosts(self, port: u16) -> io::Result<SmallVec<SocketAddr, 4>> {
        Ok(match self.as_repr() {
            HostRpr::Static(&ip) => smallvec::smallvec![SocketAddr::new(ip, port)],
            HostRpr::Dynamic(host) => host.resolver.resolve(host.name.clone(), port).await?,
        })
    }
}

impl Display for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.as_repr() {
            HostRpr::Static(ip) => <IpAddr as Display>::fmt(ip, f),
            HostRpr::Dynamic(host) => {
                f.write_char('"')?;
                <Name as Display>::fmt(&host.name, f)?;
                f.write_char('"')
            }
        }
    }
}

impl Debug for Host {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct DebugDisplay<T>(T);
        impl<T: Display> Debug for DebugDisplay<T> {
            #[inline]
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }

        f.debug_tuple("Host").field(&DebugDisplay(*self)).finish()
    }
}

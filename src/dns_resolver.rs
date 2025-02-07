use hickory_resolver::config::{LookupIpStrategy, ResolveHosts, ResolverConfig, ResolverOpts};
use hickory_resolver::ResolveError;
use hickory_resolver::{Name, TokioResolver};
use smallvec::SmallVec;
use std::convert::Infallible;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::thread::available_parallelism;

pub struct DnsResolver(TokioResolver);

static GLOBAL_RT: LazyLock<tokio::runtime::Handle> = LazyLock::new(|| {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let handle = rt.handle().clone();
    std::thread::spawn(move || rt.block_on(std::future::pending::<Infallible>()));
    handle
});

impl DnsResolver {
    pub async fn resolve(
        &'static self,
        host: Name,
        port: u16,
    ) -> Result<SmallVec<SocketAddr, 4>, ResolveError> {
        GLOBAL_RT
            .spawn(async move {
                Ok(self
                    .0
                    .lookup_ip(host)
                    .await?
                    .iter()
                    .map(|ip| (ip, port).into())
                    .collect())
            })
            .await
            .unwrap()
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        let resolver = TokioResolver::tokio(ResolverConfig::cloudflare(), {
            let mut opts = ResolverOpts::default();
            opts.cache_size = 128;
            opts.attempts = 8;
            opts.num_concurrent_reqs = available_parallelism()
                .map_or(1, NonZeroUsize::get)
                .saturating_mul(32);

            opts.use_hosts_file = ResolveHosts::Always;
            opts.try_tcp_on_error = true;
            opts.ip_strategy = LookupIpStrategy::Ipv4thenIpv6;
            opts
        });

        DnsResolver(resolver)
    }
}

impl Debug for DnsResolver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DnsResolver").finish_non_exhaustive()
    }
}

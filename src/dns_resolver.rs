use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::thread::available_parallelism;

use hickory_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};
use hickory_resolver::error::ResolveError;
use hickory_resolver::{Name, TokioAsyncResolver};
use smallvec::SmallVec;

pub struct DnsResolver(TokioAsyncResolver);

impl DnsResolver {
    pub async fn resolve(
        &self,
        host: Name,
        port: u16,
    ) -> Result<SmallVec<SocketAddr, 1>, ResolveError> {
        Ok(self
            .0
            .lookup_ip(host)
            .await?
            .iter()
            .map(|ip| (ip, port).into())
            .collect())
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        let resolver = TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), {
            let mut opts = ResolverOpts::default();
            opts.cache_size = 128;
            opts.attempts = 8;
            opts.num_concurrent_reqs = available_parallelism()
                .map_or(1, NonZeroUsize::get)
                .saturating_mul(8);

            opts.use_hosts_file = true;
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

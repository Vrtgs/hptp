use hickory_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};
use hickory_resolver::error::ResolveError;
use hickory_resolver::TokioAsyncResolver;
use smallvec::{smallvec, SmallVec};
use std::net::SocketAddr;
use std::thread::available_parallelism;

pub struct DnsResolver(TokioAsyncResolver);

impl DnsResolver {
    pub async fn resolve(
        &self,
        host: &str,
        port: u16,
    ) -> Result<SmallVec<SocketAddr, 1>, ResolveError> {
        if let Ok(host) = host.parse() {
            return Ok(smallvec![SocketAddr::new(host, port)]);
        }

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
            opts.num_concurrent_reqs = available_parallelism().unwrap().get();
            opts.use_hosts_file = true;
            opts.try_tcp_on_error = true;
            opts.ip_strategy = LookupIpStrategy::Ipv4thenIpv6;
            opts
        });

        DnsResolver(resolver)
    }
}

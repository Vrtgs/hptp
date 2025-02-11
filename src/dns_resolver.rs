use futures::channel::oneshot;
use hickory_resolver::config::{LookupIpStrategy, ResolveHosts, ResolverConfig, ResolverOpts};
use hickory_resolver::dns_lru::{DnsLru, TtlConfig};
use hickory_resolver::lookup_ip::LookupIp;
use hickory_resolver::proto::op::Query;
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::ResolveError;
use hickory_resolver::{Name, TokioResolver};
use smallvec::SmallVec;
use std::fmt::{Debug, Formatter};
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::thread::available_parallelism;
use std::time::Instant;

pub struct DnsResolver(DnsLru);

type DomainRequest = (Name, oneshot::Sender<Result<LookupIp, ResolveError>>);

static DNS_RESOLVER: LazyLock<flume::Sender<DomainRequest>> = LazyLock::new(|| {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let (tx, rx) = flume::unbounded::<DomainRequest>();
    let fut = async move {
        let resolver = TokioResolver::tokio(ResolverConfig::cloudflare(), {
            let mut opts = ResolverOpts::default();
            opts.cache_size = 0;
            opts.attempts = 8;
            opts.num_concurrent_reqs = available_parallelism()
                .map_or(1, NonZeroUsize::get)
                .saturating_mul(32);

            opts.use_hosts_file = ResolveHosts::Always;
            opts.try_tcp_on_error = true;
            opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
            opts
        });

        while let Ok((name, tx)) = rx.recv_async().await {
            let _ = tx.send(resolver.lookup_ip(name).await);
        }
    };
    std::thread::spawn(move || rt.block_on(fut));

    tx
});

impl DnsResolver {
    pub async fn resolve(
        &'static self,
        host: Name,
        port: u16,
    ) -> Result<SmallVec<SocketAddr, 4>, ResolveError> {
        let mut query = Query::query(host, RecordType::A);
        let now = Instant::now();

        let res = self.0.get(&query, now).or_else(|| {
            query.set_query_type(RecordType::AAAA);
            self.0.get(&query, now)
        });

        let iter = match res {
            Some(res) => res?.into(),
            None => {
                let (tx, rx) = oneshot::channel();
                DNS_RESOLVER
                    .send((query.into_parts().name, tx))
                    .map_err(|_| "dns resolver disconnected")?;
                let ret = rx.await.map_err(|_| "dns resolver didn't reply")??;

                self.0.insert_records(
                    ret.query().clone(),
                    ret.as_lookup().records().iter().cloned(),
                    Instant::now(),
                );

                ret
            }
        };

        Ok(iter.iter().map(|ip| (ip, port).into()).collect())
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        DnsResolver(DnsLru::new(128, TtlConfig::default()))
    }
}

impl Debug for DnsResolver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DnsResolver").finish_non_exhaustive()
    }
}

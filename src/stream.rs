use std::net::SocketAddr;
use std::task::{Context, Poll};

use futures::{stream, StreamExt, TryStreamExt};
use rand::seq::SliceRandom;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

struct RateLimiter(u16);

impl RateLimiter {
    fn new() -> Self {
        Self(0)
    }

    fn try_do<F: FnOnce()>(&mut self, f: F) {
        let (cnt, overflow) = self.0.overflowing_add(1);
        if overflow {
            f()
        }
        // if a panic happened don't count it
        self.0 = cnt;
    }
}

pub struct ManyTcpListener {
    listeners: Box<[(TcpListener, SocketAddr)]>,
    shuffle_limit: RateLimiter,
}

impl ManyTcpListener {
    pub async fn bind<A: Into<SocketAddr>>(
        addrs: impl IntoIterator<Item = A>,
        bind_concurrent: usize,
    ) -> io::Result<Self> {
        let stream = stream::iter(addrs.into_iter().map(Into::into))
            .map(|addr| async move { TcpListener::bind(addr).await.map(|l| (l, addr)) });

        let mut listeners = match bind_concurrent {
            2.. => {
                stream
                    .buffer_unordered(bind_concurrent)
                    .try_collect::<Vec<_>>()
                    .await
            }
            _ => stream.then(|x| x).try_collect::<Vec<_>>().await,
        }?
        .into_boxed_slice();

        listeners.shuffle(&mut rand::rng());

        Ok(Self {
            listeners,
            shuffle_limit: RateLimiter::new(),
        })
    }

    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(TcpStream, SocketAddr, SocketAddr)>> {
        #[inline(always)]
        fn select<T, B, F: FnMut(&mut T) -> Poll<B>>(slice: &mut [T], mut f: F) -> Poll<B> {
            for x in slice {
                if let res @ Poll::Ready(..) = f(x) {
                    return res;
                }
            }
            Poll::Pending
        }

        self.shuffle_limit
            .try_do(|| self.listeners.shuffle(&mut rand::rng()));

        select(&mut self.listeners, |(listener, local)| {
            listener.poll_accept(cx).map_ok(|(s, p)| (s, *local, p))
        })
    }

    /// this returns
    /// ((established TcpStream between \<local\> and \<remote\>), \<local\>, \<remote\>)
    pub async fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr, SocketAddr)> {
        std::future::poll_fn(|cx| self.poll_accept(cx)).await
    }
}

use futures::{StreamExt, TryStreamExt};
use rand::seq::SliceRandom;
use std::net::SocketAddr;
use std::task::{Context, Poll};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

pub struct ManyTcpListener(Box<[(TcpListener, SocketAddr)]>);

impl ManyTcpListener {
    pub async fn bind<A: Into<SocketAddr>>(
        addrs: impl IntoIterator<Item = A>,
        bind_concurrent: usize,
    ) -> io::Result<Self> {
        futures::stream::iter(addrs.into_iter().map(Into::into))
            .map(|addr| async move { Ok((TcpListener::bind(addr).await?, addr)) })
            .buffer_unordered(bind_concurrent.max(1))
            .try_collect::<Vec<_>>()
            .await
            .map(Vec::into_boxed_slice)
            .map(Self)
    }

    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(TcpStream, SocketAddr, SocketAddr)>> {
        self.0.shuffle(&mut rand::thread_rng());
        let item = self
            .0
            .iter_mut()
            .find_map(|listener| match listener.0.poll_accept(cx) {
                Poll::Ready(res) => Some(res.map(|(s, p)| (s, listener.1, p))),
                Poll::Pending => None,
            });

        match item {
            Some(sock) => Poll::Ready(sock),
            None => Poll::Pending,
        }
    }

    /// this returns
    /// (<established TcpStream between <local> and <remote>>, <local>, <remote>)
    pub async fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr, SocketAddr)> {
        std::future::poll_fn(|cx| self.poll_accept(cx)).await
    }
}

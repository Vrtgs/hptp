use std::io;
use std::net::SocketAddr;

use futures::future::Either;
use futures::{stream, FutureExt, StreamExt, TryStreamExt};
use monoio::net::{TcpListener, TcpStream};

#[derive(Debug)]
pub struct ManyRecvResult {
    pub stream: TcpStream,
    pub peer: SocketAddr,
    pub local: SocketAddr,
}

pub struct ManyTcpListener {
    listeners: flume::Receiver<io::Result<ManyRecvResult>>,
}

impl ManyTcpListener {
    pub async fn bind<A: Into<SocketAddr>>(
        addrs: impl IntoIterator<Item = A>,
        bind_concurrent: usize,
    ) -> io::Result<Self> {
        let stream = stream::iter(addrs.into_iter().map(Into::into))
            .map(|addr| async move { TcpListener::bind(addr).map(|l| (l, addr)) });

        let (tx, rx) = flume::unbounded();
        let listener_op = |tx: flume::Sender<_>, listener: TcpListener, local_sock| async move {
            monoio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((stream, peer)) => {
                            let res = ManyRecvResult {
                                stream,
                                peer,
                                local: local_sock,
                            };
                            let Ok(()) = tx.send(Ok(res)) else { break };
                        }
                        Err(err) => {
                            let _ = tx.send(Err(err));
                            break;
                        }
                    }
                }
            });
        };

        let stream = match bind_concurrent {
            n @ 2.. => Either::Right(stream.buffer_unordered(n)),
            _ => Either::Left(stream.then(std::convert::identity)),
        };

        stream
            .try_for_each(|(listener, sock)| {
                let tx = tx.clone();
                listener_op(tx, listener, sock).map(Ok)
            })
            .await?;

        Ok(Self { listeners: rx })
    }

    /// this returns
    /// ((established TcpStream between \<local\> and \<remote\>), \<local\>, \<remote\>)
    pub async fn accept(&mut self) -> io::Result<ManyRecvResult> {
        self.listeners.recv_async().await.unwrap_or_else(|_| {
            panic!();
            // Err(io::Error::other("all listeners disconnected"))
        })
    }
}

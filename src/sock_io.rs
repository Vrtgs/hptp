use monoio::io::{AsyncWriteRent, Splitable};
use monoio::net::TcpStream;
use std::io;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        use monoio::io::zero_copy as copy_impl;
    } else {
        use monoio::io::copy as copy_impl;
    }
}

pub async fn copy_socks(a: TcpStream, b: TcpStream) -> io::Result<(u64, u64)> {
    let (mut ra, mut wa) = a.into_split();
    let (mut rb, mut wb) = b.into_split();

    monoio::try_join!(
        async move {
            let written = copy_impl(&mut ra, &mut wb).await?;
            wb.shutdown().await?;
            Ok(written)
        },
        async move {
            let written = copy_impl(&mut rb, &mut wa).await?;
            wa.shutdown().await?;
            Ok(written)
        }
    )
}

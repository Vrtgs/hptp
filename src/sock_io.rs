cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        pub use tokio_splice::zero_copy_bidirectional as copy_socks;
    } else {
        pub use tokio::io::copy_bidirectional as copy_socks;
    }
}

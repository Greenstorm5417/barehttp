pub mod adapter;
mod os;

pub use adapter::BlockingSocket;
pub use core::net::SocketAddr;
pub use os::OsSocket as OsBlockingSocket;

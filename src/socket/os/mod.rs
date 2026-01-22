#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::OsSocket;

#[cfg(windows)]
pub use windows::OsSocket;

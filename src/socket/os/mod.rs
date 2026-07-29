#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(not(any(unix, windows)))]
mod stub;

#[cfg(unix)]
pub use unix::OsSocket;

#[cfg(windows)]
pub use windows::OsSocket;

#[cfg(not(any(unix, windows)))]
pub use stub::OsSocket;

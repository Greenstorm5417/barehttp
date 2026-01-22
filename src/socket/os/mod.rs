#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos::OsSocket;

#[cfg(all(unix, not(target_os = "macos")))]
pub use unix::OsSocket;

#[cfg(windows)]
pub use windows::OsSocket;

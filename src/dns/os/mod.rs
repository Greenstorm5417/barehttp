#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::resolve_host;

#[cfg(windows)]
pub use windows::resolve_host;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos::resolve_host;

#[cfg(all(unix, not(target_os = "macos")))]
pub use unix::resolve_host;

#[cfg(windows)]
pub use windows::resolve_host;

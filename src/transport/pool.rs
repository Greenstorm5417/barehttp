use crate::socket::BlockingSocket;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PoolKey {
  host: String,
  port: u16,
}

impl PoolKey {
  pub const fn new(
    host: String,
    port: u16,
  ) -> Self {
    Self { host, port }
  }
}

pub struct PooledSocket<S> {
  socket: S,
  last_used: core::time::Duration,
}

pub struct ConnectionPool<S> {
  connections: BTreeMap<PoolKey, Vec<PooledSocket<S>>>,
  max_idle_per_host: usize,
  idle_timeout: Option<Duration>,
}

impl<S: BlockingSocket> ConnectionPool<S> {
  pub const fn new(
    max_idle_per_host: usize,
    idle_timeout: Option<Duration>,
  ) -> Self {
    Self {
      connections: BTreeMap::new(),
      max_idle_per_host,
      idle_timeout,
    }
  }

  pub fn get(
    &mut self,
    key: &PoolKey,
  ) -> Option<S> {
    let sockets = self.connections.get_mut(key)?;

    while let Some(pooled) = sockets.pop() {
      if let Some(timeout) = self.idle_timeout {
        let now = Self::current_time();
        let elapsed = now.saturating_sub(pooled.last_used);
        if elapsed > timeout {
          continue;
        }
      }
      return Some(pooled.socket);
    }

    None
  }

  pub fn return_connection(
    &mut self,
    key: PoolKey,
    socket: S,
  ) {
    let sockets = self.connections.entry(key).or_default();

    if sockets.len() >= self.max_idle_per_host {
      return;
    }

    sockets.push(PooledSocket {
      socket,
      last_used: Self::current_time(),
    });
  }

  fn current_time() -> Duration {
    #[cfg(windows)]
    {
      use core::mem::MaybeUninit;
      unsafe {
        let mut filetime = MaybeUninit::<windows_sys::Win32::Foundation::FILETIME>::uninit();
        windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime(filetime.as_mut_ptr());
        let ft = filetime.assume_init();
        let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
        let nanos = ticks.saturating_mul(100);
        Duration::from_nanos(nanos)
      }
    }
    #[cfg(unix)]
    {
      unsafe {
        let mut ts_uninit = core::mem::MaybeUninit::<libc::timespec>::uninit();
        libc::clock_gettime(libc::CLOCK_MONOTONIC, ts_uninit.as_mut_ptr());
        let ts = ts_uninit.assume_init();
        Duration::from_secs(ts.tv_sec.cast_unsigned()).saturating_add(Duration::from_nanos(ts.tv_nsec.cast_unsigned()))
      }
    }
    #[cfg(not(any(windows, unix)))]
    {
      Duration::from_secs(0)
    }
  }
}

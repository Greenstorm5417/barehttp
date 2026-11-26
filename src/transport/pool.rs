use crate::socket::BlockingSocket;
use crate::sync::Mutex;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
use core::time::Duration;
use hashbrown::{Equivalent, HashMap};

/// Owned idle-pool key: scheme class, lowercased host, port.
///
/// Allocated once per distinct host when its first connection is pooled;
/// per-request lookups use the borrowed [`PoolKeyRef`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolKey {
  https: bool,
  host: Box<str>,
  port: u16,
}

/// Borrowed pool key for lookups (no allocation; host compared case-insensitively).
#[derive(Debug, Clone, Copy)]
pub struct PoolKeyRef<'a> {
  https: bool,
  host: &'a str,
  port: u16,
}

impl<'a> PoolKeyRef<'a> {
  /// Key for `scheme://host:port` (host case-insensitive).
  #[must_use]
  pub const fn new(
    scheme: &str,
    host: &'a str,
    port: u16,
  ) -> Self {
    Self {
      https: scheme.eq_ignore_ascii_case("https"),
      host,
      port,
    }
  }

  fn to_owned_key(self) -> PoolKey {
    PoolKey {
      https: self.https,
      host: self.host.to_ascii_lowercase().into_boxed_str(),
      port: self.port,
    }
  }
}

impl PoolKey {
  /// Host is lowercased for case-insensitive pooling.
  #[cfg(test)]
  #[must_use]
  pub fn new(
    scheme: &str,
    host: &str,
    port: u16,
  ) -> Self {
    PoolKeyRef::new(scheme, host, port).to_owned_key()
  }
}

/// Shared hash for owned and borrowed keys (ASCII-lowercased host bytes).
fn hash_key<H: Hasher>(
  https: bool,
  host: &str,
  port: u16,
  state: &mut H,
) {
  state.write_u8(u8::from(https));
  let mut chunk = [0u8; 32];
  for part in host.as_bytes().chunks(32) {
    let dst = chunk.get_mut(..part.len()).unwrap_or(&mut []);
    for (d, &s) in dst.iter_mut().zip(part) {
      *d = s.to_ascii_lowercase();
    }
    state.write(dst);
  }
  state.write_usize(host.len());
  state.write_u16(port);
}

impl Hash for PoolKey {
  fn hash<H: Hasher>(
    &self,
    state: &mut H,
  ) {
    hash_key(self.https, &self.host, self.port, state);
  }
}

impl Hash for PoolKeyRef<'_> {
  fn hash<H: Hasher>(
    &self,
    state: &mut H,
  ) {
    hash_key(self.https, self.host, self.port, state);
  }
}

impl Equivalent<PoolKey> for PoolKeyRef<'_> {
  fn equivalent(
    &self,
    key: &PoolKey,
  ) -> bool {
    self.https == key.https && self.port == key.port && self.host.eq_ignore_ascii_case(&key.host)
  }
}

struct IdleConn<S> {
  socket: S,
  /// Unix seconds when returned to the pool.
  idle_since: u64,
}

pub struct ConnectionPool<S> {
  connections: Mutex<HashMap<PoolKey, Vec<IdleConn<S>>>>,
  max_idle_per_host: usize,
  max_idle_age: Duration,
}

impl<S: BlockingSocket> ConnectionPool<S> {
  #[must_use]
  pub fn new(
    max_idle_per_host: usize,
    max_idle_age: Duration,
  ) -> Self {
    Self {
      connections: Mutex::new(HashMap::new()),
      max_idle_per_host,
      max_idle_age,
    }
  }

  pub fn get(
    &self,
    key: &PoolKeyRef<'_>,
  ) -> Option<S> {
    let mut connections = self.connections.lock();
    let sockets = connections.get_mut(key)?;
    let now = crate::util::now_unix_secs();
    let max_age = self.max_idle_age.as_secs();
    while let Some(idle) = sockets.pop() {
      if now.saturating_sub(idle.idle_since) <= max_age {
        return Some(idle.socket);
      }
      // too old — drop
    }
    None
  }

  pub fn return_connection(
    &self,
    key: &PoolKeyRef<'_>,
    socket: S,
  ) {
    if self.max_idle_per_host == 0 {
      return;
    }
    let idle = IdleConn {
      socket,
      idle_since: crate::util::now_unix_secs(),
    };
    let mut connections = self.connections.lock();
    if let Some(sockets) = connections.get_mut(key) {
      if sockets.len() < self.max_idle_per_host {
        sockets.push(idle);
      }
      return;
    }
    connections.insert(key.to_owned_key(), alloc::vec![idle]);
  }
}

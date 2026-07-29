use crate::socket::BlockingSocket;
use crate::sync::Mutex;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PoolKey {
  scheme: String,
  host: String,
  port: u16,
}

impl PoolKey {
  /// Host is lowercased for case-insensitive pooling.
  #[must_use]
  pub fn new(
    scheme: String,
    host: &str,
    port: u16,
  ) -> Self {
    Self {
      scheme,
      host: host.to_ascii_lowercase(),
      port,
    }
  }
}

struct IdleConn<S> {
  socket: S,
  /// Unix seconds when returned to the pool.
  idle_since: u64,
}

pub struct ConnectionPool<S> {
  connections: Mutex<BTreeMap<PoolKey, Vec<IdleConn<S>>>>,
  max_idle_per_host: usize,
  max_idle_age: Duration,
}

impl<S: BlockingSocket> ConnectionPool<S> {
  #[must_use]
  pub const fn new(
    max_idle_per_host: usize,
    max_idle_age: Duration,
  ) -> Self {
    Self {
      connections: Mutex::new(BTreeMap::new()),
      max_idle_per_host,
      max_idle_age,
    }
  }

  pub fn get(
    &self,
    key: &PoolKey,
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
    key: PoolKey,
    socket: S,
  ) {
    let mut connections = self.connections.lock();
    let sockets = connections.entry(key).or_default();
    if sockets.len() >= self.max_idle_per_host {
      return;
    }
    sockets.push(IdleConn {
      socket,
      idle_since: crate::util::now_unix_secs(),
    });
  }
}

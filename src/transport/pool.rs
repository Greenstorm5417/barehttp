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
  pub const fn new(host: String, port: u16) -> Self {
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
  pub const fn new(max_idle_per_host: usize, idle_timeout: Option<Duration>) -> Self {
    Self {
      connections: BTreeMap::new(),
      max_idle_per_host,
      idle_timeout,
    }
  }

  pub fn get(&mut self, key: &PoolKey) -> Option<S> {
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

  pub fn return_connection(&mut self, key: PoolKey, socket: S) {
    let sockets = self.connections.entry(key).or_default();

    if sockets.len() >= self.max_idle_per_host {
      return;
    }

    sockets.push(PooledSocket {
      socket,
      last_used: Self::current_time(),
    });
  }

  const fn current_time() -> Duration {
    Duration::from_secs(0)
  }
}

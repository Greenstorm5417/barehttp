use crate::socket::BlockingSocket;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PoolKey {
  scheme: String,
  host: String,
  port: u16,
}

impl PoolKey {
  pub const fn new(
    scheme: String,
    host: String,
    port: u16,
  ) -> Self {
    Self { scheme, host, port }
  }
}

pub struct ConnectionPool<S> {
  connections: Mutex<BTreeMap<PoolKey, Vec<S>>>,
  max_idle_per_host: usize,
}

impl<S: BlockingSocket> ConnectionPool<S> {
  pub const fn new(max_idle_per_host: usize) -> Self {
    Self {
      connections: Mutex::new(BTreeMap::new()),
      max_idle_per_host,
    }
  }

  pub fn get(
    &self,
    key: &PoolKey,
  ) -> Option<S> {
    let mut connections = self.connections.lock();
    connections.get_mut(key)?.pop()
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
    sockets.push(socket);
  }
}

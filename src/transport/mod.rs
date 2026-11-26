pub mod connection;
pub mod pool;

pub use connection::RawResponse;
#[cfg(test)]
pub use pool::PoolKey;
pub use pool::{ConnectionPool, PoolKeyRef};

#[cfg(test)]
mod tests;

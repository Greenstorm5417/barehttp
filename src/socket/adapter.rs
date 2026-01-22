use crate::error::SocketError;
use crate::socket::SocketFlags;
use crate::util::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketAddr<'a> {
  Hostname { host: &'a [u8], port: u16 },
  Ip { addr: IpAddr, port: u16 },
}

pub trait BlockingSocket {
  fn connect(&mut self, addr: &SocketAddr<'_>) -> Result<(), SocketError>;
  fn read(&mut self, buf: &mut [u8]) -> Result<usize, SocketError>;
  fn write(&mut self, buf: &[u8]) -> Result<usize, SocketError>;
  fn shutdown(&mut self) -> Result<(), SocketError>;
  fn set_flags(&mut self, flags: SocketFlags) -> Result<(), SocketError>;
  fn set_read_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError>;
  fn set_write_timeout(&mut self, timeout_ms: u32) -> Result<(), SocketError>;
}

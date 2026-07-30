//! Deterministic scripted socket for transport fault-injection tests.
//!
//! Read/write behavior is driven by ordered step queues (no sleeps). Empty read
//! queue yields EOF (`Ok(0)`). Empty write queue accepts the full buffer
//! (`AcceptAll`). I/O call counts are tracked; exceeding `max_io_calls` panics
//! so hung loops surface immediately.

use crate::error::SocketError;
use crate::socket::{BlockingSocket, BlockingSocketFactory, SocketAddr};
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// One scripted outcome for a `read` call (or a multi-call `Data` step).
#[derive(Debug, Clone)]
pub enum ReadStep {
  /// Deliver these bytes across one or more reads: each call copies
  /// `min(buf.len(), remaining)` and advances; the step is popped when exhausted.
  Data(Vec<u8>),
  /// Return `Ok(0)` once (peer EOF / closed), then pop.
  Eof,
  /// First read returns `Ok(0)`; later reads deliver `data` as [`Data`].
  /// Covers mid-stream zero-byte cases before more payload.
  ZeroThenData(Vec<u8>),
  /// Return `Err(Interrupted)` once, then pop. Next call uses the next step.
  Interrupted,
  /// Return `Err(TimedOut)` once, then pop.
  TimedOut,
  /// Return the given error once, then pop.
  Error(SocketError),
}

/// One scripted outcome for a `write` call.
#[derive(Debug, Clone)]
pub enum WriteStep {
  /// Accept up to `n` bytes of this write (short write), then pop.
  Accept(usize),
  /// Accept the entire buffer of this write call, then pop.
  AcceptAll,
  /// Return `Ok(0)` once (maps to `NotConnected` in `Connection::send_request`).
  Zero,
  /// Return `Err(Interrupted)` once, then pop. `Connection` propagates it.
  Interrupted,
  /// Return the given error once, then pop.
  Error(SocketError),
}

/// Scripted [`BlockingSocket`] with ordered read/write step queues.
pub struct ScriptedSocket {
  reads: VecDeque<ReadStep>,
  writes: VecDeque<WriteStep>,
  written: Vec<u8>,
  pub read_calls: usize,
  pub write_calls: usize,
  /// Panic if `read_calls + write_calls` exceeds this (default `10_000`).
  pub max_io_calls: usize,
  pub connected_addr: Option<String>,
  pub connected_host: Option<String>,
  pub read_timeout: Option<u32>,
  pub write_timeout: Option<u32>,
  pub connect_timeout: Option<u32>,
  pub should_fail_connect: bool,
  pub connect_error: SocketError,
}

impl ScriptedSocket {
  pub fn new() -> Self {
    Self {
      reads: VecDeque::new(),
      writes: VecDeque::new(),
      written: Vec::new(),
      read_calls: 0,
      write_calls: 0,
      max_io_calls: 10_000,
      connected_addr: None,
      connected_host: None,
      read_timeout: None,
      write_timeout: None,
      connect_timeout: None,
      should_fail_connect: false,
      connect_error: SocketError::ConnectionRefused,
    }
  }

  pub fn push_read(
    &mut self,
    step: ReadStep,
  ) -> &mut Self {
    self.reads.push_back(step);
    self
  }

  pub fn push_write(
    &mut self,
    step: WriteStep,
  ) -> &mut Self {
    self.writes.push_back(step);
    self
  }

  pub fn push_writes(
    &mut self,
    steps: impl IntoIterator<Item = WriteStep>,
  ) -> &mut Self {
    self.writes.extend(steps);
    self
  }

  pub fn with_max_io_calls(
    mut self,
    max: usize,
  ) -> Self {
    self.max_io_calls = max;
    self
  }

  pub fn get_written(&self) -> &[u8] {
    &self.written
  }

  pub fn written_len(&self) -> usize {
    self.written.len()
  }

  fn bump_io(&self) {
    let total = self.read_calls.saturating_add(self.write_calls);
    assert!(
      total <= self.max_io_calls,
      "ScriptedSocket: exceeded max_io_calls ({}); possible infinite I/O loop (reads={}, writes={})",
      self.max_io_calls,
      self.read_calls,
      self.write_calls
    );
  }
}

impl Default for ScriptedSocket {
  fn default() -> Self {
    Self::new()
  }
}

impl BlockingSocket for ScriptedSocket {
  fn connect(
    &mut self,
    addr: &SocketAddr,
    host: &str,
  ) -> Result<(), SocketError> {
    if self.should_fail_connect {
      return Err(self.connect_error);
    }
    self.connected_addr = Some(format!("{addr}"));
    self.connected_host = Some(String::from(host));
    Ok(())
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    self.read_calls = self.read_calls.saturating_add(1);
    self.bump_io();

    loop {
      let Some(step) = self.reads.front_mut() else {
        // No more scripted data → EOF.
        return Ok(0);
      };

      match step {
        ReadStep::Data(data) => {
          if data.is_empty() {
            let _ = self.reads.pop_front();
            continue;
          }
          let n = data.len().min(buf.len());
          if let (Some(dst), Some(src)) = (buf.get_mut(..n), data.get(..n)) {
            dst.copy_from_slice(src);
          }
          data.drain(..n);
          if data.is_empty() {
            let _ = self.reads.pop_front();
          }
          return Ok(n);
        },
        ReadStep::Eof => {
          let _ = self.reads.pop_front();
          return Ok(0);
        },
        ReadStep::ZeroThenData(data) => {
          let pending = core::mem::take(data);
          let _ = self.reads.pop_front();
          if !pending.is_empty() {
            self.reads.push_front(ReadStep::Data(pending));
          }
          return Ok(0);
        },
        ReadStep::Interrupted => {
          let _ = self.reads.pop_front();
          return Err(SocketError::Interrupted);
        },
        ReadStep::TimedOut => {
          let _ = self.reads.pop_front();
          return Err(SocketError::TimedOut);
        },
        ReadStep::Error(err) => {
          let err = *err;
          let _ = self.reads.pop_front();
          return Err(err);
        },
      }
    }
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    self.write_calls = self.write_calls.saturating_add(1);
    self.bump_io();

    let Some(step) = self.writes.pop_front() else {
      // Unscripted writes succeed in full (AcceptAll).
      self.written.extend_from_slice(buf);
      return Ok(buf.len());
    };

    match step {
      WriteStep::Accept(n) => {
        let to_write = buf.len().min(n);
        if let Some(slice) = buf.get(..to_write) {
          self.written.extend_from_slice(slice);
        }
        Ok(to_write)
      },
      WriteStep::AcceptAll => {
        self.written.extend_from_slice(buf);
        Ok(buf.len())
      },
      WriteStep::Zero => Ok(0),
      WriteStep::Interrupted => Err(SocketError::Interrupted),
      WriteStep::Error(err) => Err(err),
    }
  }

  fn write_vectored(
    &mut self,
    bufs: &[&[u8]],
  ) -> Result<usize, SocketError> {
    let total: usize = bufs.iter().map(|b| b.len()).sum();
    if total == 0 {
      return Ok(0);
    }
    let mut flat = Vec::with_capacity(total);
    for buf in bufs {
      flat.extend_from_slice(buf);
    }
    self.write(flat.as_slice())
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    Ok(())
  }

  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.read_timeout = Some(timeout_ms);
    Ok(())
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.write_timeout = Some(timeout_ms);
    Ok(())
  }

  fn set_connect_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.connect_timeout = Some(timeout_ms);
    Ok(())
  }
}

impl BlockingSocketFactory for ScriptedSocket {
  fn new() -> Result<Self, SocketError> {
    Ok(Self::new())
  }
}

/// Retries `Interrupted` the way the Unix OS socket adapters do.
///
/// Wrap a [`ScriptedSocket`] when a test needs the Interrupted-then-success path
/// that OS adapters absorb as EINTR before `Connection` sees the I/O result.
pub struct RetryInterrupted<S> {
  inner: S,
  pub interrupted_retries: usize,
}

impl<S> RetryInterrupted<S> {
  pub const fn new(inner: S) -> Self {
    Self {
      inner,
      interrupted_retries: 0,
    }
  }

  pub const fn inner(&self) -> &S {
    &self.inner
  }
}

impl<S: BlockingSocket> BlockingSocket for RetryInterrupted<S> {
  fn connect(
    &mut self,
    addr: &SocketAddr,
    host: &str,
  ) -> Result<(), SocketError> {
    self.inner.connect(addr, host)
  }

  fn read(
    &mut self,
    buf: &mut [u8],
  ) -> Result<usize, SocketError> {
    loop {
      match self.inner.read(buf) {
        Err(SocketError::Interrupted) => {
          self.interrupted_retries = self.interrupted_retries.saturating_add(1);
        },
        other => return other,
      }
    }
  }

  fn write(
    &mut self,
    buf: &[u8],
  ) -> Result<usize, SocketError> {
    loop {
      match self.inner.write(buf) {
        Err(SocketError::Interrupted) => {
          self.interrupted_retries = self.interrupted_retries.saturating_add(1);
        },
        other => return other,
      }
    }
  }

  fn write_vectored(
    &mut self,
    bufs: &[&[u8]],
  ) -> Result<usize, SocketError> {
    loop {
      match self.inner.write_vectored(bufs) {
        Err(SocketError::Interrupted) => {
          self.interrupted_retries = self.interrupted_retries.saturating_add(1);
        },
        other => return other,
      }
    }
  }

  fn shutdown(&mut self) -> Result<(), SocketError> {
    self.inner.shutdown()
  }

  fn set_read_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_read_timeout(timeout_ms)
  }

  fn set_write_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_write_timeout(timeout_ms)
  }

  fn set_connect_timeout(
    &mut self,
    timeout_ms: u32,
  ) -> Result<(), SocketError> {
    self.inner.set_connect_timeout(timeout_ms)
  }
}

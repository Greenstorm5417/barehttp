//! Busy-wait mutex for `no_std` + `alloc` shared state.
//!
//! # ponytail
//! Spins until free. For cookie-jar and pool critical sections that stay short.
//! Does not park threads or poison; acquisition is unfair.

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Blocking spin lock over `T`.
pub struct Mutex<T: ?Sized> {
  locked: AtomicBool,
  data: UnsafeCell<T>,
}

/// RAII guard; unlocks on drop.
pub struct MutexGuard<'a, T: ?Sized> {
  lock: &'a Mutex<T>,
}

// SAFETY: exclusive mutable access is gated by `locked`; `T: Send` moves across threads.
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
// SAFETY: `lock()` serializes access; only `T: Send` values are shared.
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

// SAFETY: guard is not `Sync` (holds exclusive borrow); `Send` if `T` is.
unsafe impl<T: ?Sized + Send> Send for MutexGuard<'_, T> {}

impl<T> Mutex<T> {
  /// Create a mutex around `data`.
  #[must_use]
  pub const fn new(data: T) -> Self {
    Self {
      locked: AtomicBool::new(false),
      data: UnsafeCell::new(data),
    }
  }
}

impl<T: ?Sized> Mutex<T> {
  /// Acquire the lock, spinning until free.
  #[inline]
  pub fn lock(&self) -> MutexGuard<'_, T> {
    while self
      .locked
      .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
      .is_err()
    {
      core::hint::spin_loop();
    }
    MutexGuard { lock: self }
  }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
  #[inline]
  fn drop(&mut self) {
    self.lock.locked.store(false, Ordering::Release);
  }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
  type Target = T;

  #[inline]
  fn deref(&self) -> &T {
    // SAFETY: we hold the lock; no other guard exists.
    unsafe { &*self.lock.data.get() }
  }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
  #[inline]
  fn deref_mut(&mut self) -> &mut T {
    // SAFETY: we hold the lock; no other guard exists.
    unsafe { &mut *self.lock.data.get() }
  }
}

impl<T: ?Sized> fmt::Debug for Mutex<T> {
  fn fmt(
    &self,
    f: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    // Do not lock (avoids deadlock if Debug runs while held).
    f.debug_struct("Mutex").finish_non_exhaustive()
  }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::expect_used, clippy::shadow_reuse)]
  extern crate std;

  use super::Mutex;
  use alloc::vec::Vec;

  #[test]
  fn lock_unlock_roundtrip() {
    let m = Mutex::new(0_u32);
    {
      let mut g = m.lock();
      *g = 7;
    }
    assert_eq!(*m.lock(), 7);
  }

  #[test]
  fn nested_critical_sections_sequential() {
    let m = Mutex::new(Vec::<u8>::new());
    {
      let mut g = m.lock();
      g.push(1);
      g.push(2);
    }
    {
      let mut g = m.lock();
      g.push(3);
    }
    assert_eq!(m.lock().as_slice(), &[1, 2, 3]);
  }

  #[test]
  fn guard_drop_releases() {
    let m = Mutex::new(0_u32);
    let g = m.lock();
    drop(g);
    // Would hang forever if unlock failed.
    let _ = m.lock();
  }

  #[test]
  fn concurrent_increments() {
    // Miri + threads: verifies UnsafeCell mutex under the interpreter.
    use std::sync::Arc;
    use std::thread;

    let m = Arc::new(Mutex::new(0_u32));
    let mut handles = Vec::new();
    for _ in 0..4 {
      let lock = Arc::clone(&m);
      handles.push(thread::spawn(move || {
        for _ in 0..50 {
          let mut g = lock.lock();
          *g = g.wrapping_add(1);
        }
      }));
    }
    for h in handles {
      assert!(h.join().is_ok());
    }
    assert_eq!(*m.lock(), 200);
  }

  #[test]
  fn debug_does_not_lock() {
    let m = Mutex::new(1_u32);
    let _g = m.lock();
    // Debug must not try to lock (would deadlock if it did).
    let _ = alloc::format!("{m:?}");
  }
}

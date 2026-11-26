use crate::transport::{PoolKey, PoolKeyRef};
use core::hash::{BuildHasher, Hash};
extern crate alloc;
use hashbrown::Equivalent;

#[test]
fn pool_key_scheme_distinguishes_same_host_port() {
  let http = PoolKey::new("http", "example.com", 443);
  let https = PoolKey::new("https", "example.com", 443);
  assert_ne!(http, https);
}

#[test]
fn pool_key_lowercases_host() {
  let mixed = PoolKey::new("http", "Example.COM", 80);
  let lower = PoolKey::new("http", "example.com", 80);
  assert_eq!(mixed, lower);
}

fn hash_of<T: Hash>(
  builder: &hashbrown::DefaultHashBuilder,
  k: &T,
) -> u64 {
  builder.hash_one(k)
}

#[test]
fn borrowed_key_hashes_and_matches_owned() {
  let owned = PoolKey::new("http", "example.com", 80);
  let probe = PoolKeyRef::new("HTTP", "EXAMPLE.com", 80);
  assert!(probe.equivalent(&owned));
  // One builder: each `default()` may pick a fresh random seed.
  let b = hashbrown::DefaultHashBuilder::default();
  assert_eq!(hash_of(&b, &owned), hash_of(&b, &probe));
  assert!(!PoolKeyRef::new("https", "example.com", 80).equivalent(&owned));
  assert!(!PoolKeyRef::new("http", "example.com", 81).equivalent(&owned));
  // Long hosts hash across several lowercase chunks.
  let long = "A".repeat(70);
  assert_eq!(
    hash_of(&b, &PoolKey::new("http", &long, 80)),
    hash_of(&b, &PoolKeyRef::new("http", &long, 80))
  );
}

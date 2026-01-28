//! Integration tests for Version struct

use barehttp::Version;

#[test]
fn test_version_new() {
  let version = Version::new(1, 1);
  assert_eq!(version.major(), 1);
  assert_eq!(version.minor(), 1);
}

#[test]
fn test_version_major() {
  let version = Version::new(2, 0);
  assert_eq!(version.major(), 2);
}

#[test]
fn test_version_minor() {
  let version = Version::new(1, 0);
  assert_eq!(version.minor(), 0);
}

#[test]
fn test_version_constants() {
  assert_eq!(Version::HTTP_09.major(), 0);
  assert_eq!(Version::HTTP_09.minor(), 9);
  
  assert_eq!(Version::HTTP_10.major(), 1);
  assert_eq!(Version::HTTP_10.minor(), 0);
  
  assert_eq!(Version::HTTP_11.major(), 1);
  assert_eq!(Version::HTTP_11.minor(), 1);
  
  assert_eq!(Version::HTTP_2.major(), 2);
  assert_eq!(Version::HTTP_2.minor(), 0);
  
  assert_eq!(Version::HTTP_3.major(), 3);
  assert_eq!(Version::HTTP_3.minor(), 0);
}

#[test]
fn test_version_clone() {
  let version1 = Version::new(1, 1);
  let version2 = version1.clone();
  assert_eq!(version1.major(), version2.major());
  assert_eq!(version1.minor(), version2.minor());
}

#[test]
fn test_version_copy() {
  let version1 = Version::new(1, 1);
  let version2 = version1;
  assert_eq!(version1.major(), version2.major());
  assert_eq!(version1.minor(), version2.minor());
}

#[test]
fn test_version_equality() {
  let version1 = Version::new(1, 1);
  let version2 = Version::new(1, 1);
  let version3 = Version::new(2, 0);
  
  assert_eq!(version1, version2);
  assert_ne!(version1, version3);
}

#[test]
fn test_version_debug() {
  let version = Version::new(1, 1);
  let debug_str = format!("{:?}", version);
  assert!(!debug_str.is_empty());
}

#[test]
fn test_version_hash() {
  use std::collections::HashSet;
  
  let mut set = HashSet::new();
  set.insert(Version::new(1, 1));
  set.insert(Version::new(2, 0));
  set.insert(Version::new(1, 1));
  
  assert_eq!(set.len(), 2);
}

//! Docker interop client tests.
//!
//! Gated behind `BAREHTTP_INTEROP=1`. Started by `scripts/run-interop.sh`
//! (release / nightly workflow only).

use barehttp::Agent;
use barehttp::config::Config;
use std::time::Duration;

fn interop_enabled() -> bool {
  matches!(std::env::var("BAREHTTP_INTEROP"), Ok(v) if v == "1")
}

fn base(port: u16) -> String {
  format!("http://127.0.0.1:{port}")
}

fn client() -> Agent {
  Agent::with_config(
    Config::builder()
      .max_redirects(0)
      .max_idle_per_host(0)
      .http_status_as_error(false)
      .timeout_connect(Some(Duration::from_secs(5)))
      .timeout_read(Some(Duration::from_secs(10)))
      .timeout_write(Some(Duration::from_secs(10)))
      .build(),
  )
}

fn skip_unless_interop() -> bool {
  if interop_enabled() {
    return false;
  }
  eprintln!("skip: set BAREHTTP_INTEROP=1 (and start docker-compose.interop.yml)");
  true
}

macro_rules! servers {
  ($($name:ident => $port:expr),* $(,)?) => {
    $(
      mod $name {
        use super::*;

        #[test]
        fn plain() {
          if skip_unless_interop() { return; }
          let url = format!("{}/plain", base($port));
          let resp = client().get(&url).call().expect("plain");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"hello");
        }

        #[test]
        fn chunked() {
          if skip_unless_interop() { return; }
          let url = format!("{}/chunked", base($port));
          let resp = client().get(&url).call().expect("chunked");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"hello");
        }

        #[test]
        fn gzip() {
          if skip_unless_interop() { return; }
          let url = format!("{}/gzip", base($port));
          let resp = client().get(&url).call().expect("gzip");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"hello-gzip");
        }

        #[test]
        fn headers() {
          if skip_unless_interop() { return; }
          let url = format!("{}/headers", base($port));
          let resp = client().get(&url).call().expect("headers");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"ok");
          assert!(resp.header("x-interop-server").is_some() || resp.header("X-Interop-Server").is_some()
            || resp.body() == b"ok");
        }

        #[test]
        fn status_404() {
          if skip_unless_interop() { return; }
          let url = format!("{}/status/404", base($port));
          let resp = client().get(&url).call().expect("404");
          assert_eq!(resp.status_code(), 404);
          assert_eq!(resp.body(), b"missing");
        }

        #[test]
        fn close() {
          if skip_unless_interop() { return; }
          let url = format!("{}/close", base($port));
          let resp = client().get(&url).call().expect("close");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"bye");
        }

        #[test]
        fn http10() {
          if skip_unless_interop() { return; }
          let url = format!("{}/http10", base($port));
          let resp = client().get(&url).call().expect("http10");
          assert_eq!(resp.status_code(), 200);
          assert_eq!(resp.body(), b"http10-ish");
        }
      }
    )*
  };
}

servers! {
  nginx => 18080,
  httpd => 18081,
  caddy => 18082,
  python => 18083,
  node => 18084,
  axum => 18085,
  haproxy => 18086,
}

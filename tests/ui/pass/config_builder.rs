//! Config builder constructs a Config.
use barehttp::config::Config;

fn main() {
  let _ = Config::builder()
    .user_agent("ui-test/0.1")
    .max_redirects(0)
    .build();
}

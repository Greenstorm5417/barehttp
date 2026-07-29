//! Cookie jar (`--features cookie-jar`) against a real cleartext cookie endpoint.

fn main() {
  // Prefer httpbingo (cleartext Set-Cookie + redirect). Fall back if flaky.
  let hosts = ["httpbingo.org", "postman-echo.com"];
  let mut last_err: Option<String> = None;

  for host in hosts {
    match run_against(host) {
      Ok(()) => return,
      Err(msg) => {
        eprintln!("cookies example: {host} failed: {msg}");
        last_err = Some(msg);
      }
    }
  }

  eprintln!(
    "cookies example: all hosts failed ({})",
    last_err.unwrap_or_else(|| "unknown".into())
  );
  std::process::exit(1);
}

fn run_against(host: &str) -> Result<(), String> {
  let agent = barehttp::agent();

  let set_url = format!("http://{host}/cookies/set?session=abc123");
  let get_url = format!("http://{host}/cookies");

  let set_resp = agent
    .get(&set_url)
    .call()
    .map_err(|e| format!("SET request: {e}"))?;
  let set_status = set_resp.status();
  // After following the 302, we should land on /cookies with 200.
  println!("after set+redirect: {set_status}");
  if set_status != 200 {
    let body = set_resp.text().unwrap_or_default();
    return Err(format!("expected 200 after set, got {set_status}: {body}"));
  }

  let cookie_hdr = agent.cookie_store().get_request_cookies(&get_url, false);
  println!("stored Cookie header: {cookie_hdr}");
  if !cookie_hdr.contains("session=abc123") {
    return Err(format!("jar missing session cookie, got: {cookie_hdr:?}"));
  }

  let get_resp = agent
    .get(&get_url)
    .call()
    .map_err(|e| format!("GET cookies: {e}"))?;
  let body = get_resp.text().map_err(|e| format!("body: {e}"))?;
  println!("get cookies: {} {}", get_resp.status(), body);

  if !body.contains("abc123") && !body.contains("session") {
    return Err(format!("response body did not echo cookie: {body}"));
  }

  Ok(())
}

use crate::transport::connection_state::ConnectionState;

#[test]
fn test_new_connection() {
  let state = ConnectionState::new();
  assert!(state.can_reuse);
  assert!(!state.sent_close);
  assert!(!state.received_close);
}

#[test]
fn test_sent_close_prevents_reuse() {
  let mut state = ConnectionState::new();
  state.mark_sent_close();
  assert!(!state.can_reuse);
  assert!(state.sent_close);
}

#[test]
fn test_received_close_prevents_reuse() {
  let mut state = ConnectionState::new();
  state.mark_received_close();
  assert!(!state.can_reuse);
  assert!(state.received_close);
}

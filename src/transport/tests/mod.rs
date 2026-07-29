#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::shadow_reuse)]
#![allow(clippy::shadow_same)]
mod mock_socket;
mod scripted_socket;
mod test_connection;
mod test_connector;
mod test_fault_injection;
mod test_fragmentation;
mod test_limits;
mod test_partial_writes;
mod test_pool;

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::shadow_reuse)]
#![allow(clippy::shadow_same)]

mod chunked_encoding;
mod field_syntax;
mod framing;
mod incomplete_messages;
mod message_body;
mod message_parsing;
mod response_reading;
mod security;
mod status_line;
mod uri_parsing;

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::shadow_reuse)]
#![allow(clippy::shadow_same)]

mod chunked_encoding;
#[cfg(feature = "cookie-jar")]
mod cookie;
mod framing;
mod message_body;
mod properties;
mod rfc9112;
mod security;
mod status_line;
mod uri_parsing;

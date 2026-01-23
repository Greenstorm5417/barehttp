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
mod incomplete_messages;
mod message_body;
mod message_parsing;
mod response_reading;
mod rfc9112_compliance_validation;
mod rfc9112_missing_requirements;
mod rfc9112_must_requirements;
mod rfc9112_phase1_phase2;
mod rfc9112_phase3_phase4;
mod security;
mod status_line;
mod uri_parsing;

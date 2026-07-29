use axum::body::Body;
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

async fn plain() -> impl IntoResponse {
  ([(header::CONTENT_TYPE, "text/plain")], "hello")
}

async fn chunked() -> impl IntoResponse {
  // Short body; hyper may length-frame. Client asserts decoded body == "hello".
  ([(header::CONTENT_TYPE, "text/plain")], "hello")
}

async fn gzip() -> Response<Body> {
  let mut enc = GzEncoder::new(Vec::new(), Compression::default());
  enc.write_all(b"hello-gzip").expect("gzip write");
  let compressed = enc.finish().expect("gzip finish");
  Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "text/plain")
    .header(header::CONTENT_ENCODING, "gzip")
    .body(Body::from(compressed))
    .expect("response")
}

async fn headers() -> impl IntoResponse {
  (
    [
      (header::CONTENT_TYPE, HeaderValue::from_static("text/plain")),
      (
        header::HeaderName::from_static("x-interop-server"),
        HeaderValue::from_static("axum"),
      ),
    ],
    "ok",
  )
}

async fn status_404() -> impl IntoResponse {
  (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], "missing")
}

async fn close() -> impl IntoResponse {
  (
    [
      (header::CONTENT_TYPE, HeaderValue::from_static("text/plain")),
      (header::CONNECTION, HeaderValue::from_static("close")),
    ],
    "bye",
  )
}

async fn http10() -> impl IntoResponse {
  ([(header::CONTENT_TYPE, "text/plain")], "http10-ish")
}

#[tokio::main]
async fn main() {
  let app = Router::new()
    .route("/plain", get(plain))
    .route("/chunked", get(chunked))
    .route("/gzip", get(gzip))
    .route("/headers", get(headers))
    .route("/status/404", get(status_404))
    .route("/close", get(close))
    .route("/http10", get(http10));

  let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
    .await
    .expect("bind");
  axum::serve(listener, app).await.expect("serve");
}

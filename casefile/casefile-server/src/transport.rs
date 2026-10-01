use crate::api::Host;
use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    runtime::{Builder, Runtime},
};
use tokio_io_timeout::TimeoutStream;

pub(crate) fn runtime() -> Result<Runtime> {
    Ok(Builder::new_multi_thread().enable_all().build()?)
}

pub(crate) async fn serve(listener: TcpListener, host: Arc<Host>) -> Result<()> {
    loop {
        let (socket, _) = listener.accept().await?;
        let host = Arc::clone(&host);
        tokio::spawn(async move {
            let mut stream = TimeoutStream::new(socket);
            stream.set_read_timeout(Some(Duration::from_secs(30)));
            stream.set_write_timeout(Some(Duration::from_secs(30)));
            let service = service_fn(move |request| respond(Arc::clone(&host), request));
            if let Err(error) = http1::Builder::new()
                .serve_connection(TokioIo::new(Box::pin(stream)), service)
                .await
            {
                eprintln!("HTTP connection failed: {error}");
            }
        });
    }
}

async fn respond(
    host: Arc<Host>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = request.into_parts();
    let response = match body.collect().await {
        Ok(body) => {
            let bytes = body.to_bytes();
            // The admitted job owns the request and Provider even if the connection drops.
            match tokio::task::spawn_blocking(move || host.handle(parts, bytes)).await {
                Ok(response) => response,
                Err(error) => failure(500, &error.to_string()),
            }
        }
        Err(error) => failure(400, &error.to_string()),
    };
    Ok(response)
}

fn failure(status: u16, message: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(
            serde_json::json!({"error":message}).to_string(),
        )))
        .expect("valid response")
}

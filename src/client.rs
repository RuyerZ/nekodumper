use anyhow::{anyhow, Result};
use flate2::read::{DeflateDecoder, GzDecoder};
use hyper::{
    body::{aggregate, to_bytes, Buf, Bytes},
    client::{Client, HttpConnector},
    Body, Request, Uri,
};
use hyper_tls::HttpsConnector;
use once_cell::sync::Lazy;
use std::io::Read;

static CLIENT: Lazy<Client<HttpsConnector<HttpConnector>>> = Lazy::new(|| {
    let https = HttpsConnector::new();
    Client::builder()
        .set_host(false)
        .http1_title_case_headers(true)
        .build(https)
});

///Popular Xiaomi 6 UA
static USER_AGENT: &str = "Dalvik/2.1.0 (Linux; U; Android 8.0.0; MI 6 MIUI/8.6.28)";

fn request(uri: Uri) -> Result<Request<Body>> {
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("Not a valid URL:{}", uri))?;
    Ok(Request::builder()
        .method("GET")
        .uri(&uri)
        .header("User-Agent", USER_AGENT)
        .header("Accept-Encoding", "gzip, deflate")
        .header("Host", host)
        .body(Body::empty())?)
}

pub async fn httpget_unlimited(uri: Uri) -> Result<Bytes> {
    let req = request(uri)?;
    let resp = CLIENT.request(req).await?;
    match resp.headers().get("content-encoding") {
        Some(coding) => match coding.as_bytes() {
            b"gzip" => {
                let reader = aggregate(resp.into_body()).await?.reader();
                let mut gz = GzDecoder::new(reader);
                let mut buf = Vec::new();
                gz.read_to_end(&mut buf)?;
                Ok(Bytes::from(buf))
            }
            b"deflate" => {
                let reader = aggregate(resp.into_body()).await?.reader();
                let mut df = DeflateDecoder::new(reader);
                let mut buf = Vec::new();
                df.read_to_end(&mut buf)?;
                Ok(Bytes::from(buf))
            }
            _ => Err(anyhow!(
                "Encoding not supported: {}",
                String::from_utf8_lossy(coding.as_bytes())
            )),
        },
        None => Ok(to_bytes(resp.into_body()).await?),
    }
}

static LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);

pub async fn httpget(url: Uri) -> Result<Bytes> {
    let _permit = LIMIT.acquire().await.unwrap();
    httpget_unlimited(url).await
}

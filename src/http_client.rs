// src/http_client.rs
use hyper::{Client, Body, Request, Method, Uri};
use hyper::body::HttpBody;
use serde::Serialize;
use serde::de::DeserializeOwned;
use anyhow::{Result, Context};
use std::str::FromStr;

// Import the specific client type from proxy.rs or define it generically
// We'll use a generic wrapper to handle both standard and Tor clients if needed,
// but for now, let's focus on the Hyper client structure.

#[derive(Clone)]
pub struct HyruleClient {
    // We store the inner Hyper client. We use a dynamic dispatch approach or specific type
    // depending on how strict we want to be. For simplicity in your current setup:
    inner: Client<arti_hyper::ArtiHttpConnector<tor_rtcompat::tokio::TokioNativeTlsRuntime, tls_api_native_tls::TlsConnector>, Body>,
}

impl HyruleClient {
    pub fn new(inner: Client<arti_hyper::ArtiHttpConnector<tor_rtcompat::tokio::TokioNativeTlsRuntime, tls_api_native_tls::TlsConnector>, Body>) -> Self {
        Self { inner }
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        RequestBuilder::new(self.inner.clone(), Method::GET, url.to_string())
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        RequestBuilder::new(self.inner.clone(), Method::POST, url.to_string())
    }
}

pub struct RequestBuilder {
    client: Client<arti_hyper::ArtiHttpConnector<tor_rtcompat::tokio::TokioNativeTlsRuntime, tls_api_native_tls::TlsConnector>, Body>,
    method: Method,
    url: String,
    body: Body,
    headers: hyper::HeaderMap,
    timeout: Option<std::time::Duration>,
}

impl RequestBuilder {
    pub fn new(client: Client<arti_hyper::ArtiHttpConnector<tor_rtcompat::tokio::TokioNativeTlsRuntime, tls_api_native_tls::TlsConnector>, Body>, method: Method, url: String) -> Self {
        Self {
            client,
            method,
            url,
            body: Body::empty(),
            headers: hyper::HeaderMap::new(),
            timeout: None,
        }
    }

    pub fn json<T: Serialize>(mut self, json: &T) -> Self {
        let bytes = serde_json::to_vec(json).expect("Failed to serialize JSON");
        self.body = Body::from(bytes);
        self.headers.insert(hyper::header::CONTENT_TYPE, "application/json".parse().unwrap());
        self
    }
    
    pub fn timeout(mut self, duration: std::time::Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub async fn send(self) -> Result<HyruleResponse> {
        let uri = Uri::from_str(&self.url).context("Invalid URL")?;
        
        let mut builder = Request::builder()
            .method(self.method)
            .uri(uri);
            
        for (key, value) in self.headers.iter() {
            builder = builder.header(key, value);
        }

        let req = builder.body(self.body).context("Failed to build request")?;

        // Handle timeout if set
        let resp = if let Some(duration) = self.timeout {
             match tokio::time::timeout(duration, self.client.request(req)).await {
                 Ok(res) => res?,
                 Err(_) => anyhow::bail!("Request timed out"),
             }
        } else {
            self.client.request(req).await?
        };

        Ok(HyruleResponse { inner: resp })
    }
}

pub struct HyruleResponse {
    inner: hyper::Response<Body>,
}

impl HyruleResponse {
    pub fn status(&self) -> hyper::StatusCode {
        self.inner.status()
    }

    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        let bytes = hyper::body::to_bytes(self.inner.into_body()).await?;
        let obj = serde_json::from_slice(&bytes)?;
        Ok(obj)
    }
    
    // Helper to get text for errors/debugging
    pub async fn text(self) -> Result<String> {
        let bytes = hyper::body::to_bytes(self.inner.into_body()).await?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}

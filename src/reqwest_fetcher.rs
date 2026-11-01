use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::ACCEPT;

use crate::client::{FetchError, Fetcher};

#[derive(Debug, Clone)]
pub struct ReqwestFetcherBuilder {
    user_agent: String,
    timeout: Duration,
}

impl Default for ReqwestFetcherBuilder {
    fn default() -> Self {
        Self {
            user_agent: format!("clampey/{}", env!("CARGO_PKG_VERSION")),
            timeout: Duration::from_secs(30),
        }
    }
}

impl ReqwestFetcherBuilder {
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> std::result::Result<ReqwestFetcher, FetchError> {
        let client = reqwest::Client::builder()
            .user_agent(self.user_agent)
            .timeout(self.timeout)
            .gzip(true)
            .build()
            .map_err(|error| FetchError::transport(error.to_string()))?;
        Ok(ReqwestFetcher { client })
    }
}

pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl std::fmt::Debug for ReqwestFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReqwestFetcher").finish_non_exhaustive()
    }
}

impl ReqwestFetcher {
    pub fn new() -> std::result::Result<Self, FetchError> {
        Self::builder().build()
    }

    pub fn builder() -> ReqwestFetcherBuilder {
        ReqwestFetcherBuilder::default()
    }
}

#[async_trait]
impl Fetcher for ReqwestFetcher {
    async fn get(&self, url: &str) -> std::result::Result<String, FetchError> {
        let response = self
            .client
            .get(url)
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| FetchError::transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::status(status.as_u16()));
        }
        response
            .text()
            .await
            .map_err(|error| FetchError::transport(error.to_string()))
    }
}

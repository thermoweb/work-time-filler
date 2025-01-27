use log::debug;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Debug;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T> {
    #[allow(dead_code)]
    pub expand: Option<String>,
    pub start_at: usize,
    pub max_results: usize,
    pub total: usize,
    pub is_last: Option<bool>,
    #[serde(flatten)]
    pub items: HashMap<String, Vec<T>>,
}

#[derive(Debug)]
pub struct PaginatedFetcher<'a, T> {
    client: &'a Client,
    base_url: String,
    auth_header: String,
    endpoint: String,
    query_fn: fn(usize) -> String,
    start_at: usize,
    total_items: usize,
    current_items: Option<Vec<T>>,
    is_last: bool,
}

impl<'a, T> PaginatedFetcher<'a, T>
where
    T: DeserializeOwned + Debug,
{
    pub fn new(
        client: &'a Client,
        base_url: String,
        auth_header: String,
        endpoint: String,
        query_fn: fn(usize) -> String,
    ) -> Self {
        let mut fetcher = PaginatedFetcher {
            client,
            base_url,
            auth_header,
            endpoint,
            query_fn,
            start_at: 0,
            total_items: 0,
            current_items: None,
            is_last: false,
        };
        futures::executor::block_on(fetcher.fetch_page()).unwrap_or_else(|err| {
            eprintln!("Failed to fetch paginated items: {}", err);
        });

        fetcher
    }

    pub async fn initialize(
        client: &'a Client,
        base_url: String,
        auth_header: String,
        endpoint: String,
        query_fn: fn(usize) -> String,
    ) -> Result<Self, reqwest::Error> {
        let mut fetcher = PaginatedFetcher {
            client,
            base_url,
            auth_header,
            endpoint,
            query_fn,
            start_at: 0,
            total_items: 0,
            current_items: None,
            is_last: false,
        };
        debug!("paginated fetcher created.");
        fetcher.fetch_page().await?;
        debug!("paginated fetcher initialized.");

        Ok(fetcher)
    }

    pub async fn fetch_page(&mut self) -> Result<(), reqwest::Error> {
        let query = (self.query_fn)(self.start_at);
        let url = format!("{}/{}{}", self.base_url, self.endpoint, query);
        debug!("Fetching page {}", url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header.clone())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            eprintln!("HTTP {} error fetching {}: {}", status, url, body.trim());
            self.current_items = Some(vec![]);
            self.is_last = true;
            return Ok(());
        }

        let text = response.text().await?;
        debug!("Response body: {:?}", text);

        let page: PaginatedResponse<T> = match serde_json::from_str::<PaginatedResponse<T>>(&text) {
            Ok(page) => page,
            Err(err) => {
                eprintln!(
                    "Failed to deserialize paginated response: {:?}\nBody: {}",
                    err,
                    if text.len() > 500 { &text[..500] } else { &text }
                );
                // Treat as empty last page — let the caller handle "no results" gracefully
                self.current_items = Some(vec![]);
                self.is_last = true;
                return Ok(());
            }
        };

        self.current_items = Some(page.items.into_values().flatten().collect());
        self.total_items = page.total;
        self.is_last = page.is_last.unwrap_or_else(|| {
            debug!("`is_last` not provided here...");
            if page.start_at + page.max_results > page.total {
                debug!("Current page is the last one based on `start_at` and `total`");
                true
            } else {
                debug!("More pages likely exists");
                false
            }
        });

        self.start_at += page.max_results;

        Ok(())
    }
}

impl<'a, T> Iterator for PaginatedFetcher<'a, T>
where
    T: DeserializeOwned + Debug,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        // debug!("PaginatedFetcher::next : {:?}", self);
        if self.current_items.is_none() {
            let fetch_result = futures::executor::block_on(self.fetch_page());
            if let Err(_) = fetch_result {
                return None;
            }
        }

        if let Some(items) = &mut self.current_items {
            if let Some(item) = items.pop() {
                return Some(item);
            } else if !self.is_last {
                let fetch_result = futures::executor::block_on(self.fetch_page());
                if let Err(_) = fetch_result {
                    return None;
                }
                return self.next();
            }
        }

        None
    }
}

impl<'a, T> ExactSizeIterator for PaginatedFetcher<'a, T>
where
    T: DeserializeOwned + Debug,
{
    fn len(&self) -> usize {
        self.total_items
    }
}

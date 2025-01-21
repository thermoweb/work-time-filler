use crate::models::jira::JiraError;
use indicatif::ProgressBar;
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

impl<T> PaginatedResponse<T>
where
    T: DeserializeOwned + Clone + Send + 'static,
{
    // TODO: remove all usages of this and use the PaginatedFetcher instead
    pub async fn fetch_all_items<F>(
        base_url: &str,
        client: &Client,
        auth_header: &str,
        endpoint: &str,
        query_fn: F,
        external_progress_bar: Option<&ProgressBar>,
    ) -> Result<Vec<T>, JiraError>
    where
        F: Fn(usize) -> String,
    {
        let mut start_at = 0;
        let mut all_items = Vec::new();

        let mut page =
            fetch_page(client, base_url, endpoint, &query_fn, auth_header, start_at).await?;
        let total_items = page.total;

        if let Some(bar) = external_progress_bar {
            bar.set_length(total_items as u64);
            bar.inc(page.max_results as u64);
        }

        all_items.extend(page.items.into_values().flatten());
        start_at += page.max_results;

        while start_at < total_items && !page.is_last.unwrap_or(false) {
            page = fetch_page(client, base_url, endpoint, &query_fn, auth_header, start_at).await?;
            all_items.extend(page.items.into_values().flatten());

            start_at += page.max_results;

            if let Some(bar) = external_progress_bar {
                bar.inc(page.max_results as u64);
            }
        }

        Ok(all_items)
    }
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
        futures::executor::block_on(fetcher.fetch_page()).unwrap();

        fetcher
    }

    pub async fn fetch_page(&mut self) -> Result<(), reqwest::Error> {
        let query = (self.query_fn)(self.start_at);
        let url = format!("{}/{}{}", self.base_url, self.endpoint, query);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header.clone())
            .send()
            .await?;

        let page: PaginatedResponse<T> = response.json().await?;

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

pub async fn fetch_page<T, F>(
    client: &Client,
    base_url: &str,
    endpoint: &str,
    query_fn: F,
    auth_header: &str,
    start_at: usize,
) -> Result<PaginatedResponse<T>, JiraError>
where
    T: DeserializeOwned,
    F: Fn(usize) -> String,
{
    let url = format!("{}{}{}", base_url, endpoint, query_fn(start_at));
    debug!("Fetching page: {}", url);

    let response = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
        .map_err(JiraError::RequestError)?;

    if !response.status().is_success() {
        return Err(JiraError::ApiError(response.status().to_string()));
    }

    response
        .json::<PaginatedResponse<T>>()
        .await
        .map_err(|e| JiraError::DeserializeError(e.to_string()))
}

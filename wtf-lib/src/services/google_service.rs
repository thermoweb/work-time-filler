use crate::config::{expand_path, Config};
use google_calendar3::hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use google_calendar3::hyper_util::client::legacy::connect::HttpConnector;
use google_calendar3::hyper_util::client::legacy::Client;
use google_calendar3::yup_oauth2::authenticator_delegate::InstalledFlowDelegate;
use google_calendar3::yup_oauth2::{
    ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
};
use google_calendar3::{hyper_util, CalendarHub};
use log::{debug, error, info};
use std::error::Error;
use std::fmt;
use std::fs;
use std::future::Future;
use std::pin::Pin;

// Custom delegate that logs OAuth messages instead of printing to stdout
#[derive(Clone)]
struct TuiInstalledFlowDelegate;

impl InstalledFlowDelegate for TuiInstalledFlowDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            info!("🌐 Opening browser for authentication: {}", url);
            
            // Automatically open URL in default browser
            if let Err(e) = open::that(url) {
                error!("Failed to open browser automatically: {}. Please open the URL manually.", e);
            }
            
            // Return empty string - the HTTPRedirect method handles the callback
            Ok(String::new())
        })
    }
}

#[derive(Debug)]
pub enum GoogleServiceError {
    CredentialsNotFound(String),
    CredentialsInvalid(String),
    AuthenticationFailed(String),
    SslInitFailed(String),
    ConfigError(String),
}

impl fmt::Display for GoogleServiceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GoogleServiceError::CredentialsNotFound(path) => {
                write!(f, "Google credentials file not found at: {}\nPlease download OAuth 2.0 credentials from Google Cloud Console", path)
            }
            GoogleServiceError::CredentialsInvalid(msg) => {
                write!(
                    f,
                    "Invalid Google credentials file: {}\nPlease check the JSON format",
                    msg
                )
            }
            GoogleServiceError::AuthenticationFailed(msg) => {
                write!(f, "Google authentication failed: {}", msg)
            }
            GoogleServiceError::SslInitFailed(msg) => {
                write!(f, "SSL initialization failed: {}", msg)
            }
            GoogleServiceError::ConfigError(msg) => {
                write!(
                    f,
                    "Google configuration error: {}\nPlease add [google] section to config.toml",
                    msg
                )
            }
        }
    }
}

impl Error for GoogleServiceError {}

pub struct GoogleService;

impl GoogleService {
    pub async fn get_hub() -> Result<CalendarHub<HttpsConnector<HttpConnector>>, GoogleServiceError>
    {
        // Load configuration
        let config = Config::load().map_err(|e| {
            GoogleServiceError::ConfigError(format!("Failed to load config: {}", e))
        })?;

        let google_config = config.google.ok_or_else(|| {
            GoogleServiceError::ConfigError("No [google] section found in config".to_string())
        })?;

        debug!(
            "Loading Google credentials from: {}",
            google_config.credentials_path
        );

        // Check if credentials file exists
        let creds_path = expand_path(&google_config.credentials_path);
        if !creds_path.exists() {
            return Err(GoogleServiceError::CredentialsNotFound(
                google_config.credentials_path.clone(),
            ));
        }

        // Read and parse credentials
        let creds_file = fs::File::open(creds_path).map_err(|e| {
            error!("Failed to open credentials file: {}", e);
            GoogleServiceError::CredentialsNotFound(google_config.credentials_path.clone())
        })?;

        let secret: ApplicationSecret = serde_json::from_reader(creds_file).map_err(|e| {
            error!("Failed to parse credentials JSON: {}", e);
            GoogleServiceError::CredentialsInvalid(e.to_string())
        })?;

        debug!("Using token cache at: {}", google_config.token_cache_path);

        // Build authenticator with custom delegate
        let expanded_token_path = expand_path(&google_config.token_cache_path);
        let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .persist_tokens_to_disk(expanded_token_path)
                .flow_delegate(Box::new(TuiInstalledFlowDelegate))
                .build()
                .await
                .map_err(|e| {
                    error!("OAuth authentication failed: {}", e);
                    GoogleServiceError::AuthenticationFailed(e.to_string())
                })?;

        // Build HTTPS connector
        let https_connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| {
                error!("Failed to initialize SSL with native roots: {}", e);
                GoogleServiceError::SslInitFailed(e.to_string())
            })?
            .https_or_http()
            .enable_http2()
            .build();

        let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(https_connector);

        let hub = CalendarHub::new(client, auth);
        debug!("Google Calendar Hub initialized successfully");
        Ok(hub)
    }
}

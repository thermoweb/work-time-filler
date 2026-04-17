use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use config::{Config as ConfigLoader, File};
use log::debug;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, fmt, fs};

/// Google Calendar event color names, indexed by (colorId - 1).
/// colorId "1" → "Lavender", "2" → "Sage", ..., "11" → "Tomato"
pub const GOOGLE_CALENDAR_EVENT_COLORS: [&str; 11] = [
    "Lavender",
    "Sage",
    "Grape",
    "Flamingo",
    "Banana",
    "Tangerine",
    "Peacock",
    "Graphite",
    "Blueberry",
    "Basil",
    "Tomato",
];

fn get_config_path() -> PathBuf {
    if let Ok(custom_path) = env::var("WTF_CONFIG_HOME") {
        PathBuf::from(custom_path).join("config.toml")
    } else {
        expand_tilde("~/.config/wtf/config.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub jira: JiraConfig,
    pub github: GithubConfig,
    #[serde(default)]
    pub google: Option<GoogleConfig>,
    #[serde(default)]
    pub worklog: WorklogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraConfig {
    pub base_url: String,
    pub username: String,
    pub api_token: SensitiveString,
    #[serde(default)]
    pub auto_follow_sprint_pattern: Option<String>,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            username: String::new(),
            api_token: SensitiveString::new(String::new()),
            auto_follow_sprint_pattern: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubConfig {
    #[serde(default)]
    pub organisation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleConfig {
    pub credentials_path: String,
    pub token_cache_path: String,
    /// Maps Google Calendar color name (e.g. "Grape") to a Jira issue key (e.g. "INTERNAL-42").
    /// Used as highest-priority auto-link source in meetings.
    #[serde(default)]
    pub color_labels: HashMap<String, String>,
}

impl Default for GoogleConfig {
    fn default() -> Self {
        Self {
            credentials_path: String::new(),
            token_cache_path: String::new(),
            color_labels: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorklogConfig {
    #[serde(default = "default_daily_hours_limit")]
    pub daily_hours_limit: f64,
}

impl Default for WorklogConfig {
    fn default() -> Self {
        Self {
            daily_hours_limit: 8.0,
        }
    }
}

fn default_daily_hours_limit() -> f64 {
    8.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jira: JiraConfig::default(),
            github: GithubConfig { organisation: None },
            google: None,
            worklog: WorklogConfig::default(),
        }
    }
}

impl Config {
    /// Returns true if essential Jira credentials have been configured.
    pub fn is_configured(&self) -> bool {
        !self.jira.base_url.is_empty() && !self.jira.username.is_empty()
    }

    pub fn load() -> Result<Self, Box<dyn Error>> {
        let config_path = get_config_path();
        debug!("config path: {:?}", config_path);
        let builder = ConfigLoader::builder().add_source(File::from(config_path).required(false));
        let config = builder.build()?.try_deserialize()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let config_path = get_config_path();
        let config_dir = config_path.parent().unwrap();
        fs::create_dir_all(config_dir)?;

        let toml = toml::to_string(self)?;
        fs::write(config_path, toml)?;

        Ok(())
    }
}

pub struct SensitiveString(String);

impl Clone for SensitiveString {
    fn clone(&self) -> Self {
        SensitiveString(self.0.clone())
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[HIDDEN]")
    }
}

impl fmt::Display for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[HIDDEN]")
    }
}

impl FromStr for SensitiveString {
    type Err = std::string::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(decoded) = Self::decode_str(s) {
            return Ok(decoded);
        }
        Ok(SensitiveString(s.to_string()))
    }
}

impl SensitiveString {
    pub fn new(s: String) -> Self {
        SensitiveString(s)
    }

    pub fn reveal(&self) -> &str {
        &self.0
    }

    pub fn encode(&self) -> String {
        format!("enc[{}]", URL_SAFE.encode(&self.0))
    }

    pub fn decode(&self) -> Result<SensitiveString, Box<dyn Error>> {
        Self::decode_str(&self.0)
    }

    pub fn decode_str(s: &str) -> Result<SensitiveString, Box<dyn Error>> {
        let re = Regex::new(r"enc\[([A-Za-z0-9\-_=]+)]")?;
        if let Some(caps) = re.captures(s) {
            if let Some(base64_str) = caps.get(1) {
                let decoded_bytes = URL_SAFE.decode(base64_str.as_str())?;
                let decoded_str = String::from_utf8_lossy(&decoded_bytes);
                return Ok(SensitiveString(decoded_str.to_string()));
            }
        }
        Err(base64::DecodeError::InvalidByte(0, 0).into())
    }
}

impl<'de> Deserialize<'de> for SensitiveString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SensitiveString::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for SensitiveString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip_simple() {
        let original = "my_api_token_12345";
        let s = SensitiveString::new(original.to_string());
        let encoded = s.encode();
        assert!(encoded.starts_with("enc["), "encoded form should start with enc[");
        let decoded = SensitiveString::decode_str(&encoded).expect("decode should succeed");
        assert_eq!(decoded.reveal(), original);
    }

    #[test]
    fn test_encode_decode_roundtrip_with_dash_in_base64() {
        // These strings are known to produce `-` or `=` in URL-safe base64 output,
        // which the old regex `\w+` would silently truncate.
        let cases = [
            "abc",          // base64: YWJj (no special chars, baseline)
            "abcd",         // base64: YWJjZA== (has =)
            // "~" encodes to fn4= in standard base64, but in URL_SAFE it stays fn4=
            // We use a string that encodes with a dash: the bytes [0xfb] produce -
            // In Rust we express this as a known token value that does produce a dash.
            "some long api token that may produce dashes or equals in base64",
            "ATATT3xFfGF0_YFake-Token-That-Looks-Real",
        ];
        for original in &cases {
            let s = SensitiveString::new(original.to_string());
            let encoded = s.encode();
            let decoded = SensitiveString::decode_str(&encoded)
                .unwrap_or_else(|_| panic!("decode failed for: {:?}  encoded as: {}", original, encoded));
            assert_eq!(decoded.reveal(), *original, "roundtrip failed for: {:?}", original);
        }
    }

    #[test]
    fn test_encode_produces_dash_and_roundtrips() {
        // Build a value whose UTF-8 bytes produce a `-` in URL-safe base64.
        // 0xFB in the first byte position maps to `-` in the output.
        // We create a String from raw bytes via from_utf8_lossy to avoid compile-time escaping issues.
        let raw = String::from_utf8_lossy(&[0xfb, 0xff, 0x00]).to_string();
        let s = SensitiveString::new(raw.clone());
        let encoded = s.encode();
        assert!(encoded.contains('-') || encoded.contains('='),
            "expected URL-safe base64 chars in: {}", encoded);
        let decoded = SensitiveString::decode_str(&encoded)
            .unwrap_or_else(|_| panic!("decode failed for encoded: {}", encoded));
        assert_eq!(decoded.reveal(), raw);
    }

    #[test]
    fn test_decode_str_returns_err_for_plain_string() {
        // A plain (non-encoded) string should fail to decode
        let result = SensitiveString::decode_str("plain_token_value");
        assert!(result.is_err(), "plain string should not decode");
    }

    #[test]
    fn test_from_str_falls_back_to_plain_for_non_encoded() {
        // FromStr should store the raw value when it's not an enc[...] token
        let s: SensitiveString = "plain_token".parse().unwrap();
        assert_eq!(s.reveal(), "plain_token");
    }

    #[test]
    fn test_display_hides_value() {
        let s = SensitiveString::new("super_secret".to_string());
        assert_eq!(format!("{}", s), "[HIDDEN]");
    }

    #[test]
    fn test_debug_hides_value() {
        let s = SensitiveString::new("super_secret".to_string());
        assert!(format!("{:?}", s).contains("[HIDDEN]"));
    }
}

pub fn expand_path(path: &str) -> PathBuf {
    let expanded = shellexpand::full(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    PathBuf::from(expanded.as_ref())
}

fn expand_tilde(path: &str) -> PathBuf {
    expand_path(path)
}

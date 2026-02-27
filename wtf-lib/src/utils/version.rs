/// Checks GitHub releases for a newer version than the one currently running.
/// Returns `Some(tag)` if a newer version is available, `None` otherwise (including on error).
pub async fn check_latest_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("wtf/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp: serde_json::Value = client
        .get("https://api.github.com/repos/thermoweb/work-time-filler/releases/latest")
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let tag = resp.get("tag_name")?.as_str()?;
    // Strip leading 'v' for comparison
    let latest = tag.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");

    if is_newer(latest, current) {
        Some(tag.to_string())
    } else {
        None
    }
}

/// Simple semver comparison: returns true if `candidate` is newer than `current`.
/// Handles formats like "1.2.3" and "1.2.3-beta.0".
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let core = s.split('-').next().unwrap_or(s);
        let mut parts = core.splitn(3, '.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };
    parse(candidate) > parse(current)
}

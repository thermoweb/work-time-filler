/// Fetches all available release tags from GitHub.
/// If `include_prerelease` is false, only stable releases are returned.
pub async fn list_versions(include_prerelease: bool) -> Vec<String> {
    let client = match build_client() {
        Some(c) => c,
        None => return vec![],
    };

    let response = client
        .get("https://api.github.com/repos/thermoweb/work-time-filler/releases")
        .send()
        .await;

    let releases: serde_json::Value = match response.ok().map(|r| async move { r.json().await }) {
        Some(fut) => match fut.await.ok() {
            Some(v) => v,
            None => return vec![],
        },
        None => return vec![],
    };

    releases
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|r| {
            include_prerelease
                || !r
                    .get("prerelease")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
        })
        .filter_map(|r| r.get("tag_name")?.as_str().map(str::to_string))
        .collect()
}

/// Checks GitHub releases for a newer version than the one currently running.
/// Returns `Some(tag)` if a newer stable release is available, `None` otherwise (including on error).
pub async fn check_latest_version() -> Option<String> {
    check_version_from_url(
        "https://api.github.com/repos/thermoweb/work-time-filler/releases/latest",
    )
    .await
}

/// Like `check_latest_version` but also considers pre-release versions.
/// Returns `Some(tag)` if a newer release (stable or pre-release) is available.
pub async fn check_latest_prerelease_version() -> Option<String> {
    let client = build_client()?;

    let releases: serde_json::Value = client
        .get("https://api.github.com/repos/thermoweb/work-time-filler/releases")
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    // Releases are sorted newest-first; take the first one
    let tag = releases.get(0)?.get("tag_name")?.as_str()?;
    let latest = tag.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");

    if is_newer(latest, current) {
        Some(tag.to_string())
    } else {
        None
    }
}

fn build_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("wtf/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()
}

async fn check_version_from_url(url: &str) -> Option<String> {
    let resp: serde_json::Value = build_client()?
        .get(url)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let tag = resp.get("tag_name")?.as_str()?;
    let latest = tag.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");

    if is_newer(latest, current) {
        Some(tag.to_string())
    } else {
        None
    }
}

/// Returns true if `version` is strictly older than `threshold`.
/// Treats empty string as "0.0.0".
pub fn is_older_than(version: &str, threshold: &str) -> bool {
    let v = if version.is_empty() { "0.0.0" } else { version };
    is_newer(threshold, v)
}

/// Simple semver comparison: returns true if `candidate` is newer than `current`.
/// Handles formats like "1.2.3" and "1.2.3-beta.4".
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> ((u64, u64, u64), Option<(String, u64)>) {
        let (core, pre) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(&s[idx + 1..]))
        } else {
            (s, None)
        };
        let mut parts = core.splitn(3, '.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        // Parse pre-release: "beta.4" → ("beta", 4)
        let pre_parsed = pre.and_then(|p| {
            let mut it = p.rsplitn(2, '.');
            let num: u64 = it.next()?.parse().ok()?;
            let label = it.next().unwrap_or("").to_string();
            Some((label, num))
        });
        ((major, minor, patch), pre_parsed)
    };

    let (cv, cp) = parse(candidate);
    let (rv, rp) = parse(current);

    if cv != rv {
        return cv > rv;
    }
    // Same core version: stable > pre-release (semver spec)
    match (cp, rp) {
        (None, None) => false,    // identical
        (None, Some(_)) => true,  // candidate is stable, current is pre-release
        (Some(_), None) => false, // candidate is pre-release, current is stable
        (Some((cl, cn)), Some((rl, rn))) => {
            if cl != rl {
                cl > rl
            } else {
                cn > rn
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_older_than() {
        // Empty string treated as 0.0.0
        assert!(is_older_than("", "0.0.1"));
        assert!(is_older_than("", "0.1.0-beta.3"));
        assert!(is_older_than("", "1.0.0"));

        // Pre-release ordering within same core version
        assert!(is_older_than("0.1.0-beta.2", "0.1.0-beta.3"));
        assert!(!is_older_than("0.1.0-beta.3", "0.1.0-beta.3")); // equal
        assert!(!is_older_than("0.1.0-beta.4", "0.1.0-beta.3"));

        // Pre-release < stable (semver spec)
        assert!(is_older_than("1.0.0-beta.1", "1.0.0"));
        assert!(!is_older_than("1.0.0", "1.0.0-beta.1"));

        // Major/minor/patch ordering
        assert!(is_older_than("0.9.9", "1.0.0"));
        assert!(is_older_than("1.0.0", "1.0.1"));
        assert!(is_older_than("1.0.0", "1.1.0"));
        assert!(is_older_than("1.0.0", "2.0.0"));
        assert!(!is_older_than("1.0.0", "1.0.0")); // equal
        assert!(!is_older_than("2.0.0", "1.9.9"));
        assert!(!is_older_than("1.1.0", "1.0.9"));

        // Cross pre-release and version jumps
        assert!(is_older_than("0.1.0-beta.3", "1.0.0"));
        assert!(is_older_than("0.1.0-beta.3", "0.2.0"));
        assert!(!is_older_than("1.0.0", "0.9.9-beta.99"));
    }
}

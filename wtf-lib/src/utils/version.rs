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
        (None, None) => false,         // identical
        (None, Some(_)) => true,       // candidate is stable, current is pre-release
        (Some(_), None) => false,      // candidate is pre-release, current is stable
        (Some((cl, cn)), Some((rl, rn))) => {
            if cl != rl { cl > rl } else { cn > rn }
        }
    }
}

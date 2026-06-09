/// Fetch an HTTP/HTTPS URL, returning `(final_url, body)`.
/// Tries HTTPS first if the caller passes a bare domain; the caller is
/// responsible for scheme normalisation — see `resolve_url`.
pub fn fetch_url(url: &str) -> Result<(String, String), String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Forkit/0.1 (Rust browser)")
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    // Capture the final URL after redirects (ureq exposes it via the response)
    let final_url = resp.get_url().to_owned();
    let body = resp.into_string()
        .map_err(|e| format!("read body failed: {e}"))?;

    Ok((final_url, body))
}

/// Normalise a user-typed or href-extracted URL:
/// - bare `example.com/path`  → `https://example.com/path`
/// - `/absolute/path`         → `https://{base_origin}/absolute/path`
/// - `relative/path`          → `https://{base_origin}/{base_dir}/relative/path`
/// - `//example.com/path`     → `https://example.com/path`
/// - already has scheme       → returned unchanged
pub fn resolve_url(input: &str, base: &str) -> String {
    let input = input.trim();

    // Already fully qualified
    if input.starts_with("http://") || input.starts_with("https://")
        || input.starts_with("file://")
    {
        return input.to_owned();
    }

    // Protocol-relative  //example.com/…
    if input.starts_with("//") {
        return format!("https:{}", input);
    }

    // Fragment-only or javascript: — return base unchanged
    if input.starts_with('#') || input.starts_with("javascript:") || input.is_empty() {
        return base.to_owned();
    }

    let origin = url_origin(base);

    // Absolute path on same origin
    if input.starts_with('/') {
        return format!("{}{}", origin, input);
    }

    // Relative path — resolve against base directory
    let base_dir = base_directory(base);
    format!("{}/{}", base_dir.trim_end_matches('/'), input)
}

/// Try HTTPS first; if that fails, fall back to HTTP.
/// Returns `(final_url, body)` or an error string.
pub fn fetch_with_auto_https(url: &str) -> Result<(String, String), String> {
    // If already has a scheme, try as-is then maybe flip to https
    let upgraded = if url.starts_with("http://") {
        url.replacen("http://", "https://", 1)
    } else {
        url.to_owned()
    };

    // Try the (possibly upgraded) URL first
    match fetch_url(&upgraded) {
        Ok(r) => return Ok(r),
        Err(e) => {
            // Only fall back to HTTP if we actually tried to upgrade
            if upgraded != url {
                eprintln!("HTTPS failed ({}), retrying over HTTP…", e);
                fetch_url(url)
            } else {
                Err(e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Extract `scheme://host` from a URL.
fn url_origin(url: &str) -> &str {
    // Find end of "scheme://host[:port]"
    if let Some(after_scheme) = url.find("://") {
        let rest = &url[after_scheme + 3..];
        let path_start = rest.find('/').unwrap_or(rest.len());
        &url[..after_scheme + 3 + path_start]
    } else {
        url
    }
}

/// Extract `scheme://host/path/to/` (directory part) from a URL.
fn base_directory(url: &str) -> &str {
    if let Some(q) = url.find('?') {
        // Drop query string before looking for last slash
        let without_q = &url[..q];
        if let Some(last_slash) = without_q.rfind('/') {
            return &url[..last_slash];
        }
    }
    if let Some(last_slash) = url.rfind('/') {
        // Don't strip past the "://"
        if last_slash > url.find("://").map(|i| i + 3).unwrap_or(0) {
            return &url[..last_slash];
        }
    }
    url
}

//! HTTPS clients stay on the hosts this app meant to call.

pub fn api_redirect() -> reqwest::redirect::Policy {
    allow_https(&["api.x.ai", "api.openai.com"])
}

pub fn pack_redirect() -> reqwest::redirect::Policy {
    allow_https(&[
        "github.com",
        "*.githubusercontent.com",
        "huggingface.co",
        "*.huggingface.co",
        "*.hf.co",
    ])
}

pub fn github_api_redirect() -> reqwest::redirect::Policy {
    allow_https(&["api.github.com"])
}

pub fn search_redirect() -> reqwest::redirect::Policy {
    allow_https(&[
        "api.duckduckgo.com",
        "duckduckgo.com",
        "html.duckduckgo.com",
        "*.duckduckgo.com",
    ])
}

pub fn https_url_allowed(url: &str, patterns: &[&str]) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    url.host_str()
        .is_some_and(|host| host_allowed(host, patterns))
}

fn allow_https(patterns: &'static [&'static str]) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 3 || !https_url_allowed(attempt.url().as_str(), patterns) {
            return attempt.error(std::io::Error::new(
                std::io::ErrorKind::Other,
                "redirect host is not allowed",
            ));
        }
        attempt.follow()
    })
}

fn host_allowed(host: &str, patterns: &[&str]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    patterns.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else {
            host == *pattern
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_hosts_stay_on_the_publisher_cdns() {
        assert!(https_url_allowed(
            "https://release-assets.githubusercontent.com/asset",
            &["github.com", "*.githubusercontent.com"]
        ));
        assert!(https_url_allowed(
            "https://us.aws.cdn.hf.co/file",
            &["*.hf.co"]
        ));
        assert!(!https_url_allowed(
            "http://github.com/file",
            &["github.com"]
        ));
        assert!(!https_url_allowed(
            "https://user:pw@github.com/file",
            &["github.com"]
        ));
        assert!(!https_url_allowed(
            "https://github.com.evil.test/file",
            &["github.com", "*.githubusercontent.com"]
        ));
        assert!(!https_url_allowed(
            "https://evil.example/file",
            &["github.com"]
        ));
    }

    #[test]
    fn release_checks_stay_on_the_github_api() {
        assert!(https_url_allowed(
            "https://api.github.com/repos/Gabo-Tech/grok-voice-android/releases/latest",
            &["api.github.com"]
        ));
        assert!(!https_url_allowed(
            "https://github.com/Gabo-Tech/grok-voice-android/releases/latest",
            &["api.github.com"]
        ));
        assert!(!https_url_allowed(
            "https://api.github.com.evil.test/repos",
            &["api.github.com"]
        ));
    }
}

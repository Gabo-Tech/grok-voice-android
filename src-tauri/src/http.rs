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

pub fn rates_redirect() -> reqwest::redirect::Policy {
    allow_https(&["api.frankfurter.app", "api.coingecko.com"])
}

/// A bring-your-own model link. HTTPS only, no embedded password, and no local or private address.
pub fn custom_download_url(url: &str) -> Result<(), String> {
    if url.len() > 2048 {
        return Err("That download address is not allowed.".into());
    }
    let Ok(url) = reqwest::Url::parse(url) else {
        return Err("The download address must be https.".into());
    };
    if url.scheme() != "https" {
        return Err("The download address must be https.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("That download address is not allowed.".into());
    }
    let Some(host) = url.host_str() else {
        return Err("That download address is not allowed.".into());
    };
    let host = host
        .trim_matches(|c| c == '[' || c == ']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() || host_is_local(&host) {
        return Err("That download address is not allowed.".into());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if !ip_is_public(ip) {
            return Err("That download address is not allowed.".into());
        }
    }
    Ok(())
}

pub fn ip_is_public(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => v4_is_public(ip),
        std::net::IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return v4_is_public(v4);
            }
            v6_is_public(ip)
        }
    }
}

fn v4_is_public(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    let cgnat = a == 100 && (b & 0b1100_0000) == 64;
    let this_net = a == 0;
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_broadcast()
        && !ip.is_documentation()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && !cgnat
        && !this_net
}

fn v6_is_public(ip: std::net::Ipv6Addr) -> bool {
    let [first, second, _, _, _, _, _, _, _, _, _, _, _, _, _, _] = ip.octets();
    let unique_local = first & 0xfe == 0xfc;
    let link_local = first == 0xfe && (second & 0xc0) == 0x80;
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast() && !unique_local && !link_local
}

fn host_is_local(host: &str) -> bool {
    host == "localhost"
        || host == "localhost.localdomain"
        || host == "metadata.google.internal"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
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

    #[test]
    fn custom_model_links_stay_on_public_https() {
        assert!(
            custom_download_url("https://huggingface.co/org/model/resolve/main/model.gguf").is_ok()
        );
        assert!(custom_download_url("https://example.com/my-model.gguf").is_ok());
        assert!(
            custom_download_url("http://example.com/model.gguf")
                .unwrap_err()
                .contains("https")
        );
        assert!(custom_download_url("https://user:pw@example.com/model.gguf").is_err());
        assert!(custom_download_url("https://127.0.0.1/model.gguf").is_err());
        assert!(custom_download_url("https://10.1.2.3/model.gguf").is_err());
        assert!(custom_download_url("https://192.168.1.9/model.gguf").is_err());
        assert!(custom_download_url("https://169.254.169.254/latest").is_err());
        assert!(custom_download_url("https://100.64.0.1/model.gguf").is_err());
        assert!(custom_download_url("https://localhost/model.gguf").is_err());
        assert!(custom_download_url("https://printer.local/model.gguf").is_err());
        assert!(custom_download_url("https://[::1]/model.gguf").is_err());
        assert!(custom_download_url("https://[fd00::1]/model.gguf").is_err());
        assert!(ip_is_public("8.8.8.8".parse().unwrap()));
        assert!(!ip_is_public("0.0.0.0".parse().unwrap()));
    }
}

use rookie::common::enums::Cookie;
use thiserror::Error;

const NICO_DOMAINS: &[&str] = &["nicovideo.jp", "nicochannel.jp", "nimg.jp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCookies {
    pub source: String,
    pub header_value: String,
    pub yt_dlp_argument: String,
}

pub fn load_browser_cookies(spec: &str) -> Result<BrowserCookies, CookieLoadError> {
    let browser = parse_browser_name(spec);
    let cookies = match browser.as_str() {
        "firefox" => rookie::firefox(Some(domain_filters())),
        "librewolf" => rookie::librewolf(Some(domain_filters())),
        "zen" => rookie::zen(Some(domain_filters())),
        "chrome" => rookie::chrome(Some(domain_filters())),
        "chromium" => rookie::chromium(Some(domain_filters())),
        "brave" => rookie::brave(Some(domain_filters())),
        "edge" => rookie::edge(Some(domain_filters())),
        "vivaldi" => rookie::vivaldi(Some(domain_filters())),
        "opera" => rookie::opera(Some(domain_filters())),
        "opera_gx" | "opera-gx" => rookie::opera_gx(Some(domain_filters())),
        "arc" => rookie::arc(Some(domain_filters())),
        #[cfg(target_os = "macos")]
        "safari" => rookie::safari(Some(domain_filters())),
        other => return Err(CookieLoadError::UnsupportedBrowser(other.to_string())),
    }
    .map_err(|error| CookieLoadError::BrowserRead {
        browser: browser.clone(),
        message: error.to_string(),
    })?;

    if cookies.is_empty() {
        return Err(CookieLoadError::NoCookies(browser));
    }

    Ok(BrowserCookies {
        source: spec.to_string(),
        header_value: cookie_header_value(&cookies),
        yt_dlp_argument: spec.to_string(),
    })
}

#[derive(Debug, Error)]
pub enum CookieLoadError {
    #[error("unsupported browser for `--cookies-from-browser`: {0}")]
    UnsupportedBrowser(String),

    #[error("failed to load cookies from browser `{browser}`: {message}")]
    BrowserRead { browser: String, message: String },

    #[error("no NicoNico cookies were found in browser `{0}`")]
    NoCookies(String),
}

fn domain_filters() -> Vec<String> {
    NICO_DOMAINS
        .iter()
        .map(|domain| (*domain).to_string())
        .collect()
}

fn parse_browser_name(spec: &str) -> String {
    spec.split(':').next().unwrap_or(spec).trim().to_lowercase()
}

fn cookie_header_value(cookies: &[Cookie]) -> String {
    cookies
        .iter()
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::parse_browser_name;

    #[test]
    fn extracts_browser_name_from_spec() {
        assert_eq!(parse_browser_name("chrome"), "chrome");
        assert_eq!(parse_browser_name("chrome:Default"), "chrome");
        assert_eq!(parse_browser_name("firefox:profile::container"), "firefox");
    }
}

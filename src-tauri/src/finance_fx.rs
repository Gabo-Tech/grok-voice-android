//! Display-only exchange rates. Native amounts stay in finance.json.
//! A missing pair is omitted. A cached pair is labeled when it is old.
//!
//! `api.frankfurter.app` now answers 301 to `api.frankfurter.dev`. The old
//! redirect allowlist refused that host, so every fiat fetch failed before a
//! body was read. Fiat and crypto each walk a provider list and stop at the
//! first real book. A total failure returns an error and does not invent a rate.

use chrono::TimeZone;
use serde::{Deserialize, Serialize};

pub const STALE_MS: i64 = 48 * 60 * 60 * 1000;

/// Fiat, first success wins. Frankfurter is called on its current host, not the redirecting one.
pub const FIAT_PROVIDERS: &[&str] = &["frankfurter", "open-er-api", "ecb", "currency-api", "currency-api-pages"];
/// Crypto providers, in order. Earlier books win for a code; later books only fill gaps
/// so a five-coin primary does not hide every other ticker.
pub const CRYPTO_PROVIDERS: &[&str] = &["coingecko", "coinbase", "kraken", "currency-api", "currency-api-pages"];

const FIAT_CODES: &[&str] = &[
    "AED", "AUD", "BGN", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF", "IDR", "ILS", "INR", "ISK", "JPY", "KRW", "MXN", "MYR", "NOK", "NZD", "PHP", "PLN", "RON", "SAR", "SEK", "SGD", "THB", "TRY", "USD", "ZAR",
];

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FxBook {
    #[serde(default)]
    pub as_of: String,
    #[serde(default)]
    pub fetched_ms: i64,
    /// Local time of the last fiat book. Zero on books saved before this field existed.
    #[serde(default)]
    pub fiat_fetched_ms: i64,
    #[serde(default)]
    pub per_eur: Vec<FxLeg>,
    #[serde(default)]
    pub manual: Vec<ManualRate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FxLeg {
    pub currency: String,
    pub per_eur: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualRate {
    pub base: String,
    pub quote: String,
    pub rate: f64,
    #[serde(default)]
    pub as_of: String,
    /// When true, this pair replaces today's live rate. Otherwise it is only a fallback.
    #[serde(default)]
    pub override_live: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Converted {
    pub amount_minor: i64,
    pub currency: String,
    pub as_of: String,
    pub stale: bool,
    pub source: &'static str,
}

pub fn exponent(currency: &str) -> u32 {
    let code = currency.to_ascii_uppercase();
    match code.as_str() {
        "JPY" | "KRW" | "VND" | "CLP" => 0,
        "BHD" | "KWD" | "OMR" => 3,
        _ if is_crypto(&code) => 8,
        _ => 2,
    }
}

pub fn stale(book: &FxBook, now_ms: i64) -> bool {
    if book.per_eur.is_empty() {
        return false;
    }
    if book.fetched_ms <= 0 {
        return true;
    }
    if !same_local_day(book.fetched_ms, now_ms) {
        return true;
    }
    now_ms.saturating_sub(book.fetched_ms) > STALE_MS
}

/// A fetch counts as today's rate only on the device's local calendar day.
pub fn same_local_day(a_ms: i64, b_ms: i64) -> bool {
    if a_ms <= 0 || b_ms <= 0 {
        return false;
    }
    let Some(a) = chrono::Local.timestamp_millis_opt(a_ms).single() else {
        return false;
    };
    let Some(b) = chrono::Local.timestamp_millis_opt(b_ms).single() else {
        return false;
    };
    a.date_naive() == b.date_naive()
}

pub fn convert(amount_minor: i64, from: &str, to: &str, book: &FxBook, now_ms: i64) -> Option<Converted> {
    let from = from.trim().to_ascii_uppercase();
    let to = to.trim().to_ascii_uppercase();
    if from.is_empty() || to.is_empty() {
        return None;
    }
    if from == to {
        return Some(Converted { amount_minor, currency: to, as_of: String::new(), stale: false, source: "identity" });
    }
    let (rate, source, as_of) = rate_of(&from, &to, book)?;
    if !rate.is_finite() || rate <= 0.0 {
        return None;
    }
    let major = amount_minor as f64 / 10f64.powi(exponent(&from) as i32);
    let minor = (major * rate * 10f64.powi(exponent(&to) as i32)).round();
    if !minor.is_finite() {
        return None;
    }
    Some(Converted {
        amount_minor: minor as i64,
        currency: to,
        as_of,
        stale: source == "cached" && stale(book, now_ms),
        source,
    })
}

/// Codes that cannot move into `display`. An empty result means every part converted.
pub fn missing_codes(parts: &[(i64, String)], display: &str, book: &FxBook) -> Vec<String> {
    let display = display.trim().to_ascii_uppercase();
    let mut missing = Vec::new();
    for (amount, currency) in parts {
        if *amount == 0 {
            continue;
        }
        let code = currency.trim().to_ascii_uppercase();
        if code.is_empty() {
            continue;
        }
        if display.is_empty() || convert(*amount, &code, &display, book, 0).is_none() {
            if !missing.iter().any(|row: &String| row == &code) {
                missing.push(code);
            }
        }
    }
    missing.sort();
    missing
}

pub fn combine(parts: &[(i64, String)], display: &str, book: &FxBook, now_ms: i64) -> Option<Converted> {
    let display = display.trim().to_ascii_uppercase();
    if display.is_empty() {
        return None;
    }
    let usable: Vec<&(i64, String)> = parts.iter().filter(|part| part.0 != 0 && !part.1.is_empty()).collect();
    if usable.is_empty() {
        return None;
    }
    let mut total = 0i64;
    let mut stale_rate = false;
    let mut as_of = String::new();
    let mut source = "identity";
    for (amount, currency) in usable {
        let converted = convert(*amount, currency, &display, book, now_ms)?;
        total = total.saturating_add(converted.amount_minor);
        stale_rate |= converted.stale;
        if !converted.as_of.is_empty() && (as_of.is_empty() || converted.as_of < as_of) {
            as_of = converted.as_of;
        }
        if converted.source == "manual" {
            source = "manual";
        } else if converted.source == "cached" && source != "manual" {
            source = "cached";
        }
    }
    Some(Converted { amount_minor: total, currency: display, as_of, stale: stale_rate, source })
}

pub fn upsert_manual(book: &mut FxBook, base: &str, quote: &str, rate: f64, as_of: &str, override_live: bool) -> Result<(), String> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err("The rate has to be a number above zero.".into());
    }
    let base = base.trim().to_ascii_uppercase();
    let quote = quote.trim().to_ascii_uppercase();
    if base.len() != 3 || quote.len() != 3 || base == quote {
        return Err("Say two currencies, such as 1 USD in EUR.".into());
    }
    book.manual.retain(|row| !(row.base.eq_ignore_ascii_case(&base) && row.quote.eq_ignore_ascii_case(&quote)));
    book.manual.push(ManualRate { base, quote, rate, as_of: as_of.to_string(), override_live });
    Ok(())
}

/// True when Finance should hit the network. A book fetched earlier today is enough.
/// `force` is the Settings refresh. An empty book always tries.
pub fn needs_live_fetch(book: &FxBook, now_ms: i64, force: bool) -> bool {
    if force || book.per_eur.is_empty() {
        return true;
    }
    let stamp = if book.fiat_fetched_ms > 0 { book.fiat_fetched_ms } else { book.fetched_ms };
    !same_local_day(stamp, now_ms)
}

pub fn fetch_book(previous: &FxBook, now_ms: i64) -> Result<FxBook, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(crate::http::rates_redirect())
        .build()
        .map_err(|_| "Could not refresh rates. Native amounts stay.".to_string())?;
    fetch_book_with(previous, now_ms, &|url| get(&client, url))
}

/// Walks fiat providers, then crypto providers. The first real book in each list wins.
/// If every provider fails, the error leaves the caller's previous book untouched.
pub fn fetch_book_with(previous: &FxBook, now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>) -> Result<FxBook, String> {
    let mut next = previous.clone();
    let fiat_ok = first_ok(FIAT_PROVIDERS, get, |id, body| apply_fiat(id, body, &mut next));
    let crypto_ok = merge_crypto(get, &mut next);
    if !fiat_ok && !crypto_ok {
        return Err("Could not refresh rates. Native amounts stay.".into());
    }
    if fiat_ok {
        next.fetched_ms = now_ms;
        next.fiat_fetched_ms = now_ms;
    }
    Ok(next)
}

fn merge_crypto(get: &dyn Fn(&str) -> Result<String, String>, book: &mut FxBook) -> bool {
    let mut any = false;
    for id in CRYPTO_PROVIDERS {
        let Some(url) = provider_url(id) else { continue };
        let Ok(body) = get(url) else { continue };
        if apply_crypto(id, &body, book) {
            any = true;
        }
    }
    any
}

fn first_ok(ids: &[&str], get: &dyn Fn(&str) -> Result<String, String>, mut apply: impl FnMut(&str, &str) -> bool) -> bool {
    for id in ids {
        let Some(url) = provider_url(id) else { continue };
        let Ok(body) = get(url) else { continue };
        if apply(id, &body) {
            return true;
        }
    }
    false
}

pub fn provider_url(id: &str) -> Option<&'static str> {
    Some(match id {
        "frankfurter" => "https://api.frankfurter.dev/v1/latest?base=EUR",
        "open-er-api" => "https://open.er-api.com/v6/latest/EUR",
        "ecb" => "https://data-api.ecb.europa.eu/service/data/EXR/D.USD+CHF+GBP+JPY+AUD+CAD+CNY+SEK+NOK+DKK+PLN+CZK+HUF+TRY+ZAR+MXN+BRL+INR+KRW+SGD+HKD+NZD+THB+ILS+RON+BGN+ISK+PHP+IDR+MYR.EUR.SP00.A?lastNObservations=1&format=csvdata&detail=dataonly",
        "currency-api" => "https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@latest/v1/currencies/eur.min.json",
        "currency-api-pages" => "https://latest.currency-api.pages.dev/v1/currencies/eur.min.json",
        "coingecko" => "https://api.coingecko.com/api/v3/simple/price?ids=monero,bitcoin,ethereum,solana,litecoin&vs_currencies=eur",
        "coinbase" => "https://api.coinbase.com/v2/exchange-rates?currency=EUR",
        "kraken" => "https://api.kraken.com/0/public/Ticker?pair=XXBTZEUR,XETHZEUR,XXMRZEUR,SOLEUR,XLTCZEUR",
        _ => return None,
    })
}

fn apply_fiat(id: &str, body: &str, book: &mut FxBook) -> bool {
    let Ok((date, rates)) = (match id {
        "frankfurter" => parse_frankfurter(body),
        "open-er-api" => parse_er_api(body),
        "ecb" => parse_ecb_csv(body),
        "currency-api" | "currency-api-pages" => parse_currency_api_fiat(body),
        _ => return false,
    }) else {
        return false;
    };
    let rates: Vec<(String, f64)> = rates.into_iter().filter(|(code, per)| per.is_finite() && *per > 0.0 && !is_crypto(code)).collect();
    if rates.is_empty() || date.is_empty() {
        return false;
    }
    book.per_eur.retain(|leg| is_crypto(&leg.currency));
    book.as_of = date;
    put_leg(book, "EUR", 1.0);
    for (code, per) in rates {
        put_leg(book, &code, per);
    }
    true
}

fn apply_crypto(id: &str, body: &str, book: &mut FxBook) -> bool {
    let legs = match id {
        "coingecko" => parse_coingecko(body).map(|rows| rows.into_iter().filter_map(|(code, eur)| invert_price(&code, eur)).collect()),
        "kraken" => parse_kraken(body),
        "coinbase" => parse_coinbase_crypto(body),
        "currency-api" | "currency-api-pages" => parse_currency_api_crypto(body),
        _ => return false,
    };
    let Ok(legs) = legs else { return false };
    let legs: Vec<(String, f64)> = legs.into_iter().filter(|(code, per)| is_crypto(code) && per.is_finite() && *per > 0.0).collect();
    if legs.is_empty() {
        return false;
    }
    for (code, per) in legs {
        if book.per_eur.iter().any(|row| row.currency.eq_ignore_ascii_case(&code)) {
            continue;
        }
        put_leg(book, &code, per);
    }
    true
}

fn invert_price(code: &str, eur_price: f64) -> Option<(String, f64)> {
    if !eur_price.is_finite() || eur_price <= 0.0 {
        return None;
    }
    Some((code.to_string(), 1.0 / eur_price))
}

pub fn is_fiat(code: &str) -> bool {
    let code = code.to_ascii_uppercase();
    FIAT_CODES.iter().any(|fiat| *fiat == code)
}

/// A crypto ticker, not a closed list of five coins. Three-letter codes must be known
/// here so an ISO fiat code is not treated as a coin. Longer tickers are crypto.
pub fn is_crypto(code: &str) -> bool {
    let code = code.to_ascii_uppercase();
    if code.is_empty() || is_fiat(&code) {
        return false;
    }
    const KNOWN: &[&str] = &[
        "BTC", "ETH", "XMR", "SOL", "LTC", "BCH", "XRP", "XLM", "DOGE", "ADA", "DOT", "AVAX", "ATOM", "LINK", "UNI", "AAVE", "MATIC", "POL", "NEAR", "APT", "SUI", "TON", "TRX", "ETC", "FIL", "ICP", "HBAR", "ALGO", "EOS", "XTZ", "DASH", "ZEC", "NEO", "USDT", "USDC", "DAI", "SHIB", "PEPE", "ARB", "OP", "INJ", "TIA", "SEI", "RENDER", "FET", "WLD", "BONK", "FLOKI",
    ];
    if KNOWN.contains(&code.as_str()) {
        return true;
    }
    const DENY: &[&str] = &[
        "CREDIT", "BALANCE", "WORTH", "CUSTOM", "PROPERTY", "COLLECTIBLE", "STAKE", "INCOME", "EXPENSE", "PAYMENT", "WALLET", "HOLDING", "ACCOUNT", "ASSET", "OWED", "DEBT", "CASH", "CARD", "LOAN", "FUND", "STOCK", "ABOUT", "UNITS", "BITCOIN", "ETHEREUM", "MONERO", "SOLANA", "DOGECOIN", "LITECOIN",
    ];
    if DENY.contains(&code.as_str()) {
        return false;
    }
    let len = code.len();
    (4..=12).contains(&len) && code.chars().all(|ch| ch.is_ascii_alphabetic())
}

pub fn is_currency_code(code: &str) -> bool {
    let code = code.to_ascii_uppercase();
    if is_fiat(&code) || is_crypto(&code) {
        return true;
    }
    code.len() == 3 && code.chars().all(|ch| ch.is_ascii_alphabetic())
}

pub fn parse_frankfurter(body: &str) -> Result<(String, Vec<(String, f64)>), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    let date = value.get("date").and_then(|item| item.as_str()).unwrap_or("").to_string();
    if date.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    let mut rates = Vec::new();
    if let Some(map) = value.get("rates").and_then(|item| item.as_object()) {
        for (code, rate) in map {
            if let Some(rate) = rate.as_f64() {
                rates.push((code.to_ascii_uppercase(), rate));
            }
        }
    }
    if rates.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok((date, rates))
}

pub fn parse_er_api(body: &str) -> Result<(String, Vec<(String, f64)>), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    if value.get("result").and_then(|item| item.as_str()) != Some("success") {
        return Err("The rate source did not answer.".into());
    }
    let unix = value.get("time_last_update_unix").and_then(|item| item.as_i64()).unwrap_or(0);
    let date = chrono::DateTime::from_timestamp(unix, 0).map(|when| when.date_naive().to_string()).unwrap_or_default();
    let mut rates = Vec::new();
    if let Some(map) = value.get("rates").and_then(|item| item.as_object()) {
        for (code, rate) in map {
            if let Some(rate) = json_rate(rate) {
                rates.push((code.to_ascii_uppercase(), rate));
            }
        }
    }
    if date.is_empty() || rates.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok((date, rates))
}

pub fn parse_ecb_csv(body: &str) -> Result<(String, Vec<(String, f64)>), String> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().ok_or("The rate source did not answer.")?;
    let cols: Vec<String> = split_csv(header).into_iter().map(|cell| cell.to_ascii_uppercase()).collect();
    let currency_at = cols.iter().position(|cell| cell == "CURRENCY").ok_or("The rate source did not answer.")?;
    let date_at = cols.iter().position(|cell| cell == "TIME_PERIOD").ok_or("The rate source did not answer.")?;
    let value_at = cols.iter().position(|cell| cell == "OBS_VALUE").ok_or("The rate source did not answer.")?;
    let mut rates = Vec::new();
    let mut date = String::new();
    for line in lines {
        let cells = split_csv(line);
        let Some(code) = cells.get(currency_at).map(|cell| cell.to_ascii_uppercase()) else { continue };
        let Some(when) = cells.get(date_at).cloned() else { continue };
        let Some(raw) = cells.get(value_at) else { continue };
        let Ok(rate) = raw.parse::<f64>() else { continue };
        if code.is_empty() || !rate.is_finite() || rate <= 0.0 {
            continue;
        }
        if date.is_empty() || when > date {
            date = when;
        }
        rates.push((code, rate));
    }
    if date.is_empty() || rates.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok((date, rates))
}

pub fn parse_currency_api_fiat(body: &str) -> Result<(String, Vec<(String, f64)>), String> {
    let (date, map) = currency_api_map(body)?;
    let rates = map
        .into_iter()
        .filter(|(code, _)| FIAT_CODES.iter().any(|fiat| fiat.eq_ignore_ascii_case(code)) && !is_crypto(code))
        .collect::<Vec<_>>();
    if rates.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok((date, rates))
}

pub fn parse_currency_api_crypto(body: &str) -> Result<Vec<(String, f64)>, String> {
    let (_, map) = currency_api_map(body)?;
    let legs = map.into_iter().filter(|(code, _)| is_crypto(code)).collect::<Vec<_>>();
    if legs.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok(legs)
}

fn currency_api_map(body: &str) -> Result<(String, Vec<(String, f64)>), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    let date = value.get("date").and_then(|item| item.as_str()).unwrap_or("").to_string();
    let Some(map) = value.get("eur").and_then(|item| item.as_object()) else {
        return Err("The rate source did not answer.".into());
    };
    let mut rates = Vec::new();
    for (code, rate) in map {
        if let Some(rate) = json_rate(rate) {
            if rate.is_finite() && rate > 0.0 {
                rates.push((code.to_ascii_uppercase(), rate));
            }
        }
    }
    if date.is_empty() || rates.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok((date, rates))
}

pub fn parse_coinbase_crypto(body: &str) -> Result<Vec<(String, f64)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    let Some(map) = value.get("data").and_then(|row| row.get("rates")).and_then(|row| row.as_object()) else {
        return Err("The rate source did not answer.".into());
    };
    let mut legs = Vec::new();
    for (code, rate) in map {
        if !is_crypto(code) {
            continue;
        }
        if let Some(rate) = json_rate(rate) {
            if rate.is_finite() && rate > 0.0 {
                legs.push((code.to_ascii_uppercase(), rate));
            }
        }
    }
    if legs.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok(legs)
}

pub fn parse_kraken(body: &str) -> Result<Vec<(String, f64)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    if value.get("error").and_then(|item| item.as_array()).is_some_and(|rows| !rows.is_empty()) {
        return Err("The rate source did not answer.".into());
    }
    let Some(map) = value.get("result").and_then(|item| item.as_object()) else {
        return Err("The rate source did not answer.".into());
    };
    let mut legs = Vec::new();
    for (pair, row) in map {
        let Some(code) = kraken_code(pair) else { continue };
        let Some(price) = row.get("c").and_then(|cell| cell.get(0)).and_then(json_rate) else { continue };
        if let Some(leg) = invert_price(code, price) {
            legs.push(leg);
        }
    }
    if legs.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok(legs)
}

fn kraken_code(pair: &str) -> Option<&'static str> {
    let pair = pair.to_ascii_uppercase();
    if pair.contains("XBT") || pair.contains("BTC") {
        Some("BTC")
    } else if pair.contains("ETH") {
        Some("ETH")
    } else if pair.contains("XMR") {
        Some("XMR")
    } else if pair.contains("SOL") {
        Some("SOL")
    } else if pair.contains("LTC") {
        Some("LTC")
    } else {
        None
    }
}

fn json_rate(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(std::mem::take(&mut cell));
            }
            _ => cell.push(ch),
        }
    }
    out.push(cell);
    out
}

pub fn parse_coingecko(body: &str) -> Result<Vec<(String, f64)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The rate source did not answer.".to_string())?;
    let map = [
        ("bitcoin", "BTC"),
        ("ethereum", "ETH"),
        ("monero", "XMR"),
        ("solana", "SOL"),
        ("litecoin", "LTC"),
    ];
    let mut legs = Vec::new();
    for (id, code) in map {
        if let Some(price) = value.get(id).and_then(|row| row.get("eur")).and_then(|price| price.as_f64()) {
            legs.push((code.to_string(), price));
        }
    }
    if legs.is_empty() {
        return Err("The rate source did not answer.".into());
    }
    Ok(legs)
}

/// Precedence: an explicit override, then a fetched pair (fresh or labeled stale), then a manual fallback.
/// EUR's 1.0 leg is the Frankfurter base, not a cross-currency 1:1. A missing leg stays missing.
fn rate_of(from: &str, to: &str, book: &FxBook) -> Option<(f64, &'static str, String)> {
    if let Some(found) = manual_rate(book, from, to, true) {
        return Some(found);
    }
    if let Some(rate) = cached_rate(book, from, to) {
        return Some((rate, "cached", book.as_of.clone()));
    }
    manual_rate(book, from, to, false)
}

fn manual_rate(book: &FxBook, from: &str, to: &str, override_live: bool) -> Option<(f64, &'static str, String)> {
    if let Some(row) = book.manual.iter().find(|row| row.override_live == override_live && row.base.eq_ignore_ascii_case(from) && row.quote.eq_ignore_ascii_case(to) && row.rate > 0.0) {
        return Some((row.rate, "manual", if row.as_of.is_empty() { book.as_of.clone() } else { row.as_of.clone() }));
    }
    if let Some(row) = book.manual.iter().find(|row| row.override_live == override_live && row.base.eq_ignore_ascii_case(to) && row.quote.eq_ignore_ascii_case(from) && row.rate > 0.0) {
        return Some((1.0 / row.rate, "manual", if row.as_of.is_empty() { book.as_of.clone() } else { row.as_of.clone() }));
    }
    None
}

fn cached_rate(book: &FxBook, from: &str, to: &str) -> Option<f64> {
    let from_per = if from == "EUR" { Some(1.0) } else { leg(book, from) };
    let to_per = if to == "EUR" { Some(1.0) } else { leg(book, to) };
    let rate = to_per? / from_per?;
    if rate.is_finite() && rate > 0.0 { Some(rate) } else { None }
}

fn leg(book: &FxBook, currency: &str) -> Option<f64> {
    book.per_eur.iter().find(|row| row.currency.eq_ignore_ascii_case(currency) && row.per_eur > 0.0).map(|row| row.per_eur)
}

fn put_leg(book: &mut FxBook, currency: &str, per_eur: f64) {
    if let Some(row) = book.per_eur.iter_mut().find(|row| row.currency.eq_ignore_ascii_case(currency)) {
        row.per_eur = per_eur;
        row.currency = currency.to_string();
    } else {
        book.per_eur.push(FxLeg { currency: currency.to_string(), per_eur });
    }
}

fn get(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "GrokVoice/0.1.42")
        .header(reqwest::header::ACCEPT, "application/json, text/csv, */*")
        .send()
        .map_err(|_| "Could not refresh rates. Native amounts stay.".to_string())?
        .error_for_status()
        .map_err(|_| "Could not refresh rates. Native amounts stay.".to_string())?
        .text()
        .map_err(|_| "Could not refresh rates. Native amounts stay.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> FxBook {
        FxBook {
            as_of: "2026-11-12".into(),
            fetched_ms: 1_000,
            fiat_fetched_ms: 0,
            per_eur: vec![FxLeg { currency: "EUR".into(), per_eur: 1.0 }, FxLeg { currency: "USD".into(), per_eur: 1.1 }, FxLeg { currency: "CHF".into(), per_eur: 0.95 }],
            manual: Vec::new(),
        }
    }

    #[test]
    fn conversion_needs_a_real_rate() {
        let fresh = book();
        let usd = convert(1_100, "USD", "EUR", &fresh, 1_000).unwrap();
        assert_eq!(usd.amount_minor, 1_000);
        assert!(!usd.stale);
        assert_eq!(usd.source, "cached");
        assert!(convert(1_100, "USD", "XMR", &fresh, 1_000).is_none());
        let hero = combine(&[(10_000, "EUR".into()), (1_100, "USD".into())], "EUR", &fresh, 1_000).unwrap();
        assert_eq!(hero.amount_minor, 11_000);
        assert_ne!(hero.amount_minor, 11_100);
        assert!(combine(&[(10_000, "EUR".into()), (100, "XMR".into())], "EUR", &fresh, 1_000).is_none());
        assert_eq!(missing_codes(&[(10_000, "EUR".into()), (100, "XMR".into())], "EUR", &fresh), vec!["XMR".to_string()]);
        assert!(missing_codes(&[(10_000, "EUR".into()), (1_100, "USD".into())], "EUR", &fresh).is_empty());
        let mut old = fresh.clone();
        old.fetched_ms = 1_000 - STALE_MS - 1;
        let stale_rate = convert(1_100, "USD", "EUR", &old, 1_000).unwrap();
        assert!(stale_rate.stale);
        assert_eq!(stale_rate.amount_minor, 1_000);
        let mut manual = fresh.clone();
        upsert_manual(&mut manual, "USD", "EUR", 0.5, "2026-01-01", false).unwrap();
        let live = convert(1_100, "USD", "EUR", &manual, 1_000).unwrap();
        assert_eq!(live.amount_minor, 1_000);
        assert_eq!(live.source, "cached");
        assert_ne!(live.amount_minor, 550);
        upsert_manual(&mut manual, "USD", "EUR", 0.5, "2026-01-01", true).unwrap();
        let overridden = convert(1_100, "USD", "EUR", &manual, 1_000).unwrap();
        assert_eq!(overridden.amount_minor, 550);
        assert_eq!(overridden.source, "manual");
        assert_eq!(convert(-3_000, "EUR", "USD", &fresh, 1_000).unwrap().amount_minor, -3_300);
        assert!(upsert_manual(&mut manual, "USD", "EUR", 0.0, "", false).is_err());
    }

    #[test]
    fn live_today_beats_a_stale_cache_and_a_manual_fallback() {
        let now = chrono::Local::now();
        let now_ms = now.timestamp_millis();
        let yesterday = now.date_naive().pred_opt().unwrap().and_hms_opt(12, 0, 0).unwrap();
        let yesterday_ms = yesterday.and_local_timezone(chrono::Local).earliest().expect("yesterday").timestamp_millis();
        assert!(now_ms - yesterday_ms < STALE_MS);
        assert!(!same_local_day(yesterday_ms, now_ms));
        assert!(same_local_day(now_ms, now_ms));

        let mut live = book();
        live.fetched_ms = now_ms;
        upsert_manual(&mut live, "USD", "EUR", 0.5, "2026-01-01", false).unwrap();
        let today = convert(1_100, "USD", "EUR", &live, now_ms).unwrap();
        assert_eq!(today.amount_minor, 1_000);
        assert!(!today.stale);
        assert_eq!(today.source, "cached");
        assert_ne!(today.amount_minor, 1_100);

        let mut stale_book = live.clone();
        stale_book.fetched_ms = yesterday_ms;
        let stale_rate = convert(1_100, "USD", "EUR", &stale_book, now_ms).unwrap();
        assert!(stale_rate.stale);
        assert_eq!(stale_rate.amount_minor, 1_000);
        assert_eq!(stale_rate.source, "cached");
        assert_ne!(stale_rate.amount_minor, 550);

        let mut missing = FxBook::default();
        assert!(convert(1_100, "USD", "EUR", &missing, now_ms).is_none());
        assert!(combine(&[(10_000, "EUR".into()), (1_100, "USD".into())], "EUR", &missing, now_ms).is_none());
        upsert_manual(&mut missing, "USD", "EUR", 0.5, "2026-01-01", false).unwrap();
        let fallback = convert(1_100, "USD", "EUR", &missing, now_ms).unwrap();
        assert_eq!(fallback.amount_minor, 550);
        assert_eq!(fallback.source, "manual");
        let stored: ManualRate = serde_json::from_str(r#"{"base":"USD","quote":"EUR","rate":0.5,"asOf":"2026-01-01"}"#).unwrap();
        assert!(!stored.override_live);
    }

    #[test]
    fn parsers_do_not_invent_a_missing_pair() {
        let (date, rates) = parse_frankfurter(r#"{"amount":1.0,"base":"EUR","date":"2026-11-12","rates":{"USD":1.08,"CHF":0.94}}"#).unwrap();
        assert_eq!(date, "2026-11-12");
        assert!(rates.iter().any(|row| row.0 == "USD" && row.1 == 1.08));
        assert!(parse_frankfurter(r#"{"date":"2026-11-12","rates":{}}"#).is_err());
        let coins = parse_coingecko(r#"{"monero":{"eur":150.0},"bitcoin":{"eur":80000.0}}"#).unwrap();
        assert!(coins.iter().any(|row| row.0 == "XMR" && row.1 == 150.0));
        assert!(!coins.iter().any(|row| row.0 == "SOL"));
        let mut stored = FxBook::default();
        put_leg(&mut stored, "XMR", 1.0 / 150.0);
        let xmr = convert(100_000_000, "XMR", "EUR", &stored, 5_000).unwrap();
        assert_eq!(xmr.amount_minor, 15_000);
        assert!(xmr.stale);
    }

    #[test]
    fn provider_order_starts_with_frankfurter_then_falls_through() {
        assert_eq!(FIAT_PROVIDERS[0], "frankfurter");
        assert_eq!(CRYPTO_PROVIDERS[0], "coingecko");
        assert!(provider_url("frankfurter").unwrap().contains("api.frankfurter.dev"));
        assert!(!provider_url("frankfurter").unwrap().contains("api.frankfurter.app"));
        let calls = std::sync::Mutex::new(Vec::new());
        let book = fetch_book_with(&FxBook::default(), 5_000, &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("frankfurter") {
                return Err("redirect refused".into());
            }
            if url.contains("open.er-api.com") {
                return Ok(r#"{"result":"success","time_last_update_unix":1794441600,"base_code":"EUR","rates":{"USD":1.1,"CHF":0.95}}"#.into());
            }
            if url.contains("coingecko") {
                return Err("429".into());
            }
            if url.contains("coinbase") {
                return Ok(r#"{"data":{"currency":"EUR","rates":{"BTC":"0.00002","USD":"1.1"}}}"#.into());
            }
            Err("not used".into())
        })
        .unwrap();
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("frankfurter")));
        assert!(seen.iter().any(|url| url.contains("open.er-api.com")));
        assert!(!seen.iter().any(|url| url.contains("ecb.europa.eu")), "a later fiat host stays unused after a real book");
        assert!(seen.iter().any(|url| url.contains("coingecko")));
        assert!(seen.iter().any(|url| url.contains("coinbase")));
        assert!(seen.iter().any(|url| url.contains("kraken")), "later crypto books fill tickers the first book missed");
        let usd = convert(1_100, "USD", "EUR", &book, 5_000).unwrap();
        assert_eq!(usd.amount_minor, 1_000);
        assert_ne!(usd.amount_minor, 1_100);
        let btc = convert(100_000_000, "BTC", "EUR", &book, 5_000).unwrap();
        assert_eq!(btc.amount_minor, 5_000_000);
        assert_eq!(book.fetched_ms, 5_000);
        assert_eq!(book.fiat_fetched_ms, 5_000);
        assert!(!needs_live_fetch(&book, 5_000, false));
        assert!(needs_live_fetch(&book, 5_000, true));
    }

    #[test]
    fn a_total_provider_failure_does_not_invent_a_rate() {
        let mut previous = book();
        previous.fetched_ms = 1_000;
        let err = fetch_book_with(&previous, 9_000, &|_url| Err("down".into())).unwrap_err();
        assert!(err.contains("Native amounts stay"));
        assert!(parse_er_api("{}").is_err());
        assert!(parse_ecb_csv("CURRENCY,TIME_PERIOD,OBS_VALUE\n").is_err());
        assert!(parse_coinbase_crypto(r#"{"data":{"rates":{"USD":"1.1"}}}"#).is_err());
        assert!(parse_kraken(r#"{"error":["EGeneral"],"result":{}}"#).is_err());
        assert!(parse_currency_api_fiat(r#"{"date":"2026-11-12","eur":{"btc":0.00002}}"#).is_err());
        let untouched = convert(1_100, "USD", "EUR", &previous, 9_000).unwrap();
        assert_eq!(untouched.amount_minor, 1_000);
        assert_ne!(untouched.amount_minor, 1_100);
        let empty = FxBook::default();
        assert!(convert(1_100, "USD", "EUR", &empty, 9_000).is_none());
        assert!(combine(&[(10_000, "EUR".into()), (1_100, "USD".into())], "EUR", &empty, 9_000).is_none());
        let (date, rates) = parse_ecb_csv("KEY,CURRENCY,TIME_PERIOD,OBS_VALUE\nEXR,USD,2026-11-12,1.08\nEXR,CHF,2026-11-12,0.94\n").unwrap();
        assert_eq!(date, "2026-11-12");
        assert!(rates.iter().any(|row| row.0 == "USD" && row.1 == 1.08));
        let kept = fetch_book_with(&previous, 9_000, &|url| {
            if url.contains("coingecko") {
                Ok(r#"{"bitcoin":{"eur":80000.0}}"#.into())
            } else {
                Err("fiat down".into())
            }
        })
        .unwrap();
        assert_eq!(kept.fetched_ms, 1_000, "crypto alone does not mark yesterday's fiat as today's book");
        assert_eq!(convert(1_100, "USD", "EUR", &kept, 9_000).unwrap().amount_minor, 1_000);
        assert!(convert(100_000_000, "BTC", "EUR", &kept, 9_000).unwrap().amount_minor > 0);
    }
}

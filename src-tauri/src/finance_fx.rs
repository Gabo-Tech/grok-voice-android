//! Display-only exchange rates. Native amounts stay in finance.json.
//! A missing pair is omitted. A cached pair is labeled when it is old.

use serde::{Deserialize, Serialize};

pub const STALE_MS: i64 = 48 * 60 * 60 * 1000;
const FRANKFURTER: &str = "https://api.frankfurter.app/latest?from=EUR";
const COINGECKO: &str = "https://api.coingecko.com/api/v3/simple/price?ids=monero,bitcoin,ethereum,solana,litecoin&vs_currencies=eur";

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FxBook {
    #[serde(default)]
    pub as_of: String,
    #[serde(default)]
    pub fetched_ms: i64,
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
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" => 0,
        "BHD" | "KWD" | "OMR" => 3,
        "BTC" | "ETH" | "XMR" | "SOL" | "LTC" => 8,
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
    now_ms.saturating_sub(book.fetched_ms) > STALE_MS
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
        if converted.source == "cached" {
            source = "cached";
        } else if converted.source == "manual" && source != "cached" {
            source = "manual";
        }
    }
    Some(Converted { amount_minor: total, currency: display, as_of, stale: stale_rate, source })
}

pub fn upsert_manual(book: &mut FxBook, base: &str, quote: &str, rate: f64, as_of: &str) -> Result<(), String> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err("The rate has to be a number above zero.".into());
    }
    let base = base.trim().to_ascii_uppercase();
    let quote = quote.trim().to_ascii_uppercase();
    if base.len() != 3 || quote.len() != 3 || base == quote {
        return Err("Say two currencies, such as 1 USD in EUR.".into());
    }
    book.manual.retain(|row| !(row.base.eq_ignore_ascii_case(&base) && row.quote.eq_ignore_ascii_case(&quote)));
    book.manual.push(ManualRate { base, quote, rate, as_of: as_of.to_string() });
    Ok(())
}

pub fn fetch_book(previous: &FxBook, now_ms: i64) -> Result<FxBook, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(crate::http::rates_redirect())
        .build()
        .map_err(|_| "Could not refresh rates. Native amounts stay.".to_string())?;
    let mut next = previous.clone();
    let mut any = false;
    if let Ok(body) = get(&client, FRANKFURTER) {
        if let Ok((date, rates)) = parse_frankfurter(&body) {
            next.as_of = date;
            put_leg(&mut next, "EUR", 1.0);
            for (code, per) in rates {
                if per.is_finite() && per > 0.0 {
                    put_leg(&mut next, &code, per);
                }
            }
            any = true;
        }
    }
    if let Ok(body) = get(&client, COINGECKO) {
        if let Ok(legs) = parse_coingecko(&body) {
            for (code, eur_price) in legs {
                if eur_price.is_finite() && eur_price > 0.0 {
                    put_leg(&mut next, &code, 1.0 / eur_price);
                }
            }
            any = true;
        }
    }
    if !any {
        return Err("Could not refresh rates. Native amounts stay.".into());
    }
    next.fetched_ms = now_ms;
    Ok(next)
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

fn rate_of(from: &str, to: &str, book: &FxBook) -> Option<(f64, &'static str, String)> {
    if let Some(row) = book.manual.iter().find(|row| row.base.eq_ignore_ascii_case(from) && row.quote.eq_ignore_ascii_case(to) && row.rate > 0.0) {
        return Some((row.rate, "manual", if row.as_of.is_empty() { book.as_of.clone() } else { row.as_of.clone() }));
    }
    if let Some(row) = book.manual.iter().find(|row| row.base.eq_ignore_ascii_case(to) && row.quote.eq_ignore_ascii_case(from) && row.rate > 0.0) {
        return Some((1.0 / row.rate, "manual", if row.as_of.is_empty() { book.as_of.clone() } else { row.as_of.clone() }));
    }
    let from_per = if from == "EUR" { Some(1.0) } else { leg(book, from) };
    let to_per = if to == "EUR" { Some(1.0) } else { leg(book, to) };
    Some((to_per? / from_per?, "cached", book.as_of.clone()))
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
        .header(reqwest::header::USER_AGENT, "GrokVoice/0.1.38")
        .header(reqwest::header::ACCEPT, "application/json")
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
        upsert_manual(&mut manual, "USD", "EUR", 0.5, "2026-01-01").unwrap();
        let overridden = convert(1_100, "USD", "EUR", &manual, 1_000).unwrap();
        assert_eq!(overridden.amount_minor, 550);
        assert_eq!(overridden.source, "manual");
        assert_eq!(convert(-3_000, "EUR", "USD", &fresh, 1_000).unwrap().amount_minor, -3_300);
        assert!(upsert_manual(&mut manual, "USD", "EUR", 0.0, "").is_err());
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
}

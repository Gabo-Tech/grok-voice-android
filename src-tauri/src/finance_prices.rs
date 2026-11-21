//! Market prices for holdings. A quote is a real number from a provider, a
//! cached copy of one, or a price the user typed. Nothing here invents a rate.
//!
//! Crypto walks CoinGecko, then CoinPaprika, then Kraken. Metals use gold-api.com
//! (troy ounce). US ETF last closes use Atlantis when that symbol is listed.
//! A later provider fills only what an earlier one missed. An AI estimate is
//! stored only when every free provider missed, and it never replaces an API quote.
//! A manual price replaces the live quote only when the user marks it as an override.
//! Otherwise it is the fallback.

use serde::{Deserialize, Serialize};

use crate::finance_fx::{self, FxBook};

pub const PRICE_STALE_MS: i64 = 24 * 60 * 60 * 1000;
pub const TROY_OZ_GRAMS: f64 = 31.1034768;

/// Crypto, first success per symbol wins. Later books fill symbols the earlier one missed.
pub const CRYPTO_PROVIDERS: &[&str] = &["coingecko", "coinpaprika", "kraken"];
/// Spot metals per troy ounce. gold-api.com is the keyless source whose terms allow this.
pub const METAL_PROVIDERS: &[&str] = &["gold-api"];
/// US ETF last close is keyless. Individual stocks use an optional user key, in this order.
pub const STOCK_PROVIDERS: &[&str] = &["atlantis", "finnhub", "alphavantage", "twelvedata"];

#[derive(Clone, Debug, PartialEq)]
pub struct AssetRef {
    pub symbol: String,
    pub kind: String,
    /// `coin`, `troy_oz`, `gram`, or `share`.
    pub holding_unit: String,
    pub coingecko_id: String,
    pub paprika_id: String,
    pub kraken_pair: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spot {
    pub symbol: String,
    pub kind: String,
    pub price: f64,
    pub currency: String,
    pub source: String,
    pub fetched_ms: i64,
    #[serde(default)]
    pub estimated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManualPrice {
    pub symbol: String,
    pub price: f64,
    pub currency: String,
    #[serde(default)]
    pub as_of: String,
    /// When true, this price replaces a live quote. Otherwise it is only a fallback.
    #[serde(default)]
    pub override_live: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceBook {
    #[serde(default)]
    pub quotes: Vec<Spot>,
    #[serde(default)]
    pub manual: Vec<ManualPrice>,
    /// Provider ids the user turned off. Empty means every provider is on.
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Optional keys the user typed in Settings. Empty means that provider is skipped.
    #[serde(default)]
    pub keys: PriceKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceKeys {
    #[serde(default)]
    pub finnhub: String,
    #[serde(default)]
    pub alphavantage: String,
    #[serde(default)]
    pub twelvedata: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Priced {
    pub price: f64,
    pub currency: String,
    pub source: &'static str,
    pub provider: String,
    pub fetched_ms: i64,
    pub stale: bool,
    pub estimated: bool,
    /// Major price of one holding unit (a gram, when the holding is in grams).
    pub unit_price: f64,
}

pub fn provider_enabled(book: &PriceBook, id: &str) -> bool {
    !book.disabled.iter().any(|row| row.eq_ignore_ascii_case(id))
}

pub fn quote_stale(fetched_ms: i64, now_ms: i64) -> bool {
    if fetched_ms <= 0 {
        return true;
    }
    now_ms.saturating_sub(fetched_ms) > PRICE_STALE_MS
}

pub fn needs_refresh(book: &PriceBook, assets: &[AssetRef], now_ms: i64, force: bool) -> bool {
    if force {
        return !assets.is_empty();
    }
    assets.iter().any(|asset| match find_quote(book, &asset.symbol) {
        Some(quote) if !quote.estimated && !quote_stale(quote.fetched_ms, now_ms) => false,
        _ => true,
    })
}

/// Map a holding's name, ticker, or currency onto a symbol. `None` means it stays manual.
pub fn resolve_holding(name: &str, ticker: &str, currency: &str, investment_type: &str, notes: &str) -> Option<AssetRef> {
    let unit = metal_unit(&format!("{name} {ticker} {notes}"));
    if let Some(mut asset) = alias(ticker).or_else(|| alias(name)) {
        if asset.kind == "metal" {
            asset.holding_unit = unit.to_string();
        }
        return Some(asset);
    }
    let kind = fold(investment_type);
    if kind == "stock" || kind == "fund" {
        let sym = ticker_token(ticker).or_else(|| ticker_token(name))?;
        if let Some(asset) = alias(&sym) {
            return Some(asset);
        }
        if finance_fx::is_fiat(&sym) {
            return None;
        }
        return Some(stock_ref(sym));
    }
    if kind == "crypto" || finance_fx::is_crypto(currency) {
        let sym = ticker_token(ticker).or_else(|| ticker_token(currency)).or_else(|| ticker_token(name))?;
        if finance_fx::is_fiat(&sym) {
            return None;
        }
        if let Some(asset) = alias(&sym) {
            return Some(asset);
        }
        return Some(crypto_ref(sym, String::new(), String::new(), String::new()));
    }
    None
}

pub fn price_for(book: &PriceBook, asset: &AssetRef, now_ms: i64) -> Option<Priced> {
    if let Some(row) = manual_row(book, &asset.symbol, true) {
        return Some(priced_manual(row, asset, now_ms));
    }
    if let Some(quote) = find_quote(book, &asset.symbol).filter(|quote| !quote.estimated) {
        return Some(priced_quote(quote, asset, now_ms));
    }
    if let Some(quote) = find_quote(book, &asset.symbol).filter(|quote| quote.estimated) {
        return Some(priced_quote(quote, asset, now_ms));
    }
    manual_row(book, &asset.symbol, false).map(|row| priced_manual(row, asset, now_ms))
}

pub fn major_to_minor(major: f64, exp: u32) -> Option<i64> {
    if !major.is_finite() || major <= 0.0 {
        return None;
    }
    let minor = (major * 10f64.powi(exp as i32)).round();
    if !minor.is_finite() || minor <= 0.0 || minor > i64::MAX as f64 {
        return None;
    }
    Some(minor as i64)
}

/// Write non-estimated crypto quotes into the FX book as units per EUR.
/// An estimated quote never replaces a leg that is already there.
pub fn apply_crypto_legs(book: &mut FxBook, prices: &PriceBook) {
    for quote in &prices.quotes {
        if quote.estimated || quote.kind != "crypto" || !quote.price.is_finite() || quote.price <= 0.0 {
            continue;
        }
        let Some(eur) = eur_price(quote.price, &quote.currency, book) else { continue };
        if eur <= 0.0 {
            continue;
        }
        let per = 1.0 / eur;
        if !per.is_finite() || per <= 0.0 {
            continue;
        }
        if let Some(row) = book.per_eur.iter_mut().find(|row| row.currency.eq_ignore_ascii_case(&quote.symbol)) {
            row.per_eur = per;
            row.currency = quote.symbol.clone();
        } else {
            book.per_eur.push(finance_fx::FxLeg { currency: quote.symbol.clone(), per_eur: per });
        }
    }
}

pub fn upsert_manual(book: &mut PriceBook, symbol: &str, price: f64, currency: &str, as_of: &str, override_live: bool) -> Result<(), String> {
    if !price.is_finite() || price <= 0.0 {
        return Err("The price has to be a number above zero.".into());
    }
    let symbol = symbol.trim().to_ascii_uppercase();
    let currency = currency.trim().to_ascii_uppercase();
    if symbol.is_empty() || symbol.len() > 12 || currency.len() != 3 {
        return Err("Say the symbol and a currency, such as 150 EUR for XMR.".into());
    }
    book.manual.retain(|row| !row.symbol.eq_ignore_ascii_case(&symbol));
    book.manual.push(ManualPrice { symbol, price, currency, as_of: as_of.to_string(), override_live });
    Ok(())
}

pub fn set_provider(book: &mut PriceBook, id: &str, enabled: bool) {
    let id = id.trim().to_ascii_lowercase();
    book.disabled.retain(|row| !row.eq_ignore_ascii_case(&id));
    if !enabled && !id.is_empty() {
        book.disabled.push(id);
    }
}

/// Walk providers for `assets`. Fresh API quotes are kept unless `force`.
/// `ai` runs only for a symbol every free provider missed. It cannot replace an API quote.
pub fn refresh_book(
    previous: &PriceBook,
    assets: &[AssetRef],
    now_ms: i64,
    force: bool,
    get: &dyn Fn(&str) -> Result<String, String>,
    ai: Option<&dyn Fn(&AssetRef) -> Result<(f64, String), String>>,
) -> PriceBook {
    let mut next = previous.clone();
    let pending: Vec<&AssetRef> = assets
        .iter()
        .filter(|asset| {
            force
                || match find_quote(&next, &asset.symbol) {
                    Some(quote) if !quote.estimated && !quote_stale(quote.fetched_ms, now_ms) => false,
                    _ => true,
                }
        })
        .collect();
    if pending.is_empty() {
        return next;
    }
    let mut open: Vec<&AssetRef> = pending.clone();
    fill_crypto(&mut next, &open, now_ms, get);
    open.retain(|asset| asset.kind == "crypto" && find_quote(&next, &asset.symbol).map(|quote| quote.estimated || quote_stale(quote.fetched_ms, now_ms) && force).unwrap_or(true) && !(find_quote(&next, &asset.symbol).is_some_and(|quote| !quote.estimated && quote.fetched_ms == now_ms)));
    // Recompute the still-missing set from what this pass stored.
    open = pending.iter().copied().filter(|asset| !fresh_api(&next, asset, now_ms)).collect();
    fill_metals(&mut next, &open, now_ms, get);
    open = pending.iter().copied().filter(|asset| !fresh_api(&next, asset, now_ms)).collect();
    fill_stocks(&mut next, &open, now_ms, get);
    if let Some(ai) = ai {
        for asset in pending {
            if fresh_api(&next, asset, now_ms) || find_quote(&next, &asset.symbol).is_some_and(|quote| !quote.estimated) {
                continue;
            }
            if let Ok((price, currency)) = ai(asset) {
                if price.is_finite() && price > 0.0 && currency.len() == 3 {
                    put_quote(
                        &mut next,
                        Spot {
                            symbol: asset.symbol.clone(),
                            kind: asset.kind.clone(),
                            price,
                            currency: currency.to_ascii_uppercase(),
                            source: "ai".into(),
                            fetched_ms: now_ms,
                            estimated: true,
                        },
                    );
                }
            }
        }
    }
    next
}

fn fresh_api(book: &PriceBook, asset: &AssetRef, now_ms: i64) -> bool {
    find_quote(book, &asset.symbol).is_some_and(|quote| !quote.estimated && quote.fetched_ms == now_ms)
}

fn fill_crypto(book: &mut PriceBook, assets: &[&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>) {
    let cryptos: Vec<&&AssetRef> = assets.iter().filter(|asset| asset.kind == "crypto").collect();
    if cryptos.is_empty() {
        return;
    }
    if provider_enabled(book, "coingecko") {
        let ids: Vec<&str> = cryptos.iter().filter_map(|asset| (!asset.coingecko_id.is_empty()).then_some(asset.coingecko_id.as_str())).collect();
        if !ids.is_empty() {
            let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=eur", ids.join(","));
            if let Ok(body) = get(&url) {
                if let Ok(rows) = parse_coingecko_ids(&body) {
                    for asset in &cryptos {
                        if let Some(price) = rows.iter().find(|(id, _)| id.eq_ignore_ascii_case(&asset.coingecko_id)).map(|(_, price)| *price) {
                            put_api(book, asset, price, "EUR", "coingecko", now_ms);
                        }
                    }
                }
            }
        }
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) {
                continue;
            }
            let query = if asset.coingecko_id.is_empty() { asset.symbol.to_ascii_lowercase() } else { continue };
            let url = format!("https://api.coingecko.com/api/v3/simple/price?symbols={query}&vs_currencies=eur");
            if let Ok(body) = get(&url) {
                if let Ok(rows) = parse_coingecko_symbols(&body) {
                    if let Some(price) = rows.into_iter().find(|(sym, _)| sym.eq_ignore_ascii_case(&asset.symbol)).map(|(_, price)| price) {
                        put_api(book, asset, price, "EUR", "coingecko", now_ms);
                        continue;
                    }
                }
            }
            let search = format!("https://api.coingecko.com/api/v3/search?query={}", enc(&asset.symbol));
            if let Ok(body) = get(&search) {
                if let Some(id) = coingecko_search_id(&body, &asset.symbol) {
                    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={id}&vs_currencies=eur");
                    if let Ok(body) = get(&url) {
                        if let Ok(rows) = parse_coingecko_ids(&body) {
                            if let Some(price) = rows.into_iter().find(|(row, _)| row.eq_ignore_ascii_case(&id)).map(|(_, price)| price) {
                                put_api(book, asset, price, "EUR", "coingecko", now_ms);
                            }
                        }
                    }
                }
            }
        }
    }
    if provider_enabled(book, "coinpaprika") {
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) {
                continue;
            }
            let id = if !asset.paprika_id.is_empty() {
                asset.paprika_id.clone()
            } else if let Ok(body) = get(&format!("https://api.coinpaprika.com/v1/search/?q={}&c=currencies&limit=5", enc(&asset.symbol))) {
                paprika_search_id(&body, &asset.symbol).unwrap_or_default()
            } else {
                String::new()
            };
            if id.is_empty() {
                continue;
            }
            let url = format!("https://api.coinpaprika.com/v1/tickers/{id}?quotes=EUR");
            if let Ok(body) = get(&url) {
                if let Some(price) = parse_paprika_eur(&body) {
                    put_api(book, asset, price, "EUR", "coinpaprika", now_ms);
                }
            }
        }
    }
    if provider_enabled(book, "kraken") {
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) || asset.kraken_pair.is_empty() {
                continue;
            }
            let url = format!("https://api.kraken.com/0/public/Ticker?pair={}", asset.kraken_pair);
            if let Ok(body) = get(&url) {
                if let Some(price) = parse_kraken_last(&body) {
                    put_api(book, asset, price, "EUR", "kraken", now_ms);
                }
            }
        }
    }
}

fn fill_metals(book: &mut PriceBook, assets: &[&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>) {
    if !provider_enabled(book, "gold-api") {
        return;
    }
    for asset in assets.iter().filter(|asset| asset.kind == "metal") {
        let url = format!("https://api.gold-api.com/price/{}/EUR", asset.symbol);
        if let Ok(body) = get(&url) {
            if let Some((price, currency)) = parse_gold_api(&body) {
                put_api(book, asset, price, &currency, "gold-api", now_ms);
            }
        }
    }
}

fn fill_stocks(book: &mut PriceBook, assets: &[&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>) {
    for asset in assets.iter().filter(|asset| asset.kind == "stock" || asset.kind == "fund") {
        if provider_enabled(book, "atlantis") && !fresh_api(book, asset, now_ms) {
            let url = format!("https://atlantisdatasolutions.com/v1/us/etfs/{}", asset.symbol);
            if let Ok(body) = get(&url) {
                if let Some((price, currency)) = parse_atlantis_etf(&body) {
                    put_api(book, asset, price, &currency, "atlantis", now_ms);
                }
            }
        }
        if provider_enabled(book, "finnhub") && !book.keys.finnhub.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://finnhub.io/api/v1/quote?symbol={}&token={}", enc(&asset.symbol), enc(book.keys.finnhub.trim()));
            if let Ok(body) = get(&url) {
                if let Some((price, currency)) = parse_finnhub_quote(&body) {
                    put_api(book, asset, price, &currency, "finnhub", now_ms);
                }
            }
        }
        if provider_enabled(book, "alphavantage") && !book.keys.alphavantage.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={}&apikey={}", enc(&asset.symbol), enc(book.keys.alphavantage.trim()));
            if let Ok(body) = get(&url) {
                if let Some((price, currency)) = parse_alpha_quote(&body) {
                    put_api(book, asset, price, &currency, "alphavantage", now_ms);
                }
            }
        }
        if provider_enabled(book, "twelvedata") && !book.keys.twelvedata.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://api.twelvedata.com/quote?symbol={}&apikey={}", enc(&asset.symbol), enc(book.keys.twelvedata.trim()));
            if let Ok(body) = get(&url) {
                if let Some((price, currency)) = parse_twelve_quote(&body) {
                    put_api(book, asset, price, &currency, "twelvedata", now_ms);
                }
            }
        }
    }
}

pub fn parse_finnhub_quote(body: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let price = json_rate(value.get("c")?)?;
    if price.is_finite() && price > 0.0 { Some((price, "USD".into())) } else { None }
}

pub fn parse_alpha_quote(body: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let quote = value.get("Global Quote")?;
    let price = json_rate(quote.get("05. price")?)?;
    if price.is_finite() && price > 0.0 { Some((price, "USD".into())) } else { None }
}

pub fn parse_twelve_quote(body: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let price = json_rate(value.get("close")?)?;
    let currency = value.get("currency").and_then(|item| item.as_str()).unwrap_or("USD").to_ascii_uppercase();
    if price.is_finite() && price > 0.0 && currency.len() == 3 { Some((price, currency)) } else { None }
}

fn put_api(book: &mut PriceBook, asset: &AssetRef, price: f64, currency: &str, source: &str, now_ms: i64) {
    if !price.is_finite() || price <= 0.0 {
        return;
    }
    put_quote(
        book,
        Spot {
            symbol: asset.symbol.clone(),
            kind: asset.kind.clone(),
            price,
            currency: currency.to_ascii_uppercase(),
            source: source.into(),
            fetched_ms: now_ms,
            estimated: false,
        },
    );
}

fn put_quote(book: &mut PriceBook, quote: Spot) {
    if quote.estimated && book.quotes.iter().any(|row| row.symbol.eq_ignore_ascii_case(&quote.symbol) && !row.estimated) {
        return;
    }
    if let Some(row) = book.quotes.iter_mut().find(|row| row.symbol.eq_ignore_ascii_case(&quote.symbol)) {
        *row = quote;
    } else {
        book.quotes.push(quote);
    }
}

pub fn parse_ai_price(text: &str) -> Result<(f64, String), String> {
    let trimmed = text.trim();
    let start = trimmed.find('{').ok_or("The estimate had no price.")?;
    let end = trimmed.rfind('}').ok_or("The estimate had no price.")?;
    let value: serde_json::Value = serde_json::from_str(&trimmed[start..=end]).map_err(|_| "The estimate had no price.".to_string())?;
    if value.get("price").is_some_and(|price| price.is_null()) {
        return Err("The estimate had no price.".into());
    }
    let price = value.get("price").and_then(json_rate).ok_or("The estimate had no price.")?;
    let currency = value.get("currency").and_then(|item| item.as_str()).unwrap_or("").trim().to_ascii_uppercase();
    if !price.is_finite() || price <= 0.0 || currency.len() != 3 || !currency.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err("The estimate had no price.".into());
    }
    Ok((price, currency))
}

pub fn parse_coingecko_ids(body: &str) -> Result<Vec<(String, f64)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The price source did not answer.".to_string())?;
    let Some(map) = value.as_object() else {
        return Err("The price source did not answer.".into());
    };
    let mut rows = Vec::new();
    for (id, row) in map {
        if let Some(price) = row.get("eur").and_then(json_rate) {
            if price.is_finite() && price > 0.0 {
                rows.push((id.clone(), price));
            }
        }
    }
    if rows.is_empty() {
        return Err("The price source did not answer.".into());
    }
    Ok(rows)
}

pub fn parse_coingecko_symbols(body: &str) -> Result<Vec<(String, f64)>, String> {
    let rows = parse_coingecko_ids(body)?;
    Ok(rows.into_iter().map(|(id, price)| (id.to_ascii_uppercase(), price)).collect())
}

pub fn coingecko_search_id(body: &str, symbol: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let coins = value.get("coins")?.as_array()?;
    let symbol = symbol.trim();
    let exact = coins.iter().find(|row| row.get("symbol").and_then(|item| item.as_str()).is_some_and(|sym| sym.eq_ignore_ascii_case(symbol)) && row.get("id").and_then(|item| item.as_str()).is_some_and(|id| id.eq_ignore_ascii_case("epic-cash") || !id.contains("chain")));
    let row = exact.or_else(|| coins.iter().find(|row| row.get("symbol").and_then(|item| item.as_str()).is_some_and(|sym| sym.eq_ignore_ascii_case(symbol))))?;
    row.get("id").and_then(|item| item.as_str()).map(|id| id.to_string())
}

pub fn paprika_search_id(body: &str, symbol: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let coins = value.get("currencies")?.as_array()?;
    let row = coins.iter().find(|row| row.get("symbol").and_then(|item| item.as_str()).is_some_and(|sym| sym.eq_ignore_ascii_case(symbol)))?;
    row.get("id").and_then(|item| item.as_str()).map(|id| id.to_string())
}

pub fn parse_paprika_eur(body: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    json_rate(value.get("quotes")?.get("EUR")?.get("price")?)
}

pub fn parse_kraken_last(body: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("error").and_then(|item| item.as_array()).is_some_and(|rows| !rows.is_empty()) {
        return None;
    }
    let result = value.get("result")?.as_object()?;
    let (_, row) = result.iter().next()?;
    json_rate(row.get("c")?.get(0)?)
}

pub fn parse_gold_api(body: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let price = json_rate(value.get("price")?)?;
    let currency = value.get("currency").and_then(|item| item.as_str()).unwrap_or("USD").to_ascii_uppercase();
    if price.is_finite() && price > 0.0 && currency.len() == 3 { Some((price, currency)) } else { None }
}

pub fn parse_atlantis_etf(body: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let close = value.get("etf")?.get("last_close")?;
    let price = json_rate(close.get("close")?)?;
    let currency = close.get("currency").and_then(|item| item.as_str()).unwrap_or("USD").to_ascii_uppercase();
    if price.is_finite() && price > 0.0 && currency.len() == 3 { Some((price, currency)) } else { None }
}

fn json_rate(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|text| text.replace(',', "").parse().ok()))
}

fn find_quote<'a>(book: &'a PriceBook, symbol: &str) -> Option<&'a Spot> {
    book.quotes.iter().find(|row| row.symbol.eq_ignore_ascii_case(symbol) && row.price.is_finite() && row.price > 0.0)
}

fn manual_row<'a>(book: &'a PriceBook, symbol: &str, override_live: bool) -> Option<&'a ManualPrice> {
    book.manual.iter().find(|row| row.override_live == override_live && row.symbol.eq_ignore_ascii_case(symbol) && row.price.is_finite() && row.price > 0.0)
}

fn priced_quote(quote: &Spot, asset: &AssetRef, now_ms: i64) -> Priced {
    Priced {
        price: quote.price,
        currency: quote.currency.clone(),
        source: if quote.estimated { "ai" } else { "cached" },
        provider: quote.source.clone(),
        fetched_ms: quote.fetched_ms,
        stale: !quote.estimated && quote_stale(quote.fetched_ms, now_ms),
        estimated: quote.estimated,
        unit_price: unit_major(quote.price, &asset.kind, &asset.holding_unit),
    }
}

fn priced_manual(row: &ManualPrice, asset: &AssetRef, _now_ms: i64) -> Priced {
    Priced {
        price: row.price,
        currency: row.currency.clone(),
        source: "manual",
        provider: "manual".into(),
        fetched_ms: 0,
        stale: false,
        estimated: false,
        unit_price: unit_major(row.price, &asset.kind, &asset.holding_unit),
    }
}

fn unit_major(price: f64, kind: &str, holding_unit: &str) -> f64 {
    if kind != "metal" {
        return price;
    }
    match holding_unit {
        "gram" => price / TROY_OZ_GRAMS,
        "kilogram" => price * 1000.0 / TROY_OZ_GRAMS,
        _ => price,
    }
}

/// Full decimal quantity times a unit price. The quantity string is not shortened.
pub fn quantity_times(qty: &str, unit_price: f64) -> Option<f64> {
    let qty = qty.trim().replace(',', ".");
    let value: f64 = qty.parse().ok()?;
    if !value.is_finite() || value <= 0.0 || !unit_price.is_finite() || unit_price <= 0.0 {
        return None;
    }
    let product = value * unit_price;
    if product.is_finite() && product > 0.0 { Some(product) } else { None }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HoldingPhrase {
    pub quantity: String,
    pub unit: String,
    pub query: String,
}

/// Pull a quantity, a unit, and the asset words out of a spoken holding.
/// Ounces of a metal are troy ounces. A missing unit on a coin stays `coin`.
pub fn parse_holding_phrase(text: &str) -> Option<HoldingPhrase> {
    let folded = fold(text);
    if folded.is_empty() {
        return None;
    }
    let mut quantity = String::new();
    let mut unit = String::new();
    let mut query = Vec::new();
    for token in folded.split(' ') {
        if matches!(token, "half" | "media" | "medio" | "meia" | "meio" | "mitja" | "mitat" | "halbe" | "halb") {
            if quantity.is_empty() {
                quantity = "0.5".into();
            }
            continue;
        }
        if let Some(number) = number_token(token) {
            if quantity.is_empty() || quantity == "0.5" {
                quantity = number;
            }
            continue;
        }
        if let Some(parsed) = unit_word(token) {
            if unit.is_empty() {
                unit = parsed.into();
            }
            continue;
        }
        if matches!(token, "of" | "de" | "del" | "da" | "do" | "dos" | "das" | "von" | "a" | "an" | "the" | "el" | "la" | "los" | "las" | "ein" | "eine" | "um" | "d" | "di" | "en" | "und" | "and" | "y" | "e" | "i") {
            continue;
        }
        query.push(token);
    }
    if quantity.is_empty() {
        return None;
    }
    let query = query.join(" ");
    if query.is_empty() && unit.is_empty() {
        return None;
    }
    if unit.is_empty() {
        unit = if alias(&query).is_some_and(|asset| asset.kind == "metal") {
            "troy_oz".into()
        } else if alias(&query).is_some_and(|asset| asset.kind == "stock") {
            "share".into()
        } else {
            "coin".into()
        };
    }
    Some(HoldingPhrase { quantity, unit, query })
}

fn number_token(token: &str) -> Option<String> {
    let normalized = if token.contains(',') && !token.contains('.') { token.replace(',', ".") } else { token.replace(',', "") };
    if normalized.is_empty() || !normalized.chars().all(|ch| ch.is_ascii_digit() || ch == '.') || normalized.matches('.').count() > 1 {
        return None;
    }
    let value: f64 = normalized.parse().ok()?;
    if value.is_finite() && value > 0.0 { Some(normalized) } else { None }
}

fn unit_word(token: &str) -> Option<&'static str> {
    Some(match token {
        "ounce" | "ounces" | "oz" | "ozt" | "unze" | "unzen" | "onza" | "onzas" | "onca" | "oncas" | "unca" | "unces" => "troy_oz",
        "kilogram" | "kilograms" | "kilogramm" | "kilo" | "kg" | "quilogram" | "quilograms" | "quilograma" | "quilogramas" => "kilogram",
        "gram" | "grams" | "gramo" | "gramos" | "gramm" => "gram",
        "share" | "shares" | "aktie" | "aktien" | "accion" | "acciones" | "acao" | "acoes" | "accio" | "accions" => "share",
        _ => return None,
    })
}

fn eur_price(price: f64, currency: &str, book: &FxBook) -> Option<f64> {
    let currency = currency.to_ascii_uppercase();
    if currency == "EUR" {
        return Some(price);
    }
    let per = if currency == "EUR" {
        Some(1.0)
    } else {
        book.per_eur.iter().find(|row| row.currency.eq_ignore_ascii_case(&currency) && row.per_eur > 0.0).map(|row| row.per_eur)
    };
    let per = per?;
    let eur = price / per;
    if eur.is_finite() && eur > 0.0 { Some(eur) } else { None }
}

pub fn metal_unit(text: &str) -> &'static str {
    let text = format!(" {} ", fold(text));
    if text.contains(" kilogram") || text.contains(" kilo") || text.contains(" kg") || text.contains(" quilogram") {
        "kilogram"
    } else if text.contains(" gram") || text.contains(" gramo") || text.contains(" gramm") {
        "gram"
    } else {
        "troy_oz"
    }
}

fn alias(raw: &str) -> Option<AssetRef> {
    let key = fold(raw);
    if key.is_empty() {
        return None;
    }
    let row = match key.as_str() {
        "btc" | "xbt" | "bitcoin" => ("BTC", "crypto", "bitcoin", "btc-bitcoin", "XXBTZEUR"),
        "eth" | "ethereum" => ("ETH", "crypto", "ethereum", "eth-ethereum", "XETHZEUR"),
        "xmr" | "monero" => ("XMR", "crypto", "monero", "xmr-monero", "XXMRZEUR"),
        "sol" | "solana" => ("SOL", "crypto", "solana", "sol-solana", "SOLEUR"),
        "ltc" | "litecoin" => ("LTC", "crypto", "litecoin", "ltc-litecoin", "XLTCZEUR"),
        "epic" | "epic cash" | "epiccash" | "epic private internet cash" => ("EPIC", "crypto", "epic-cash", "epic-epic-cash", ""),
        "zano" => ("ZANO", "crypto", "zano", "zano-zano", ""),
        "doge" | "dogecoin" => ("DOGE", "crypto", "dogecoin", "doge-dogecoin", ""),
        "ada" | "cardano" => ("ADA", "crypto", "cardano", "ada-cardano", ""),
        "gold" | "xau" | "au" | "oro" | "ouro" | "or" => ("XAU", "metal", "", "", ""),
        "silver" | "xag" | "ag" | "plata" | "prata" | "silber" => ("XAG", "metal", "", "", ""),
        "platinum" | "xpt" | "platino" | "platin" | "platina" | "plati" => ("XPT", "metal", "", "", ""),
        "palladium" | "xpd" | "paladio" | "pallad" | "paladi" => ("XPD", "metal", "", "", ""),
        "apple" | "aapl" => ("AAPL", "stock", "", "", ""),
        "tesla" | "tsla" => ("TSLA", "stock", "", "", ""),
        "microsoft" | "msft" => ("MSFT", "stock", "", "", ""),
        "nvidia" | "nvda" => ("NVDA", "stock", "", "", ""),
        "spy" | "spdr" => ("SPY", "stock", "", "", ""),
        "qqq" => ("QQQ", "stock", "", "", ""),
        "voo" => ("VOO", "stock", "", "", ""),
        "vti" => ("VTI", "stock", "", "", ""),
        "vwce" => ("VWCE", "stock", "", "", ""),
        _ => return None,
    };
    let holding_unit = if row.1 == "metal" { "troy_oz" } else if row.1 == "stock" { "share" } else { "coin" };
    Some(AssetRef {
        symbol: row.0.into(),
        kind: row.1.into(),
        holding_unit: holding_unit.into(),
        coingecko_id: row.2.into(),
        paprika_id: row.3.into(),
        kraken_pair: row.4.into(),
    })
}

fn crypto_ref(symbol: String, coingecko_id: String, paprika_id: String, kraken_pair: String) -> AssetRef {
    AssetRef { symbol, kind: "crypto".into(), holding_unit: "coin".into(), coingecko_id, paprika_id, kraken_pair }
}

fn stock_ref(symbol: String) -> AssetRef {
    AssetRef { symbol, kind: "stock".into(), holding_unit: "share".into(), coingecko_id: String::new(), paprika_id: String::new(), kraken_pair: String::new() }
}

fn ticker_token(raw: &str) -> Option<String> {
    let token = raw.trim().split_whitespace().next()?.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    if (1..=12).contains(&token.len()) && token.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        Some(token.to_ascii_uppercase())
    } else {
        None
    }
}

fn fold(value: &str) -> String {
    let lowered: String = value
        .trim()
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect();
    lowered.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn blocking_get(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(crate::http::rates_redirect())
        .build()
        .map_err(|_| "Could not refresh prices.".to_string())?;
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "GrokVoice")
        .header(reqwest::header::ACCEPT, "application/json, text/csv, */*")
        .send()
        .map_err(|_| "Could not refresh prices.".to_string())?
        .error_for_status()
        .map_err(|_| "Could not refresh prices.".to_string())?
        .text()
        .map_err(|_| "Could not refresh prices.".to_string())
}

fn enc(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(*byte as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xmr() -> AssetRef {
        alias("XMR").unwrap()
    }

    fn book_with(quote: Spot) -> PriceBook {
        PriceBook { quotes: vec![quote], manual: Vec::new(), disabled: Vec::new(), keys: PriceKeys::default() }
    }

    #[test]
    fn symbols_map_names_and_tickers_and_leave_unknown_words_alone() {
        let epic = resolve_holding("EPIC Cash", "", "EPIC", "crypto", "").unwrap();
        assert_eq!(epic.symbol, "EPIC");
        assert_eq!(epic.kind, "crypto");
        assert_eq!(epic.coingecko_id, "epic-cash");
        assert_eq!(epic.paprika_id, "epic-epic-cash");
        let xmr = resolve_holding("Monero", "XMR", "XMR", "crypto", "").unwrap();
        assert_eq!(xmr.symbol, "XMR");
        assert_eq!(xmr.coingecko_id, "monero");
        assert_eq!(xmr.kraken_pair, "XXMRZEUR");
        let gold = resolve_holding("gold", "XAU", "", "custom", "").unwrap();
        assert_eq!(gold.symbol, "XAU");
        assert_eq!(gold.holding_unit, "troy_oz");
        let grams = resolve_holding("gold", "", "", "custom", "10 grams").unwrap();
        assert_eq!(grams.holding_unit, "gram");
        assert_eq!(resolve_holding("silver", "", "", "custom", "").unwrap().symbol, "XAG");
        assert_eq!(resolve_holding("platinum", "", "", "", "").unwrap().symbol, "XPT");
        assert_eq!(resolve_holding("palladium", "", "", "", "").unwrap().symbol, "XPD");
        let spy = resolve_holding("S&P", "SPY", "USD", "fund", "").unwrap();
        assert_eq!((spy.symbol.as_str(), spy.kind.as_str()), ("SPY", "stock"));
        let apple = resolve_holding("Apple", "", "USD", "stock", "").unwrap();
        assert_eq!(apple.symbol, "AAPL");
        assert!(resolve_holding("Rent", "", "EUR", "custom", "").is_none());
        assert!(resolve_holding("Nitroeggs", "", "CHF", "custom", "").is_none());
        let unknown = resolve_holding("Kaspa", "KAS", "KAS", "crypto", "").unwrap();
        assert_eq!(unknown.symbol, "KAS");
        assert!(unknown.coingecko_id.is_empty());
    }

    #[test]
    fn providers_run_in_order_and_a_later_one_only_fills_gaps() {
        assert_eq!(CRYPTO_PROVIDERS, ["coingecko", "coinpaprika", "kraken"]);
        assert_eq!(METAL_PROVIDERS[0], "gold-api");
        assert_eq!(STOCK_PROVIDERS[0], "atlantis");
        let calls = std::sync::Mutex::new(Vec::new());
        let assets = vec![xmr(), alias("EPIC").unwrap(), alias("gold").unwrap(), alias("SPY").unwrap()];
        let book = refresh_book(&PriceBook::default(), &assets, 5_000, true, &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("ids=monero") || url.contains("ids=epic-cash,monero") || url.contains("ids=monero,epic-cash") {
                return Err("429".into());
            }
            if url.contains("api.coingecko.com") {
                return Err("429".into());
            }
            if url.contains("coinpaprika.com/v1/tickers/xmr-monero") {
                return Ok(r#"{"symbol":"XMR","quotes":{"EUR":{"price":400.0}}}"#.into());
            }
            if url.contains("coinpaprika.com/v1/tickers/epic-epic-cash") {
                return Err("miss".into());
            }
            if url.contains("kraken.com") && url.contains("XXMRZEUR") {
                return Ok(r#"{"error":[],"result":{"XXMRZEUR":{"c":["999.0","1"]}}}"#.into());
            }
            if url.contains("gold-api.com/price/XAU") {
                return Ok(r#"{"price":3200.0,"currency":"EUR","symbol":"XAU"}"#.into());
            }
            if url.contains("atlantisdatasolutions.com/v1/us/etfs/SPY") {
                return Ok(r#"{"etf":{"ticker":"SPY","last_close":{"close":"700.5","currency":"USD","date":"2026-11-19"}}}"#.into());
            }
            Err("unused".into())
        }, None);
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("coingecko")), "coingecko is first");
        assert!(seen.iter().any(|url| url.contains("coinpaprika")));
        let xmr_quote = book.quotes.iter().find(|row| row.symbol == "XMR").unwrap();
        assert_eq!(xmr_quote.price, 400.0);
        assert_eq!(xmr_quote.source, "coinpaprika");
        assert_ne!(xmr_quote.price, 999.0, "kraken does not replace a price paprika already filled");
        assert!(book.quotes.iter().any(|row| row.symbol == "XAU" && row.price == 3200.0 && row.source == "gold-api"));
        assert!(book.quotes.iter().any(|row| row.symbol == "SPY" && row.price == 700.5 && row.currency == "USD"));
        assert!(!book.quotes.iter().any(|row| row.symbol == "EPIC"));
    }

    #[test]
    fn a_total_miss_does_not_invent_a_price_and_ai_cannot_replace_an_api_quote() {
        let previous = book_with(Spot { symbol: "XMR".into(), kind: "crypto".into(), price: 150.0, currency: "EUR".into(), source: "coingecko".into(), fetched_ms: 1_000, estimated: false });
        let kept = refresh_book(&previous, &[xmr()], 9_000, true, &|_url| Err("down".into()), Some(&|_asset| Ok((1.0, "USD".into()))));
        let quote = kept.quotes.iter().find(|row| row.symbol == "XMR").unwrap();
        assert_eq!(quote.price, 150.0);
        assert!(!quote.estimated);
        assert_ne!(quote.price, 1.0);
        let epic = alias("EPIC").unwrap();
        let estimated = refresh_book(&PriceBook::default(), &[epic], 9_000, true, &|_url| Err("down".into()), Some(&|_asset| Ok((0.9, "eur".into()))));
        let epic_quote = estimated.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!(epic_quote.estimated);
        assert_eq!(epic_quote.source, "ai");
        assert_eq!(epic_quote.price, 0.9);
        assert!(parse_ai_price("I think about 12").is_err());
        assert!(parse_ai_price(r#"{"price":null,"currency":"USD"}"#).is_err());
        assert_eq!(parse_ai_price(r#"{"price":"408.5","currency":"eur"}"#).unwrap(), (408.5, "EUR".to_string()));
    }

    #[test]
    fn staleness_manual_override_and_conversion_follow_the_fx_order() {
        let now = 100_000_000_i64;
        let mut book = book_with(Spot { symbol: "XMR".into(), kind: "crypto".into(), price: 400.0, currency: "EUR".into(), source: "coingecko".into(), fetched_ms: now, estimated: false });
        let fresh = price_for(&book, &xmr(), now).unwrap();
        assert!(!fresh.stale);
        assert_eq!(fresh.unit_price, 400.0);
        assert_eq!(fresh.source, "cached");
        assert!(!needs_refresh(&book, &[xmr()], now, false));
        assert!(needs_refresh(&book, &[xmr()], now + PRICE_STALE_MS + 1, false));
        let stale = price_for(&book, &xmr(), now + PRICE_STALE_MS + 1).unwrap();
        assert!(stale.stale);
        assert_eq!(stale.unit_price, 400.0, "a stale cache is still a real price");
        upsert_manual(&mut book, "XMR", 100.0, "EUR", "2026-01-01", false).unwrap();
        let live = price_for(&book, &xmr(), now).unwrap();
        assert_eq!(live.unit_price, 400.0);
        assert_ne!(live.unit_price, 100.0);
        upsert_manual(&mut book, "XMR", 100.0, "EUR", "2026-01-01", true).unwrap();
        let over = price_for(&book, &xmr(), now).unwrap();
        assert_eq!(over.unit_price, 100.0);
        assert_eq!(over.source, "manual");
        let mut empty = PriceBook::default();
        assert!(price_for(&empty, &xmr(), now).is_none());
        upsert_manual(&mut empty, "XMR", 150.0, "EUR", "", false).unwrap();
        assert_eq!(price_for(&empty, &xmr(), now).unwrap().unit_price, 150.0);
        let gold = resolve_holding("gold", "", "", "custom", "10 grams").unwrap();
        assert_eq!(gold.holding_unit, "gram");
        let kilo = resolve_holding("gold", "", "", "custom", "2 kilograms").unwrap();
        assert_eq!(kilo.holding_unit, "kilogram");
        let mut kilos = PriceBook::default();
        kilos.quotes.push(Spot { symbol: "XAU".into(), kind: "metal".into(), price: TROY_OZ_GRAMS, currency: "EUR".into(), source: "gold-api".into(), fetched_ms: now, estimated: false });
        let kg = price_for(&kilos, &kilo, now).unwrap();
        assert!((kg.unit_price - 1000.0).abs() < 0.01);
        let mut metals = PriceBook::default();
        metals.quotes.push(Spot { symbol: "XAU".into(), kind: "metal".into(), price: TROY_OZ_GRAMS * 100.0, currency: "EUR".into(), source: "gold-api".into(), fetched_ms: now, estimated: false });
        let gram = price_for(&metals, &gold, now).unwrap();
        assert!((gram.unit_price - 100.0).abs() < 0.001);
        let one = major_to_minor(gram.unit_price, 2).unwrap();
        assert_eq!(one, 10_000);
        let mut fx = FxBook { per_eur: vec![finance_fx::FxLeg { currency: "USD".into(), per_eur: 1.1 }], ..FxBook::default() };
        let mut priced = PriceBook::default();
        priced.quotes.push(Spot { symbol: "XMR".into(), kind: "crypto".into(), price: 440.0, currency: "USD".into(), source: "coingecko".into(), fetched_ms: now, estimated: false });
        apply_crypto_legs(&mut fx, &priced);
        let leg = fx.per_eur.iter().find(|row| row.currency == "XMR").unwrap();
        let eur = 1.0 / leg.per_eur;
        assert!((eur - 400.0).abs() < 0.01, "440 USD at 1.1 USD per EUR is 400 EUR");
        priced.quotes.push(Spot { symbol: "EPIC".into(), kind: "crypto".into(), price: 1.0, currency: "EUR".into(), source: "ai".into(), fetched_ms: now, estimated: true });
        apply_crypto_legs(&mut fx, &priced);
        assert!(fx.per_eur.iter().all(|row| row.currency != "EPIC"), "an AI estimate does not become an FX leg");
    }

    #[test]
    fn phrases_keep_full_quantity_and_convert_units() {
        let silver = parse_holding_phrase("5 ounces of silver").unwrap();
        assert_eq!(silver.quantity, "5");
        assert_eq!(silver.unit, "troy_oz");
        assert_eq!(alias(&silver.query).unwrap().symbol, "XAG");
        let gold = parse_holding_phrase("10 grams of gold").unwrap();
        assert_eq!((gold.quantity.as_str(), gold.unit.as_str()), ("10", "gram"));
        let kilo = parse_holding_phrase("2 Kilogramm Gold").unwrap();
        assert_eq!(kilo.unit, "kilogram");
        assert_eq!(alias(&kilo.query).unwrap().symbol, "XAU");
        let tesla = parse_holding_phrase("2.5 shares of Tesla").unwrap();
        assert_eq!(tesla.quantity, "2.5");
        assert_eq!(tesla.unit, "share");
        assert_eq!(alias(&tesla.query).unwrap().symbol, "TSLA");
        let half = parse_holding_phrase("half a share of VWCE").unwrap();
        assert_eq!(half.quantity, "0.5");
        assert_eq!(half.unit, "share");
        assert_eq!(alias(&half.query).unwrap().symbol, "VWCE");
        assert_eq!(parse_holding_phrase("media acción de Tesla").unwrap().quantity, "0.5");
        assert_eq!(parse_holding_phrase("eine halbe Aktie VWCE").unwrap().unit, "share");
        assert_eq!(parse_holding_phrase("meia ação de VWCE").unwrap().quantity, "0.5");
        assert_eq!(parse_holding_phrase("mitja acció de VWCE").unwrap().unit, "share");
        let zano = parse_holding_phrase("16.46 ZANO").unwrap();
        assert_eq!(zano.quantity, "16.46");
        assert_eq!(zano.unit, "coin");
        assert_eq!(alias(&zano.query).unwrap().coingecko_id, "zano");
        let epic = parse_holding_phrase("128.60340395 EPIC cash").unwrap();
        assert_eq!(epic.quantity, "128.60340395");
        assert_eq!(alias(&epic.query).unwrap().symbol, "EPIC");
        let xmr = parse_holding_phrase("2.969859854727 XMR").unwrap();
        assert_eq!(xmr.quantity, "2.969859854727");
        assert_ne!(xmr.quantity, "2.96985985");
        let worth = quantity_times(&xmr.quantity, 400.0).unwrap();
        assert!((worth - 1187.9439418908).abs() < 0.0001);
        assert!((quantity_times("0.37", 100.0).unwrap() - 37.0).abs() < 0.0001);
        assert!((quantity_times("0.5", 200.0).unwrap() - 100.0).abs() < 0.0001);
        let five = quantity_times("5", 41.0).unwrap();
        assert!((major_to_minor(five, 2).unwrap() as f64 - 20500.0).abs() < 1.0);
    }

    #[test]
    fn parsers_reject_an_empty_body() {
        assert!(parse_coingecko_ids("{}").is_err());
        assert!(parse_paprika_eur(r#"{"quotes":{"USD":{"price":1}}}"#).is_none());
        assert!(parse_kraken_last(r#"{"error":["EGeneral"],"result":{}}"#).is_none());
        assert!(parse_gold_api("{}").is_none());
        assert!(parse_atlantis_etf(r#"{"etf":{"ticker":"SPY"}}"#).is_none());
        let ids = parse_coingecko_ids(r#"{"epic-cash":{"eur":0.9},"monero":{"eur":408.0}}"#).unwrap();
        assert!(ids.iter().any(|row| row.0 == "epic-cash" && row.1 == 0.9));
        assert!(!ids.iter().any(|row| row.0 == "solana"));
    }
}

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
/// CoinMarketCap has no keyless price route, so it is not called.
pub const CRYPTO_PROVIDERS: &[&str] = &["coingecko", "coinpaprika", "kraken", "cryptocompare", "tradeogre", "bitmart"];
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
    /// The name the user typed. Search prefers this over a colliding ticker.
    pub label: String,
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

/// A failed lookup. It is not a price and it does not satisfy the daily refresh gate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceMiss {
    pub symbol: String,
    pub fetched_ms: i64,
    #[serde(default)]
    pub reason: String,
}

/// One source tried for one symbol. `status` is `ok`, `miss`, `error`, or `skipped`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceAttempt {
    pub symbol: String,
    pub source: String,
    #[serde(default)]
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub detail: String,
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
    /// Failed lookups. Kept so the reason is visible. Never treated as a fresh price.
    #[serde(default)]
    pub misses: Vec<PriceMiss>,
    /// What the last refresh tried for each symbol. Not a price.
    #[serde(default)]
    pub attempts: Vec<PriceAttempt>,
    /// Short English sentence when the last pass left a symbol unpriced. Empty after a full success.
    #[serde(default)]
    pub last_error: String,
    /// Symbols that received a quote on the last pass.
    #[serde(default)]
    pub last_updated: u32,
    /// Symbols the last pass still could not price.
    #[serde(default)]
    pub last_failed: u32,
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
    // A miss, an estimate, or a missing quote always retries. Only a live quote from the last day waits.
    assets.iter().any(|asset| match find_quote(book, &asset.symbol) {
        Some(quote) if !quote.estimated && !quote_stale(quote.fetched_ms, now_ms) => false,
        _ => true,
    })
}

/// Map a holding's name, ticker, or currency onto a symbol. `None` means it stays manual.
pub fn resolve_holding(name: &str, ticker: &str, currency: &str, investment_type: &str, notes: &str) -> Option<AssetRef> {
    let unit = metal_unit(&format!("{name} {ticker} {notes}"));
    let label = if name.trim().is_empty() { ticker.trim().to_string() } else { name.trim().to_string() };
    // The typed name wins over a ticker saved by an older version. "EPIC Cash" stays EPIC even when the ticker is ETH.
    // "Epic Chain" must not inherit the EPIC Cash ids from a shared ticker.
    let named = alias(name).or_else(|| if epic_chain_name(name) { None } else { alias(ticker) }).or_else(|| alias(&label));
    if let Some(mut asset) = named {
        if asset.kind == "metal" {
            asset.holding_unit = unit.to_string();
        }
        asset.label = label;
        return Some(asset);
    }
    let kind = fold(investment_type);
    if kind == "stock" || kind == "fund" {
        let sym = ticker_token(ticker).or_else(|| ticker_token(name))?;
        if let Some(mut asset) = alias(&sym) {
            asset.label = label;
            return Some(asset);
        }
        if finance_fx::is_fiat(&sym) {
            return None;
        }
        let mut asset = stock_ref(sym);
        asset.label = label;
        return Some(asset);
    }
    if kind == "crypto" || finance_fx::is_crypto(currency) {
        let sym = ticker_token(ticker).or_else(|| ticker_token(currency)).or_else(|| ticker_token(name))?;
        if finance_fx::is_fiat(&sym) {
            return None;
        }
        if let Some(mut asset) = alias(&sym) {
            if !(asset.symbol == "EPIC" && epic_chain_name(name)) {
                asset.label = label;
                return Some(asset);
            }
        }
        let mut asset = crypto_ref(sym, String::new(), String::new(), String::new());
        asset.label = label;
        return Some(asset);
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
    ai: Option<&dyn Fn(&AssetRef) -> Result<AiEstimate, String>>,
    prefer: &str,
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
        next.last_updated = 0;
        next.last_failed = 0;
        if assets.is_empty() {
            next.last_error = "No holdings to price.".into();
        }
        return next;
    }
    let pending_symbols: Vec<String> = pending.iter().map(|asset| asset.symbol.clone()).collect();
    next.attempts.retain(|row| !pending_symbols.iter().any(|symbol| symbol.eq_ignore_ascii_case(&row.symbol)));
    let mut open: Vec<&AssetRef> = pending.clone();
    fill_crypto(&mut next, &open, now_ms, get, prefer);
    open.retain(|asset| asset.kind == "crypto" && find_quote(&next, &asset.symbol).map(|quote| quote.estimated || quote_stale(quote.fetched_ms, now_ms) && force).unwrap_or(true) && !(find_quote(&next, &asset.symbol).is_some_and(|quote| !quote.estimated && quote.fetched_ms == now_ms)));
    // Recompute the still-missing set from what this pass stored.
    open = pending.iter().copied().filter(|asset| !fresh_api(&next, asset, now_ms)).collect();
    fill_metals(&mut next, &open, now_ms, get);
    open = pending.iter().copied().filter(|asset| !fresh_api(&next, asset, now_ms)).collect();
    fill_stocks(&mut next, &open, now_ms, get);
    if let Some(ai) = ai {
        for asset in &pending {
            if fresh_api(&next, asset, now_ms) || find_quote(&next, &asset.symbol).is_some_and(|quote| !quote.estimated) {
                continue;
            }
            // A fresh estimate is enough until the next day, unless this pass was forced.
            if !force && find_quote(&next, &asset.symbol).is_some_and(|quote| quote.estimated && !quote_stale(quote.fetched_ms, now_ms)) {
                continue;
            }
            match ai(asset) {
                Ok(estimate) if estimate.price.is_finite() && estimate.price > 0.0 && estimate.currency.len() == 3 => {
                    put_quote(
                        &mut next,
                        Spot {
                            symbol: asset.symbol.clone(),
                            kind: asset.kind.clone(),
                            price: estimate.price,
                            currency: estimate.currency.to_ascii_uppercase(),
                            source: "ai".into(),
                            fetched_ms: now_ms,
                            estimated: true,
                        },
                    );
                    let detail = if estimate.source_url.trim().is_empty() { "AI estimate".to_string() } else { estimate.source_url.trim().to_string() };
                    note_attempt(&mut next, &asset.symbol, "ai", &asset.label, "ok", &detail);
                }
                Ok(_) => {
                    note_attempt(&mut next, &asset.symbol, "ai", &asset.label, "miss", "The estimate had no price.");
                    log::warn!("price estimate for {} had no usable number", asset.symbol);
                }
                Err(err) => {
                    note_attempt(&mut next, &asset.symbol, "ai", &asset.label, "error", &err);
                    log::warn!("price estimate failed for {}: {err}", asset.symbol);
                }
            }
        }
    } else {
        for asset in &pending {
            if fresh_api(&next, asset, now_ms) || find_quote(&next, &asset.symbol).is_some_and(|quote| !quote.estimated) {
                continue;
            }
            note_attempt(&mut next, &asset.symbol, "ai", &asset.label, "skipped", "No estimate was asked.");
        }
    }
    let mut failed = 0u32;
    let mut updated = 0u32;
    let mut reasons = Vec::new();
    for asset in &pending {
        let fresh = find_quote(&next, &asset.symbol).is_some_and(|quote| quote.fetched_ms == now_ms);
        if fresh {
            updated += 1;
            next.misses.retain(|row| !row.symbol.eq_ignore_ascii_case(&asset.symbol));
            continue;
        }
        if find_quote(&next, &asset.symbol).is_some() && !force {
            next.misses.retain(|row| !row.symbol.eq_ignore_ascii_case(&asset.symbol));
            continue;
        }
        failed += 1;
        let reason = next
            .attempts
            .iter()
            .rev()
            .find(|row| row.symbol.eq_ignore_ascii_case(&asset.symbol) && row.status != "ok" && row.status != "skipped")
            .map(|row| format!("{}: {}", row.source, row.detail))
            .unwrap_or_else(|| "Could not refresh the price.".into());
        reasons.push(format!("{} ({reason})", asset.symbol));
        if let Some(row) = next.misses.iter_mut().find(|row| row.symbol.eq_ignore_ascii_case(&asset.symbol)) {
            row.fetched_ms = now_ms;
            row.reason = reason;
        } else {
            next.misses.push(PriceMiss { symbol: asset.symbol.clone(), fetched_ms: now_ms, reason });
        }
        log::warn!("no live price for {}", asset.symbol);
    }
    next.last_updated = updated;
    next.last_failed = failed;
    if failed > 0 {
        let joined = reasons.join("; ");
        next.last_error = if joined.chars().count() > 280 { format!("{}…", joined.chars().take(279).collect::<String>()) } else { joined };
    } else {
        next.last_error.clear();
    }
    next
}

fn fresh_api(book: &PriceBook, asset: &AssetRef, now_ms: i64) -> bool {
    find_quote(book, &asset.symbol).is_some_and(|quote| !quote.estimated && quote.fetched_ms == now_ms)
}

fn fill_crypto(book: &mut PriceBook, assets: &[&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>, prefer: &str) {
    let cryptos: Vec<&&AssetRef> = assets.iter().filter(|asset| asset.kind == "crypto").collect();
    if cryptos.is_empty() {
        return;
    }
    if provider_enabled(book, "coingecko") {
        let ids: Vec<&str> = cryptos.iter().filter_map(|asset| (!asset.coingecko_id.is_empty()).then_some(asset.coingecko_id.as_str())).collect();
        if !ids.is_empty() {
            let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}", ids.join(","), vs_currencies(prefer));
            match get_logged(get, &url, "crypto") {
                Ok(body) => match parse_coingecko_priced_prefer(&body, prefer) {
                    Ok(rows) => {
                        for asset in &cryptos {
                            if let Some((price, currency)) = rows.iter().find(|(id, _, _)| id.eq_ignore_ascii_case(&asset.coingecko_id)).map(|(_, price, currency)| (*price, currency.as_str())) {
                                put_api(book, asset, price, currency, "coingecko", now_ms);
                                note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "ok", "Price received");
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!("price parse failed for coingecko batch: {err}");
                        for asset in &cryptos {
                            if !asset.coingecko_id.is_empty() {
                                note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "miss", &err);
                            }
                        }
                    }
                },
                Err(err) => {
                    for asset in &cryptos {
                        if !asset.coingecko_id.is_empty() {
                            note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "error", &err);
                        }
                    }
                }
            }
        }
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) {
                continue;
            }
            // The batch can fail or omit one id. Ask for that id on its own before searching.
            if !asset.coingecko_id.is_empty() {
                let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}", enc(&asset.coingecko_id), vs_currencies(prefer));
                match get_logged(get, &url, &asset.symbol) {
                    Ok(body) => match parse_coingecko_priced_prefer(&body, prefer) {
                        Ok(rows) => {
                            if let Some((price, currency)) = rows.into_iter().find(|(id, _, _)| id.eq_ignore_ascii_case(&asset.coingecko_id)).map(|(_, price, currency)| (price, currency)) {
                                put_api(book, asset, price, &currency, "coingecko", now_ms);
                                note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "ok", "Price received");
                                continue;
                            }
                            note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "miss", "No price in the response");
                        }
                        Err(err) => note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "miss", &err),
                    },
                    Err(err) => note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "error", &err),
                }
            } else {
                note_attempt(book, &asset.symbol, "coingecko", "", "skipped", "No id is saved.");
            }
            if gecko_by_search(book, asset, now_ms, get, prefer) {
                continue;
            }
            // symbols= is last. It can answer a different coin that shares the ticker.
            // EPIC still tries it after a failed id, because CoinGecko lists Epic Cash under that ticker.
            if asset.coingecko_id.is_empty() || asset.symbol.eq_ignore_ascii_case("EPIC") {
                let query = asset.symbol.to_ascii_lowercase();
                let url = format!("https://api.coingecko.com/api/v3/simple/price?symbols={query}&vs_currencies={}", vs_currencies(prefer));
                match get_logged(get, &url, &asset.symbol) {
                    Ok(body) => match parse_coingecko_priced_prefer(&body, prefer) {
                        Ok(rows) => {
                            if let Some((price, currency)) = rows.into_iter().find(|(sym, _, _)| sym.eq_ignore_ascii_case(&asset.symbol)).map(|(_, price, currency)| (price, currency)) {
                                put_api(book, asset, price, &currency, "coingecko", now_ms);
                                note_attempt(book, &asset.symbol, "coingecko", &query, "ok", "Price received");
                            } else {
                                note_attempt(book, &asset.symbol, "coingecko", &query, "miss", "No price in the response");
                            }
                        }
                        Err(err) => note_attempt(book, &asset.symbol, "coingecko", &query, "miss", &err),
                    },
                    Err(err) => note_attempt(book, &asset.symbol, "coingecko", &query, "error", &err),
                }
            }
        }
    } else {
        for asset in &cryptos {
            note_attempt(book, &asset.symbol, "coingecko", &asset.coingecko_id, "skipped", "Turned off");
        }
    }
    if provider_enabled(book, "coinpaprika") {
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) {
                continue;
            }
            if !asset.paprika_id.is_empty() {
                if paprika_price(book, asset, &asset.paprika_id, now_ms, get, prefer) {
                    continue;
                }
            } else {
                note_attempt(book, &asset.symbol, "coinpaprika", "", "skipped", "No id is saved.");
            }
            let mut found = false;
            for query in search_queries(asset) {
                let url = format!("https://api.coinpaprika.com/v1/search/?q={}&c=currencies&limit=8", enc(&query));
                let Ok(body) = get_logged(get, &url, &asset.symbol) else {
                    note_attempt(book, &asset.symbol, "coinpaprika", &query, "error", "Could not refresh the price.");
                    continue;
                };
                let Some(id) = paprika_search_id(&body, &asset.symbol, &asset.label) else {
                    note_attempt(book, &asset.symbol, "coinpaprika", &query, "miss", "No price in the response");
                    continue;
                };
                if paprika_price(book, asset, &id, now_ms, get, prefer) {
                    found = true;
                    break;
                }
            }
            if !found && asset.paprika_id.is_empty() {
                note_attempt(book, &asset.symbol, "coinpaprika", "", "miss", "No price in the response");
            }
        }
    } else {
        for asset in &cryptos {
            note_attempt(book, &asset.symbol, "coinpaprika", &asset.paprika_id, "skipped", "Turned off");
        }
    }
    if provider_enabled(book, "kraken") {
        for asset in &cryptos {
            if fresh_api(book, asset, now_ms) {
                continue;
            }
            if asset.kraken_pair.is_empty() {
                note_attempt(book, &asset.symbol, "kraken", "", "skipped", "No id is saved.");
                continue;
            }
            let url = format!("https://api.kraken.com/0/public/Ticker?pair={}", asset.kraken_pair);
            match get_logged(get, &url, &asset.symbol) {
                Ok(body) => {
                    if let Some(price) = parse_kraken_last(&body) {
                        put_api(book, asset, price, "EUR", "kraken", now_ms);
                        note_attempt(book, &asset.symbol, "kraken", &asset.kraken_pair, "ok", "Price received");
                    } else {
                        note_attempt(book, &asset.symbol, "kraken", &asset.kraken_pair, "miss", "No price in the response");
                    }
                }
                Err(err) => note_attempt(book, &asset.symbol, "kraken", &asset.kraken_pair, "error", &err),
            }
        }
    } else {
        for asset in &cryptos {
            note_attempt(book, &asset.symbol, "kraken", &asset.kraken_pair, "skipped", "Turned off");
        }
    }
    fill_public_ticker(book, &cryptos, now_ms, get, prefer);
}

/// Keyless public books after the named providers. USDT is treated as one US dollar.
fn fill_public_ticker(book: &mut PriceBook, assets: &[&&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>, prefer: &str) {
    for asset in assets {
        if fresh_api(book, asset, now_ms) {
            continue;
        }
        let sym = asset.symbol.to_ascii_uppercase();
        if provider_enabled(book, "cryptocompare") {
            let url = format!("https://min-api.cryptocompare.com/data/price?fsym={sym}&tsyms=USD,EUR,CHF");
            match get_logged(get, &url, &asset.symbol) {
                Ok(body) => {
                    if let Some((price, currency)) = parse_cryptocompare(&body, prefer) {
                        put_api(book, asset, price, &currency, "cryptocompare", now_ms);
                        note_attempt(book, &asset.symbol, "cryptocompare", &sym, "ok", "Price received");
                        continue;
                    }
                    note_attempt(book, &asset.symbol, "cryptocompare", &sym, "miss", "No price in the response");
                }
                Err(err) => note_attempt(book, &asset.symbol, "cryptocompare", &sym, "error", &err),
            }
        }
        if fresh_api(book, asset, now_ms) {
            continue;
        }
        if provider_enabled(book, "tradeogre") {
            let url = format!("https://tradeogre.com/api/v1/ticker/{sym}-USDT");
            match get_logged(get, &url, &asset.symbol) {
                Ok(body) => {
                    if let Some(price) = parse_tradeogre(&body) {
                        put_api(book, asset, price, "USD", "tradeogre", now_ms);
                        note_attempt(book, &asset.symbol, "tradeogre", &format!("{sym}-USDT"), "ok", "Price received");
                        continue;
                    }
                    note_attempt(book, &asset.symbol, "tradeogre", &format!("{sym}-USDT"), "miss", "No price in the response");
                }
                Err(err) => note_attempt(book, &asset.symbol, "tradeogre", &format!("{sym}-USDT"), "error", &err),
            }
        }
        if fresh_api(book, asset, now_ms) {
            continue;
        }
        if provider_enabled(book, "bitmart") {
            let url = format!("https://api-cloud.bitmart.com/spot/quotation/v3/ticker?symbol={sym}_USDT");
            match get_logged(get, &url, &asset.symbol) {
                Ok(body) => {
                    if let Some(price) = parse_bitmart(&body) {
                        put_api(book, asset, price, "USD", "bitmart", now_ms);
                        note_attempt(book, &asset.symbol, "bitmart", &format!("{sym}_USDT"), "ok", "Price received");
                        continue;
                    }
                    note_attempt(book, &asset.symbol, "bitmart", &format!("{sym}_USDT"), "miss", "No price in the response");
                }
                Err(err) => note_attempt(book, &asset.symbol, "bitmart", &format!("{sym}_USDT"), "error", &err),
            }
        }
    }
}

/// CoinGecko's search for the full typed name "EPIC Cash" returns no coins.
/// The ticker query lists Epic Private Internet Cash as `epic-cash`. Try the name, then the ticker.
fn gecko_by_search(book: &mut PriceBook, asset: &AssetRef, now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>, prefer: &str) -> bool {
    for query in search_queries(asset) {
        let search = format!("https://api.coingecko.com/api/v3/search?query={}", enc(&query));
        let Ok(body) = get_logged(get, &search, &asset.symbol) else { continue };
        let Some(id) = coingecko_search_id(&body, &asset.symbol, &asset.label) else { continue };
        let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}", enc(&id), vs_currencies(prefer));
        if let Ok(body) = get_logged(get, &url, &asset.symbol) {
            if let Ok(rows) = parse_coingecko_priced_prefer(&body, prefer) {
                if let Some((price, currency)) = rows.into_iter().find(|(row, _, _)| row.eq_ignore_ascii_case(&id)).map(|(_, price, currency)| (price, currency)) {
                    put_api(book, asset, price, &currency, "coingecko", now_ms);
                    note_attempt(book, &asset.symbol, "coingecko", &id, "ok", "Price received");
                    return true;
                }
                note_attempt(book, &asset.symbol, "coingecko", &id, "miss", "No price in the response");
            }
        }
    }
    false
}

fn paprika_price(book: &mut PriceBook, asset: &AssetRef, id: &str, now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>, prefer: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let mut quotes = vec!["EUR".to_string(), "USD".to_string()];
    let prefer = prefer.trim().to_ascii_uppercase();
    if prefer.len() == 3 && prefer.chars().all(|ch| ch.is_ascii_alphabetic()) {
        quotes.retain(|code| code != &prefer);
        quotes.insert(0, prefer);
    }
    for quote in quotes {
        let url = format!("https://api.coinpaprika.com/v1/tickers/{id}?quotes={quote}");
        let quote = quote.as_str();
        let Ok(body) = get_logged(get, &url, &asset.symbol) else {
            note_attempt(book, &asset.symbol, "coinpaprika", id, "error", "Could not refresh the price.");
            continue;
        };
        let Some(price) = parse_paprika_quote(&body, quote) else {
            note_attempt(book, &asset.symbol, "coinpaprika", id, "miss", "No price in the response");
            continue;
        };
        put_api(book, asset, price, quote, "coinpaprika", now_ms);
        note_attempt(book, &asset.symbol, "coinpaprika", id, "ok", "Price received");
        return true;
    }
    false
}

fn search_queries(asset: &AssetRef) -> Vec<String> {
    let mut queries = Vec::new();
    let label = asset.label.trim();
    let symbol = asset.symbol.trim();
    if !label.is_empty() {
        queries.push(label.to_string());
    }
    if !symbol.is_empty() && !queries.iter().any(|query| query.eq_ignore_ascii_case(symbol)) {
        queries.push(symbol.to_string());
    }
    queries
}

fn get_logged(get: &dyn Fn(&str) -> Result<String, String>, url: &str, symbol: &str) -> Result<String, String> {
    match get(url) {
        Ok(body) => Ok(body),
        Err(err) => {
            log::warn!("price fetch failed for {symbol} from {}: {err}", host_of(url));
            Err(err)
        }
    }
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|url| url.host_str().map(|host| host.to_string())).unwrap_or_else(|| "price host".into())
}

fn fill_metals(book: &mut PriceBook, assets: &[&AssetRef], now_ms: i64, get: &dyn Fn(&str) -> Result<String, String>) {
    if !provider_enabled(book, "gold-api") {
        return;
    }
    for asset in assets.iter().filter(|asset| asset.kind == "metal") {
        let url = format!("https://api.gold-api.com/price/{}/EUR", asset.symbol);
        if let Ok(body) = get_logged(get, &url, &asset.symbol) {
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
            if let Ok(body) = get_logged(get, &url, &asset.symbol) {
                if let Some((price, currency)) = parse_atlantis_etf(&body) {
                    put_api(book, asset, price, &currency, "atlantis", now_ms);
                }
            }
        }
        if provider_enabled(book, "finnhub") && !book.keys.finnhub.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://finnhub.io/api/v1/quote?symbol={}&token={}", enc(&asset.symbol), enc(book.keys.finnhub.trim()));
            if let Ok(body) = get_logged(get, &url, &asset.symbol) {
                if let Some((price, currency)) = parse_finnhub_quote(&body) {
                    put_api(book, asset, price, &currency, "finnhub", now_ms);
                }
            }
        }
        if provider_enabled(book, "alphavantage") && !book.keys.alphavantage.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={}&apikey={}", enc(&asset.symbol), enc(book.keys.alphavantage.trim()));
            if let Ok(body) = get_logged(get, &url, &asset.symbol) {
                if let Some((price, currency)) = parse_alpha_quote(&body) {
                    put_api(book, asset, price, &currency, "alphavantage", now_ms);
                }
            }
        }
        if provider_enabled(book, "twelvedata") && !book.keys.twelvedata.trim().is_empty() && !fresh_api(book, asset, now_ms) {
            let url = format!("https://api.twelvedata.com/quote?symbol={}&apikey={}", enc(&asset.symbol), enc(book.keys.twelvedata.trim()));
            if let Ok(body) = get_logged(get, &url, &asset.symbol) {
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

fn note_attempt(book: &mut PriceBook, symbol: &str, source: &str, id: &str, status: &str, detail: &str) {
    book.attempts.push(PriceAttempt {
        symbol: symbol.to_string(),
        source: source.to_string(),
        id: id.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
    });
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

#[derive(Clone, Debug, PartialEq)]
pub struct AiEstimate {
    pub price: f64,
    pub currency: String,
    pub source_url: String,
    pub as_of: String,
}

impl AiEstimate {
    pub fn priced(price: f64, currency: impl Into<String>) -> Self {
        Self { price, currency: currency.into(), source_url: String::new(), as_of: String::new() }
    }
}

/// Accepts `{"price_usd","source_url","as_of"}` and the older `{"price","currency"}` shape.
pub fn parse_ai_price(text: &str) -> Result<AiEstimate, String> {
    let trimmed = text.trim();
    let start = trimmed.find('{').ok_or("The estimate had no price.")?;
    let end = trimmed.rfind('}').ok_or("The estimate had no price.")?;
    let value: serde_json::Value = serde_json::from_str(&trimmed[start..=end]).map_err(|_| "The estimate had no price.".to_string())?;
    let source_url = value.get("source_url").and_then(|item| item.as_str()).unwrap_or("").trim().to_string();
    let as_of = value.get("as_of").and_then(|item| item.as_str()).unwrap_or("").trim().to_string();
    if let Some(price) = value.get("price_usd").and_then(json_rate) {
        if price.is_finite() && price > 0.0 {
            return Ok(AiEstimate { price, currency: "USD".into(), source_url, as_of });
        }
        return Err("The estimate had no price.".into());
    }
    if value.get("price").is_some_and(|price| price.is_null()) || value.get("price_usd").is_some_and(|price| price.is_null()) {
        return Err("The estimate had no price.".into());
    }
    let price = value.get("price").and_then(json_rate).ok_or("The estimate had no price.")?;
    let currency = value.get("currency").and_then(|item| item.as_str()).unwrap_or("").trim().to_ascii_uppercase();
    if !price.is_finite() || price <= 0.0 || currency.len() != 3 || !currency.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err("The estimate had no price.".into());
    }
    Ok(AiEstimate { price, currency, source_url, as_of })
}

pub fn parse_cryptocompare(body: &str, prefer: &str) -> Option<(f64, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("Response").and_then(|item| item.as_str()).is_some_and(|item| item.eq_ignore_ascii_case("Error")) {
        return None;
    }
    pick_vs(&value, prefer)
}

pub fn parse_tradeogre(body: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("success").and_then(|item| item.as_bool()) == Some(false) {
        return None;
    }
    json_rate(value.get("price")?).filter(|price| price.is_finite() && *price > 0.0)
}

pub fn parse_bitmart(body: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = value.get("data").unwrap_or(&value);
    json_rate(data.get("last").or_else(|| data.get("last_price")).or_else(|| data.get("close"))?).filter(|price| price.is_finite() && *price > 0.0)
}

/// EUR when the provider has it, otherwise USD, then the caller's currency. Either is a real quote.
pub fn parse_coingecko_priced(body: &str) -> Result<Vec<(String, f64, String)>, String> {
    parse_coingecko_priced_prefer(body, "")
}

pub fn parse_coingecko_priced_prefer(body: &str, prefer: &str) -> Result<Vec<(String, f64, String)>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The price source did not answer.".to_string())?;
    let Some(map) = value.as_object() else {
        return Err("The price source did not answer.".into());
    };
    let mut rows = Vec::new();
    for (id, row) in map {
        if let Some((price, currency)) = pick_vs(row, prefer) {
            rows.push((id.clone(), price, currency));
        }
    }
    if rows.is_empty() {
        return Err("The price source did not answer.".into());
    }
    Ok(rows)
}

fn vs_currencies(prefer: &str) -> String {
    let mut codes = vec!["eur".to_string(), "usd".to_string(), "chf".to_string(), "gbp".to_string()];
    let prefer = prefer.trim().to_ascii_lowercase();
    if prefer.len() == 3 && prefer.chars().all(|ch| ch.is_ascii_alphabetic()) && !codes.iter().any(|code| code == &prefer) {
        codes.insert(0, prefer);
    } else if prefer.len() == 3 {
        codes.retain(|code| code != &prefer);
        codes.insert(0, prefer);
    }
    codes.into_iter().take(4).collect::<Vec<_>>().join(",")
}

fn pick_vs(row: &serde_json::Value, prefer: &str) -> Option<(f64, String)> {
    let prefer = prefer.trim().to_ascii_lowercase();
    let mut order = vec!["eur".to_string(), "usd".to_string(), "chf".to_string(), "gbp".to_string()];
    if prefer.len() == 3 {
        order.retain(|code| code != &prefer);
        order.insert(0, prefer);
    }
    for code in &order {
        if let Some(price) = row.get(code).and_then(json_rate).filter(|price| price.is_finite() && *price > 0.0) {
            return Some((price, code.to_ascii_uppercase()));
        }
    }
    let object = row.as_object()?;
    for (key, value) in object {
        if key.len() == 3 && key.chars().all(|ch| ch.is_ascii_alphabetic()) {
            if let Some(price) = json_rate(value).filter(|price| price.is_finite() && *price > 0.0) {
                return Some((price, key.to_ascii_uppercase()));
            }
        }
    }
    None
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

pub fn coingecko_search_id(body: &str, symbol: &str, name: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let coins = value.get("coins")?.as_array()?;
    best_coin(coins, symbol, name)
}

pub fn paprika_search_id(body: &str, symbol: &str, name: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let coins = value.get("currencies")?.as_array()?;
    best_coin(coins, symbol, name)
}

/// Prefer the coin whose name matches what the user typed when several share a ticker.
fn best_coin(coins: &[serde_json::Value], symbol: &str, name: &str) -> Option<String> {
    let want_symbol = symbol.trim();
    let want_name = fold(name);
    let mut best: Option<(i32, String)> = None;
    for row in coins {
        let sym = row.get("symbol").and_then(|item| item.as_str()).unwrap_or("");
        let id = row.get("id").and_then(|item| item.as_str()).unwrap_or("");
        let coin_name = fold(row.get("name").and_then(|item| item.as_str()).unwrap_or(""));
        if id.is_empty() {
            continue;
        }
        let symbol_ok = !want_symbol.is_empty() && sym.eq_ignore_ascii_case(want_symbol);
        let name_ok = !want_name.is_empty() && (coin_name == want_name || coin_name.contains(&want_name) || want_name.split(' ').all(|token| !token.is_empty() && coin_name.contains(token)));
        if !symbol_ok && !name_ok {
            continue;
        }
        let mut score = 0;
        if symbol_ok {
            score += 20;
        }
        if !want_name.is_empty() && coin_name == want_name {
            score += 80;
        }
        if !want_name.is_empty() {
            for token in want_name.split(' ') {
                if !token.is_empty() && coin_name.contains(token) {
                    score += 15;
                }
            }
        }
        if id.eq_ignore_ascii_case("epic-cash") && want_name.contains("cash") {
            score += 25;
        }
        if id.contains("chain") && !want_name.contains("chain") {
            score -= 40;
        }
        if best.as_ref().map(|(have, _)| score > *have).unwrap_or(true) {
            best = Some((score, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

pub fn parse_paprika_eur(body: &str) -> Option<f64> {
    parse_paprika_quote(body, "EUR")
}

pub fn parse_paprika_quote(body: &str, quote: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    json_rate(value.get("quotes")?.get(quote)?.get("price")?)
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

/// First decimal in `text`, including a phrase such as "128.60340395 EPIC Cash".
pub fn first_quantity(text: &str) -> Option<String> {
    if let Some(parsed) = parse_holding_phrase(text) {
        return Some(parsed.quantity);
    }
    for token in fold(text).split_whitespace() {
        if let Some(number) = number_token(token) {
            return Some(number);
        }
    }
    None
}

fn alias(raw: &str) -> Option<AssetRef> {
    let key = fold(raw);
    if key.is_empty() {
        return None;
    }
    if let Some(asset) = alias_exact(&key) {
        return Some(asset);
    }
    let stripped = fold(&key.chars().filter(|ch| ch.is_ascii_alphanumeric() || ch.is_whitespace()).collect::<String>());
    if stripped != key {
        if let Some(asset) = alias_exact(&stripped) {
            return Some(asset);
        }
    }
    epic_phrase(&stripped)
}

/// "EPIC Cash", "my EPIC Cash wallet", and "EPIC-Cash." are Epic Cash.
/// "Epic Chain" is a different coin and stays unmatched.
fn epic_chain_name(name: &str) -> bool {
    let folded = fold(name);
    let tokens: Vec<&str> = folded.split_whitespace().collect();
    tokens.iter().any(|token| *token == "chain") && !tokens.iter().any(|token| *token == "cash" || *token == "epiccash")
}

fn epic_phrase(key: &str) -> Option<AssetRef> {
    let tokens: Vec<&str> = key.split_whitespace().filter(|token| !token.is_empty()).collect();
    let cash = tokens.iter().any(|token| *token == "cash" || *token == "epiccash");
    let chain = tokens.iter().any(|token| *token == "chain");
    if chain && !cash {
        return None;
    }
    let epic = tokens.iter().any(|token| *token == "epic" || *token == "epiccash");
    if epic && cash { alias_exact("epic cash") } else { None }
}

fn alias_exact(key: &str) -> Option<AssetRef> {
    let row = match key {
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
        label: String::new(),
    })
}

fn crypto_ref(symbol: String, coingecko_id: String, paprika_id: String, kraken_pair: String) -> AssetRef {
    AssetRef { symbol, kind: "crypto".into(), holding_unit: "coin".into(), coingecko_id, paprika_id, kraken_pair, label: String::new() }
}

fn stock_ref(symbol: String) -> AssetRef {
    AssetRef { symbol, kind: "stock".into(), holding_unit: "share".into(), coingecko_id: String::new(), paprika_id: String::new(), kraken_pair: String::new(), label: String::new() }
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
            '-' | '_' | '/' => ' ',
            other => other.to_ascii_lowercase(),
        })
        .collect();
    lowered.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn blocking_get(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .connect_timeout(std::time::Duration::from_secs(10))
        // HTTP/2 through rustls fails on some Android builds. HTTP/1.1 reaches the same hosts.
        .http1_only()
        .redirect(crate::http::rates_redirect())
        .build()
        .map_err(|err| format!("Could not refresh prices. {err}"))?;
    let host = host_of(url);
    let mut last = String::new();
    for _ in 0..2 {
        match client
            .get(url)
            .header(reqwest::header::USER_AGENT, concat!("GrokVoice/", env!("CARGO_PKG_VERSION")))
            .header(reqwest::header::ACCEPT, "application/json, text/csv, */*")
            .send()
        {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    last = format!("HTTP {status} from {host}");
                    continue;
                }
                return response.text().map_err(|err| format!("Could not read {host}: {err}"));
            }
            Err(err) => last = format!("Could not reach {host}: {err}"),
        }
    }
    Err(if last.is_empty() { "Could not refresh prices.".into() } else { last })
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
        PriceBook { quotes: vec![quote], ..PriceBook::default() }
    }

    #[test]
    fn symbols_map_names_and_tickers_and_leave_unknown_words_alone() {
        let epic = resolve_holding("EPIC Cash", "", "EPIC", "crypto", "").unwrap();
        assert_eq!(epic.symbol, "EPIC");
        assert_eq!(epic.kind, "crypto");
        assert_eq!(epic.coingecko_id, "epic-cash");
        assert_eq!(epic.paprika_id, "epic-epic-cash");
        assert_eq!(resolve_holding("Epic Cash", "", "CHF", "other", "").unwrap().coingecko_id, "epic-cash");
        assert_eq!(resolve_holding("EPIC-Cash", "", "CHF", "custom", "").unwrap().symbol, "EPIC");
        assert_eq!(resolve_holding("  Epic   Cash. ", "", "CHF", "custom", "").unwrap().coingecko_id, "epic-cash");
        assert_eq!(resolve_holding("my EPIC Cash wallet", "", "CHF", "custom", "").unwrap().symbol, "EPIC");
        assert!(resolve_holding("Epic Chain", "EPIC", "CHF", "crypto", "").unwrap().coingecko_id.is_empty(), "Epic Chain does not inherit the Epic Cash id");
        let named = resolve_holding("EPIC Cash", "ETH", "CHF", "custom", "").unwrap();
        assert_eq!(named.symbol, "EPIC", "the typed name wins over a ticker saved earlier");
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
        let body = r#"{"coins":[{"id":"epic-chain","symbol":"EPIC","name":"Epic Chain"},{"id":"epic-cash","symbol":"EPIC","name":"Epic Private Internet Cash"}]}"#;
        assert_eq!(coingecko_search_id(body, "EPIC", "EPIC Cash").as_deref(), Some("epic-cash"));
        assert_eq!(coingecko_search_id(body, "EPIC", "Epic Chain").as_deref(), Some("epic-chain"));
        let paprika = r#"{"currencies":[{"id":"epic-chain","symbol":"EPIC","name":"Epic Chain"},{"id":"epic-epic-cash","symbol":"EPIC","name":"Epic Cash"}]}"#;
        assert_eq!(paprika_search_id(paprika, "EPIC", "EPIC Cash").as_deref(), Some("epic-epic-cash"));
        let unknown = resolve_holding("Kaspa", "KAS", "KAS", "crypto", "").unwrap();
        assert_eq!(unknown.symbol, "KAS");
        assert!(unknown.coingecko_id.is_empty());
    }

    #[test]
    fn providers_run_in_order_and_a_later_one_only_fills_gaps() {
        assert_eq!(CRYPTO_PROVIDERS, ["coingecko", "coinpaprika", "kraken", "cryptocompare", "tradeogre", "bitmart"]);
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
        }, None, "");
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
        let kept = refresh_book(&previous, &[xmr()], 9_000, true, &|_url| Err("down".into()), Some(&|_asset| Ok(AiEstimate::priced(1.0, "USD"))), "");
        let quote = kept.quotes.iter().find(|row| row.symbol == "XMR").unwrap();
        assert_eq!(quote.price, 150.0);
        assert!(!quote.estimated);
        assert_ne!(quote.price, 1.0);
        let epic = alias("EPIC").unwrap();
        let estimated = refresh_book(&PriceBook::default(), &[epic], 9_000, true, &|_url| Err("down".into()), Some(&|_asset| Ok(AiEstimate::priced(0.9, "eur"))), "");
        let epic_quote = estimated.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!(epic_quote.estimated);
        assert_eq!(epic_quote.source, "ai");
        assert_eq!(epic_quote.price, 0.9);
        assert!(parse_ai_price("I think about 12").is_err());
        assert!(parse_ai_price(r#"{"price":null,"currency":"USD"}"#).is_err());
        let legacy = parse_ai_price(r#"{"price":"408.5","currency":"eur"}"#).unwrap();
        assert_eq!((legacy.price, legacy.currency.as_str()), (408.5, "EUR"));
        let web = parse_ai_price(r#"{"price_usd":0.832979,"source_url":"https://www.coingecko.com/en/coins/epic-cash","as_of":"2026-11-22"}"#).unwrap();
        assert_eq!(web.currency, "USD");
        assert!((web.price - 0.832979).abs() < 0.000001);
        assert!(web.source_url.contains("coingecko"));
        assert!(parse_ai_price(r#"{"price_usd":0,"source_url":"https://example.com","as_of":"2026-11-22"}"#).is_err());
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

    #[test]
    fn a_typed_name_search_that_returns_no_coins_falls_through_to_the_ticker() {
        let mut asset = crypto_ref("EPIC".into(), String::new(), String::new(), String::new());
        asset.label = "EPIC Cash".into();
        let calls = std::sync::Mutex::new(Vec::new());
        let now = 50_000_i64;
        let book = refresh_book(&PriceBook::default(), &[asset], now, false, &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("query=EPIC%20Cash") || url.contains("query=Epic%20Cash") {
                return Ok(r#"{"coins":[]}"#.into());
            }
            if url.contains("q=EPIC%20Cash") {
                return Ok(r#"{"currencies":[]}"#.into());
            }
            if url.contains("query=EPIC") {
                return Ok(r#"{"coins":[{"id":"epic-chain","symbol":"EPIC","name":"Epic Chain"},{"id":"epic-cash","symbol":"EPIC","name":"Epic Private Internet Cash"}]}"#.into());
            }
            if url.contains("ids=epic-cash") {
                return Ok(r#"{"epic-cash":{"eur":0.769,"chf":0.709}}"#.into());
            }
            Err("unused".into())
        }, None, "");
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("query=EPIC%20Cash")), "the typed name is tried");
        assert!(seen.iter().any(|url| url.contains("query=EPIC") && !url.contains("EPIC%20")), "an empty name search tries the ticker");
        let quote = book.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert_eq!(quote.price, 0.769);
        assert_eq!(quote.source, "coingecko");
        assert!(!quote.estimated);
        assert!(book.misses.iter().all(|row| row.symbol != "EPIC"));
    }

    #[test]
    fn a_failed_lookup_does_not_count_as_todays_price_and_ai_runs_when_live_providers_miss() {
        let now = 80_000_i64;
        let epic = alias("EPIC Cash").unwrap();
        let mut missed = PriceBook::default();
        missed.misses.push(PriceMiss { symbol: "EPIC".into(), fetched_ms: now, reason: "Could not refresh the price.".into() });
        assert!(needs_refresh(&missed, &[epic.clone()], now, false), "a miss from this morning still retries");
        let asked = std::cell::Cell::new(false);
        let estimated = refresh_book(&missed, &[epic.clone()], now, false, &|_url| Err("down".into()), Some(&|_asset| {
            asked.set(true);
            Ok(AiEstimate::priced(0.77, "EUR"))
        }), "");
        assert!(asked.get(), "an unpriced holding asks the configured AI provider");
        let quote = estimated.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!(quote.estimated);
        assert_eq!(quote.source, "ai");
        assert_eq!(quote.price, 0.77);
        assert_eq!(quote.fetched_ms, now);
        let priced = price_for(&estimated, &epic, now).unwrap();
        assert!(priced.estimated);
        assert_eq!(priced.provider, "ai");
        let mut with_manual = estimated.clone();
        upsert_manual(&mut with_manual, "EPIC", 1.5, "EUR", "", false).unwrap();
        let live_estimate = price_for(&with_manual, &epic, now).unwrap();
        assert!(live_estimate.estimated, "a non-override manual price waits behind the AI estimate");
        assert_ne!(live_estimate.unit_price, 1.5);
        let mut bare = PriceBook::default();
        upsert_manual(&mut bare, "EPIC", 1.5, "EUR", "", false).unwrap();
        let fallback = refresh_book(&bare, &[epic.clone()], now, false, &|_url| Err("down".into()), Some(&|_asset| Err("no estimate".into())), "");
        let manual = price_for(&fallback, &epic, now).unwrap();
        assert_eq!(manual.source, "manual");
        assert_eq!(manual.unit_price, 1.5);
        assert!(needs_refresh(&estimated, &[epic.clone()], now, false), "an estimate does not close the daily gate for a live retry");
        let again = std::cell::Cell::new(false);
        let kept = refresh_book(&estimated, &[epic], now, false, &|_url| Err("down".into()), Some(&|_asset| {
            again.set(true);
            Ok(AiEstimate::priced(9.0, "EUR"))
        }), "");
        assert!(!again.get(), "a fresh AI estimate is not requested again the same day");
        assert_eq!(kept.quotes.iter().find(|row| row.symbol == "EPIC").unwrap().price, 0.77);
    }

    #[test]
    fn live_epic_bodies_price_cash_crypto_and_other_shapes() {
        let gecko = r#"{"epic-cash":{"eur":0.786935,"usd":0.863111,"chf":0.725395}}"#;
        let symbols = r#"{"EPIC":{"eur":0.787549,"usd":0.863784,"chf":0.725961}}"#;
        let paprika = r#"{"id":"epic-epic-cash","quotes":{"CHF":{"price":0.7259933329388898}}}"#;
        let rows = parse_coingecko_priced_prefer(gecko, "CHF").unwrap();
        assert_eq!(rows[0].0, "epic-cash");
        assert!((rows[0].1 - 0.725395).abs() < 0.000001);
        assert_eq!(rows[0].2, "CHF");
        let symbol_rows = parse_coingecko_priced_prefer(symbols, "EUR").unwrap();
        assert!(symbol_rows[0].0.eq_ignore_ascii_case("EPIC"));
        assert!(parse_paprika_quote(paprika, "CHF").unwrap() > 0.7);
        let shapes = [
            ("EPIC Cash", "EPIC", "CHF", "cash"),
            ("Epic Cash", "EPIC", "CHF", "crypto"),
            ("EPIC", "EPIC", "CHF", "other"),
        ];
        for (name, ticker, currency, kind) in shapes {
            let asset = resolve_holding(name, ticker, currency, kind, "128.60340395").unwrap();
            assert_eq!(asset.symbol, "EPIC", "{name} {kind}");
            assert_eq!(asset.coingecko_id, "epic-cash");
            assert_eq!(asset.paprika_id, "epic-epic-cash");
            let book = refresh_book(&PriceBook::default(), &[asset.clone()], 90_000, true, &|url| {
                if url.contains("ids=epic-cash") {
                    return Ok(gecko.into());
                }
                if url.contains("symbols=epic") {
                    return Ok(symbols.into());
                }
                if url.contains("epic-epic-cash") {
                    return Ok(paprika.into());
                }
                Err("unused".into())
            }, None, "CHF");
            let quote = book.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
            assert!((quote.price - 0.725395).abs() < 0.000001, "{kind} price {}", quote.price);
            assert_eq!(quote.currency, "CHF");
            assert_eq!(quote.source, "coingecko");
            assert!(book.last_updated >= 1, "{kind}");
            assert_eq!(book.last_failed, 0);
            assert!(book.last_error.is_empty(), "{}", book.last_error);
        }
    }

    #[test]
    #[ignore = "hits the public CoinGecko and CoinPaprika APIs"]
    fn live_epic_cash_prices() {
        let gecko = blocking_get("https://api.coingecko.com/api/v3/simple/price?ids=epic-cash&vs_currencies=eur,usd,chf").expect("coingecko");
        println!("coingecko {gecko}");
        let rows = parse_coingecko_priced_prefer(&gecko, "CHF").expect("parse coingecko");
        assert!(rows.iter().any(|(id, price, currency)| id == "epic-cash" && *currency == "CHF" && *price > 0.01));
        let paprika = blocking_get("https://api.coinpaprika.com/v1/tickers/epic-epic-cash?quotes=EUR,CHF").expect("coinpaprika");
        println!("coinpaprika {}", &paprika[..paprika.len().min(240)]);
        let price = parse_paprika_quote(&paprika, "CHF").or_else(|| parse_paprika_quote(&paprika, "EUR")).expect("paprika price");
        assert!(price > 0.01 && price < 100.0, "{price}");
        let asset = resolve_holding("EPIC Cash", "EPIC", "CHF", "cash", "128.60340395").unwrap();
        let book = refresh_book(&PriceBook::default(), &[asset], 90_000, true, &blocking_get, None, "CHF");
        let quote = book.quotes.iter().find(|row| row.symbol == "EPIC").expect("stored");
        println!("stored {} {} via {} updated {} failed {} {}", quote.price, quote.currency, quote.source, book.last_updated, book.last_failed, book.last_error);
        assert!(quote.price > 0.01);
        assert_eq!(book.last_failed, 0, "{}", book.last_error);
    }
}

//! Daily holding values. A point is a real price times the quantity known that day.
//! Nothing here invents a price. A later sample on the same day replaces the earlier one.
//! CoinGecko history, when used, is labeled and is not rewritten after a quantity change.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValuePoint {
    pub account_id: String,
    pub date: String,
    pub quantity: String,
    pub value_minor: i64,
    pub currency: String,
    #[serde(default)]
    pub native_minor: i64,
    #[serde(default)]
    pub native_currency: String,
    pub source: String,
    pub name: String,
    #[serde(default)]
    pub class_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPin {
    pub account_id: String,
    #[serde(default)]
    pub coingecko_id: String,
    #[serde(default)]
    pub paprika_id: String,
    /// Set from the price sheet. An automatic pin can be replaced. This one stays.
    #[serde(default)]
    pub manual: bool,
}

/// Keep the last value for a holding on a calendar day.
pub fn upsert_point(points: &mut Vec<ValuePoint>, point: ValuePoint) {
    if point.date.len() != 10 || point.account_id.is_empty() || point.value_minor == 0 {
        return;
    }
    if let Some(existing) = points.iter_mut().find(|row| row.account_id == point.account_id && row.date == point.date) {
        *existing = point;
    } else {
        points.push(point);
    }
}

/// One EUR price per day, last sample of that day. Dates are YYYY-MM-DD in UTC.
pub fn parse_market_chart(body: &str) -> Vec<(String, f64)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let Some(rows) = value.get("prices").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    let mut by_day: Vec<(String, i64, f64)> = Vec::new();
    for row in rows {
        let Some(pair) = row.as_array() else { continue };
        let Some(stamp) = pair.first().and_then(|item| item.as_i64().or_else(|| item.as_f64().map(|n| n as i64))) else { continue };
        let Some(price) = pair.get(1).and_then(|item| item.as_f64()) else { continue };
        if !price.is_finite() || price <= 0.0 {
            continue;
        }
        let date = utc_date(stamp);
        if date.is_empty() {
            continue;
        }
        if let Some(existing) = by_day.iter_mut().find(|item| item.0 == date) {
            if stamp >= existing.1 {
                existing.1 = stamp;
                existing.2 = price;
            }
        } else {
            by_day.push((date, stamp, price));
        }
    }
    by_day.sort_by(|a, b| a.0.cmp(&b.0));
    by_day.into_iter().map(|(date, _, price)| (date, price)).collect()
}

fn utc_date(stamp_ms: i64) -> String {
    let seconds = stamp_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// Change from the first point in range to the last, summed across holdings.
pub fn portfolio_change(points: &[ValuePoint], from: &str, today: &str) -> Option<(i64, i64, String)> {
    let mut ids = Vec::new();
    for point in points {
        if !ids.iter().any(|id: &String| id == &point.account_id) {
            ids.push(point.account_id.clone());
        }
    }
    if ids.is_empty() {
        return None;
    }
    let mut start = 0_i64;
    let mut end = 0_i64;
    let mut currency = String::new();
    let mut any = false;
    for id in ids {
        let mut rows: Vec<&ValuePoint> = points.iter().filter(|row| row.account_id == id && row.date.as_str() >= from && row.date.as_str() <= today).collect();
        rows.sort_by(|a, b| a.date.cmp(&b.date));
        let Some(first) = rows.first() else { continue };
        let Some(last) = rows.last() else { continue };
        start = start.saturating_add(first.value_minor);
        end = end.saturating_add(last.value_minor);
        if currency.is_empty() {
            currency = last.currency.clone();
        }
        any = true;
    }
    if !any || currency.is_empty() {
        return None;
    }
    Some((end.saturating_sub(start), end, currency))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_day_keeps_the_last_sample_and_older_days_stay() {
        let mut points = Vec::new();
        upsert_point(&mut points, point("a", "2026-11-01", "1", 100));
        upsert_point(&mut points, point("a", "2026-11-01", "2", 180));
        upsert_point(&mut points, point("a", "2026-11-02", "2", 200));
        assert_eq!(points.len(), 2);
        assert_eq!(points.iter().find(|row| row.date == "2026-11-01").unwrap().quantity, "2");
        assert_eq!(points.iter().find(|row| row.date == "2026-11-01").unwrap().value_minor, 180);
        assert_eq!(points.iter().find(|row| row.date == "2026-11-02").unwrap().quantity, "2");
    }

    #[test]
    fn market_chart_keeps_one_real_price_per_day() {
        let body = r#"{"prices":[[1761955200000,0.8],[1761958800000,0.9],[1762041600000,1.1]]}"#;
        let rows = parse_market_chart(body);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, price)| *price > 0.0));
        assert_eq!(rows[0].1, 0.9);
        assert!(parse_market_chart("{}").is_empty());
    }

    #[test]
    fn portfolio_change_uses_saved_points_only() {
        let points = vec![point("a", "2026-11-01", "1", 100), point("b", "2026-11-01", "1", 50), point("a", "2026-11-21", "1", 140)];
        let (delta, end, currency) = portfolio_change(&points, "2026-11-01", "2026-11-21").unwrap();
        assert_eq!(currency, "CHF");
        assert_eq!(end, 190);
        assert_eq!(delta, 40);
    }

    fn point(id: &str, date: &str, quantity: &str, value: i64) -> ValuePoint {
        ValuePoint {
            account_id: id.into(),
            date: date.into(),
            quantity: quantity.into(),
            value_minor: value,
            currency: "CHF".into(),
            native_minor: value,
            native_currency: "CHF".into(),
            source: "test".into(),
            name: id.into(),
            class_name: String::new(),
        }
    }
}

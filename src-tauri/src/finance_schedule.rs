//! Schedule dates, progress, and the phrases Gabriel uses to program money.
//! Open-ended runs never grow a lifetime total. A fixed run keeps every date.

use chrono::{Datelike, Duration, NaiveDate};

#[derive(Clone, Debug)]
pub struct Spec {
    pub cadence: String,
    pub interval_days: u32,
    pub start: NaiveDate,
    pub end_kind: String,
    pub end_count: u32,
    pub end_date: Option<NaiveDate>,
    pub amount_minor: Option<i64>,
    pub skipped: Vec<NaiveDate>,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub mode: &'static str,
    pub paid: u32,
    pub total: Option<u32>,
    pub remaining: Option<u32>,
    pub remaining_minor: Option<i64>,
    pub per_period_minor: Option<i64>,
    pub near_term_minor: Option<i64>,
    pub near_term_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub kind: String,
    pub name: String,
    pub amount: String,
    pub currency: String,
    pub direction: String,
    pub purpose: String,
    pub cadence: String,
    pub interval_days: u32,
    pub end_kind: String,
    pub end_count: u32,
    pub start_date: String,
    pub amount_mode: String,
    pub payout_currency: String,
    pub category: String,
    pub account_name: String,
    pub account_kind: String,
    pub balance: String,
    pub balance_currency: String,
    pub action: String,
    pub new_name: String,
}

pub fn occurrence_dates(spec: &Spec, today: NaiveDate) -> Vec<NaiveDate> {
    if spec.cadence.is_empty() || !repeats(&spec.cadence, spec.interval_days) {
        return vec![spec.start];
    }
    if spec.end_kind == "after_count" && spec.end_count > 0 {
        let mut dates = Vec::new();
        for n in 0..spec.end_count.min(600) {
            let Some(cursor) = nth_date(spec.start, &spec.cadence, spec.interval_days, n as i32) else { break };
            dates.push(cursor);
        }
        return dates;
    }
    let horizon = add_months(today, 24).unwrap_or(today);
    let floor = add_months(today, -24).unwrap_or(today);
    let end = if spec.end_kind == "on_date" { spec.end_date.unwrap_or(horizon) } else { horizon };
    let first = if spec.start < floor && spec.end_kind != "on_date" { first_on_or_after(spec.start, floor, &spec.cadence, spec.interval_days) } else { 0 };
    let mut dates = Vec::new();
    for n in first..first.saturating_add(800) {
        let Some(cursor) = nth_date(spec.start, &spec.cadence, spec.interval_days, n) else { break };
        if cursor > end {
            break;
        }
        if cursor >= floor || spec.end_kind == "on_date" {
            dates.push(cursor);
        }
    }
    dates
}

pub fn repeats(cadence: &str, interval: u32) -> bool {
    matches!(cadence, "daily" | "weekly" | "monthly" | "yearly") || (is_interval(cadence) && interval >= 1)
}

pub fn is_interval(cadence: &str) -> bool {
    matches!(cadence, "every_n_days" | "every_n_weeks" | "every_n_months" | "every_n_years")
}

/// Date of occurrence `n`, counted from the start. Month and year steps clamp to the
/// start day's month length, so 31 January stays the 31st after February.
pub fn nth_date(start: NaiveDate, cadence: &str, interval: u32, n: i32) -> Option<NaiveDate> {
    if n < 0 {
        return None;
    }
    match cadence {
        "daily" => start.checked_add_signed(Duration::days(i64::from(n))),
        "weekly" => start.checked_add_signed(Duration::days(7 * i64::from(n))),
        "monthly" => add_months(start, n),
        "yearly" => add_months(start, n.saturating_mul(12)),
        "every_n_days" if interval >= 1 => start.checked_add_signed(Duration::days(i64::from(interval) * i64::from(n))),
        "every_n_weeks" if interval >= 1 => start.checked_add_signed(Duration::days(7 * i64::from(interval) * i64::from(n))),
        "every_n_months" if interval >= 1 => add_months(start, n.saturating_mul(interval as i32)),
        "every_n_years" if interval >= 1 => add_months(start, n.saturating_mul((interval as i32).saturating_mul(12))),
        _ => (n == 0).then_some(start),
    }
}

pub fn progress(spec: &Spec, settled: &[NaiveDate], today: NaiveDate) -> Progress {
    let dates = occurrence_dates(spec, today);
    let skipped: Vec<NaiveDate> = spec.skipped.iter().copied().filter(|date| dates.contains(date) && !settled.contains(date)).collect();
    let paid = dates.iter().filter(|date| settled.contains(date)).count() as u32;
    let active = spec.status.is_empty() || spec.status == "active";
    let horizon = add_months(today, 12).unwrap_or(today);
    let near: Vec<NaiveDate> = if active {
        dates.iter().copied().filter(|date| *date >= today && *date < horizon && !settled.contains(date) && !spec.skipped.contains(date)).collect()
    } else {
        Vec::new()
    };
    if spec.end_kind == "after_count" && spec.end_count > 0 && !spec.cadence.is_empty() {
        let remaining = spec.end_count.saturating_sub(paid).saturating_sub(skipped.len() as u32);
        return Progress {
            mode: "fixed",
            paid,
            total: Some(spec.end_count),
            remaining: Some(remaining),
            remaining_minor: spec.amount_minor.map(|amount| i64::from(remaining) * amount),
            per_period_minor: spec.amount_minor,
            near_term_minor: None,
            near_term_count: 0,
        };
    }
    if spec.cadence.is_empty() {
        let done = paid > 0 || !skipped.is_empty();
        let remaining = if done { 0 } else { 1 };
        return Progress {
            mode: "once",
            paid: u32::from(paid > 0),
            total: Some(1),
            remaining: Some(remaining),
            remaining_minor: spec.amount_minor.map(|amount| i64::from(remaining) * amount),
            per_period_minor: spec.amount_minor,
            near_term_minor: None,
            near_term_count: 0,
        };
    }
    Progress {
        mode: "open",
        paid,
        total: None,
        remaining: None,
        remaining_minor: None,
        per_period_minor: spec.amount_minor,
        near_term_minor: if active { spec.amount_minor.map(|amount| i64::from(near.len() as u32) * amount) } else { Some(0) },
        near_term_count: near.len() as u32,
    }
}

pub fn next_open(spec: &Spec, settled: &[NaiveDate], _today: NaiveDate) -> Option<NaiveDate> {
    if spec.status == "paused" || spec.status == "stopped" || spec.status == "completed" {
        return None;
    }
    let limit = if spec.end_kind == "after_count" && spec.end_count > 0 { spec.end_count.min(600) } else { 2_000 };
    for n in 0..limit {
        let Some(date) = nth_date(spec.start, &spec.cadence, spec.interval_days, n as i32) else { break };
        if spec.end_kind == "on_date" {
            if let Some(end) = spec.end_date {
                if date > end {
                    break;
                }
            }
        }
        if !settled.contains(&date) && !spec.skipped.contains(&date) {
            return Some(date);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct WorthAccount {
    pub kind: String,
    pub currency: String,
    pub shown_minor: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorthRow {
    pub currency: String,
    pub cash_minor: i64,
    pub invested_minor: i64,
    pub owed_minor: i64,
    pub net_minor: i64,
}

pub fn worth_rows(accounts: &[WorthAccount]) -> Vec<WorthRow> {
    let mut rows: Vec<WorthRow> = Vec::new();
    for account in accounts {
        let Some(shown) = account.shown_minor else { continue };
        if account.currency.is_empty() {
            continue;
        }
        let row = if let Some(row) = rows.iter_mut().find(|row| row.currency == account.currency) {
            row
        } else {
            rows.push(WorthRow { currency: account.currency.clone(), cash_minor: 0, invested_minor: 0, owed_minor: 0, net_minor: 0 });
            rows.last_mut().unwrap()
        };
        match bucket(&account.kind) {
            "debt" => row.owed_minor += shown,
            "invest" => row.invested_minor += shown,
            _ => row.cash_minor += shown,
        }
        row.net_minor = row.cash_minor + row.invested_minor - row.owed_minor;
    }
    rows.sort_by(|a, b| a.currency.cmp(&b.currency));
    rows
}

pub fn bucket(kind: &str) -> &'static str {
    match kind {
        "credit_card" | "loan" | "other_debt" | "card" => "debt",
        "investment" => "invest",
        _ => "cash",
    }
}

pub fn parse_phrase(phrase: &str, today: NaiveDate, default_currency: &str) -> Result<Parsed, String> {
    let text = phrase.trim();
    let folded = fold(text);
    if folded.is_empty() {
        return Err("Say the amount and how often.".into());
    }
    let tokens = words(&folded);
    if let Some(parsed) = action_of(&tokens) {
        return Ok(parsed);
    }
    if folded.contains("pays") && folded.contains("every") {
        return payout_of(text, &tokens, today);
    }
    if let Some((count, unit)) = for_count(&tokens) {
        return Ok(course_of(&tokens, count, &unit, today));
    }
    if folded.contains("as long as") || folded.contains("for as long") {
        return Ok(open_of(&tokens, today));
    }
    if folded.contains("subscription") {
        return Ok(subscription_of(text, &tokens, today));
    }
    if folded.contains("will get") || folded.contains("will receive") {
        return Ok(future_income(&tokens, today));
    }
    if folded.contains("got paid") || folded.contains("was paid") {
        return Ok(cash_tx(&tokens, "income", "", today, default_currency));
    }
    if tokens.iter().any(|token| token == "spent" || token == "spend") {
        return Ok(spent(&tokens, today, default_currency));
    }
    if let Some(parsed) = list_of(&tokens) {
        return Ok(parsed);
    }
    if let Some(parsed) = delete_of(&tokens) {
        return Ok(parsed);
    }
    if let Some(parsed) = edit_of(&tokens, today) {
        return Ok(parsed);
    }
    if let Some(parsed) = payment_every(&tokens, today, default_currency) {
        return Ok(parsed);
    }
    Err("Say the amount, how often, and whether it ends.".into())
}

fn course_of(tokens: &[String], count: u32, unit: &str, today: NaiveDate) -> Parsed {
    let money = first_money(tokens);
    let mut parsed = schedule("Course", "expense", "recurring", &money, today);
    parsed.cadence = if unit.starts_with("week") { "weekly" } else if unit.starts_with("day") { "daily" } else { "monthly" }.into();
    parsed.end_kind = "after_count".into();
    parsed.end_count = count;
    parsed
}

fn open_of(tokens: &[String], today: NaiveDate) -> Parsed {
    let money = first_money(tokens);
    let mut parsed = schedule("Rent", "expense", "recurring", &money, today);
    parsed.cadence = cadence_of(tokens);
    parsed.end_kind = "none".into();
    parsed
}

fn subscription_of(original: &str, tokens: &[String], today: NaiveDate) -> Parsed {
    let money = first_money(tokens);
    let name = title_before(original, "subscription").unwrap_or_else(|| "Subscription".into());
    let mut parsed = schedule(&name, "expense", "subscription", &money, today);
    parsed.cadence = cadence_of(tokens);
    if parsed.cadence.is_empty() {
        parsed.cadence = "monthly".into();
    }
    parsed.end_kind = "none".into();
    parsed
}

fn future_income(tokens: &[String], today: NaiveDate) -> Parsed {
    let money = first_money(tokens);
    let mut parsed = schedule("Income", "income", "one_off", &money, today);
    parsed.cadence.clear();
    parsed.end_kind = "after_count".into();
    parsed.end_count = 1;
    if let Some(month) = month_in(tokens) {
        parsed.start_date = iso(next_month_first(month, today));
    }
    parsed
}

fn spent(tokens: &[String], today: NaiveDate, default_currency: &str) -> Parsed {
    let mut parsed = cash_tx(tokens, "expense", "", today, default_currency);
    if let Some(on) = tokens.iter().position(|token| token == "on") {
        if let Some(word) = tokens.get(on + 1) {
            if currency_code(word).is_none() {
                parsed.category = title_word(word);
            }
        }
    }
    parsed
}

fn payout_of(original: &str, tokens: &[String], today: NaiveDate) -> Result<Parsed, String> {
    let folded = fold(original);
    let pays = folded.find(" that pays").ok_or("Say what the payout is.")?;
    let before = &folded[..pays];
    let inn = before.rfind(" in ").ok_or("Say the investment name.")?;
    let name = original[inn + 4..pays].trim();
    if name.is_empty() {
        return Err("Say the investment name.".into());
    }
    let hits = money_hits(tokens);
    if hits.len() < 2 {
        return Err("Say the principal and the payout.".into());
    }
    let interval = every_days(tokens).ok_or("Say how many days between payouts.")?;
    let payout_currency = tokens.iter().rev().find_map(|token| currency_code(token)).unwrap_or("").to_string();
    let payout = hits[1].clone();
    let mut parsed = schedule(&format!("{name} payout"), "income", "investment_payout", &payout, today);
    parsed.account_name = name.to_string();
    parsed.account_kind = "investment".into();
    parsed.balance = hits[0].amount.clone();
    parsed.balance_currency = hits[0].currency.clone();
    parsed.cadence = "every_n_days".into();
    parsed.interval_days = interval;
    parsed.end_kind = "none".into();
    parsed.kind = "payout".into();
    if !payout_currency.is_empty() && payout_currency != payout.currency {
        parsed.amount_mode = "quote".into();
        parsed.payout_currency = payout_currency;
    } else if payout.currency != "USD" && payout.currency != "EUR" && payout.currency != "CHF" && payout.currency != "GBP" && !payout.currency.is_empty() {
        parsed.amount_mode = "fixed".into();
        parsed.payout_currency = payout.currency.clone();
    }
    Ok(parsed)
}

fn action_of(tokens: &[String]) -> Option<Parsed> {
    let action = match tokens.first().map(String::as_str) {
        Some("stop") | Some("cancel") => "stop",
        Some("pause") => "pause",
        Some("resume") => "resume",
        _ => return None,
    };
    let name = tokens
        .iter()
        .skip(1)
        .filter(|token| !matches!(token.as_str(), "the" | "a" | "my" | "subscription" | "bill" | "payment"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return None;
    }
    let mut parsed = Parsed::blank();
    parsed.kind = "action".into();
    parsed.action = action.into();
    parsed.name = title_word(&name);
    Some(parsed)
}

fn cash_tx(tokens: &[String], direction: &str, category: &str, today: NaiveDate, default_currency: &str) -> Parsed {
    let money = first_money(tokens);
    let mut parsed = Parsed::blank();
    parsed.kind = "transaction".into();
    parsed.direction = direction.into();
    parsed.amount = money.amount;
    parsed.currency = if money.currency.is_empty() { default_currency.to_string() } else { money.currency };
    parsed.category = category.into();
    parsed.start_date = iso(today);
    parsed
}

fn schedule(name: &str, direction: &str, purpose: &str, money: &MoneyHit, today: NaiveDate) -> Parsed {
    let mut parsed = Parsed::blank();
    parsed.kind = "schedule".into();
    parsed.name = name.into();
    parsed.direction = direction.into();
    parsed.purpose = purpose.into();
    parsed.amount = money.amount.clone();
    parsed.currency = money.currency.clone();
    parsed.start_date = iso(today);
    parsed.amount_mode = "fixed".into();
    parsed.status_defaults();
    parsed
}

#[derive(Clone, Debug)]
struct MoneyHit {
    amount: String,
    currency: String,
}

fn first_money(tokens: &[String]) -> MoneyHit {
    money_hits(tokens).into_iter().next().unwrap_or(MoneyHit { amount: String::new(), currency: String::new() })
}

fn money_hits(tokens: &[String]) -> Vec<MoneyHit> {
    let mut hits = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if !is_number(token) {
            continue;
        }
        let prev = index.checked_sub(1).and_then(|at| tokens.get(at)).map(String::as_str).unwrap_or("");
        let next = tokens.get(index + 1).map(String::as_str).unwrap_or("");
        if prev == "every" || prev == "for" || next.starts_with("day") || next.starts_with("month") || next.starts_with("week") {
            continue;
        }
        let currency = currency_code(next).or_else(|| currency_code(prev)).unwrap_or("").to_string();
        hits.push(MoneyHit { amount: token.clone(), currency });
    }
    hits
}

fn every_days(tokens: &[String]) -> Option<u32> {
    for window in tokens.windows(3) {
        if window[0] == "every" && is_number(&window[1]) && window[2].starts_with("day") {
            return window[1].parse().ok();
        }
    }
    None
}

fn for_count(tokens: &[String]) -> Option<(u32, String)> {
    for window in tokens.windows(3) {
        if window[0] == "for" && is_number(&window[1]) && (window[2].starts_with("month") || window[2].starts_with("week") || window[2].starts_with("day")) {
            return Some((window[1].parse().ok()?, window[2].clone()));
        }
    }
    None
}

fn find_interval(tokens: &[String]) -> Option<(u32, &'static str)> {
    for (index, token) in tokens.iter().enumerate() {
        let (number_at, unit_at) = if matches!(token.as_str(), "every" | "cada" | "alle") {
            (index + 1, index + 2)
        } else if token == "a" && tokens.get(index + 1).map(String::as_str) == Some("cada") {
            (index + 2, index + 3)
        } else {
            continue;
        };
        let number = tokens.get(number_at)?;
        if !is_number(number) {
            continue;
        }
        let count: u32 = number.parse().ok()?;
        if count < 1 {
            continue;
        }
        let unit = interval_unit(tokens.get(unit_at)?.as_str())?;
        return Some((count, unit));
    }
    None
}

fn interval_unit(word: &str) -> Option<&'static str> {
    let word = fold(word);
    if matches!(word.as_str(), "day" | "days" | "dia" | "dias" | "tag" | "tage" | "tagen" | "dies") {
        Some("day")
    } else if matches!(word.as_str(), "week" | "weeks" | "semana" | "semanas" | "woche" | "wochen" | "setmana" | "setmanes") {
        Some("week")
    } else if word.starts_with("month") || word.starts_with("monat") || matches!(word.as_str(), "mes" | "meses" | "mesos" | "mesi") {
        Some("month")
    } else if matches!(word.as_str(), "year" | "years" | "ano" | "anos" | "jahr" | "jahre" | "jahren" | "any" | "anys") {
        Some("year")
    } else {
        None
    }
}

pub fn cadence_for_unit(unit: &str) -> &'static str {
    match unit {
        "week" => "every_n_weeks",
        "month" => "every_n_months",
        "year" => "every_n_years",
        _ => "every_n_days",
    }
}

fn starts_on(tokens: &[String], today: NaiveDate) -> NaiveDate {
    let tomorrow = tokens.iter().any(|token| matches!(token.as_str(), "tomorrow" | "manana" | "morgen" | "amanha" | "dema"));
    if tomorrow {
        return today + Duration::days(1);
    }
    today
}

fn list_of(tokens: &[String]) -> Option<Parsed> {
    let head = tokens.first().map(String::as_str)?;
    if !matches!(head, "list" | "lista" | "liste" | "llista") {
        return None;
    }
    let joined = tokens.join(" ");
    if !(joined.contains("recurr") || joined.contains("bill") || joined.contains("pago") || joined.contains("pagament") || joined.contains("zahlung") || joined.contains("despesa") || joined.contains("gasto")) {
        return None;
    }
    let mut parsed = Parsed::blank();
    parsed.kind = "list".into();
    Some(parsed)
}

fn delete_of(tokens: &[String]) -> Option<Parsed> {
    let confirmed = tokens.iter().any(|token| matches!(token.as_str(), "yes" | "si" | "ja" | "sim"));
    let verb_at = tokens.iter().position(|token| matches!(token.as_str(), "delete" | "borra" | "borrar" | "losche" | "apaga" | "apagar" | "esborra"))?;
    let name = tokens
        .iter()
        .skip(verb_at + 1)
        .filter(|token| !matches!(token.as_str(), "the" | "a" | "my" | "el" | "la" | "o" | "yes" | "si" | "ja" | "sim" | "payment" | "pago" | "bill" | "pagament"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return None;
    }
    let mut parsed = Parsed::blank();
    parsed.kind = "delete".into();
    parsed.action = if confirmed { "delete_yes".into() } else { "delete".into() };
    parsed.name = title_word(&name);
    Some(parsed)
}

fn edit_of(tokens: &[String], today: NaiveDate) -> Option<Parsed> {
    let head = tokens.first().map(String::as_str)?;
    if !matches!(head, "change" | "cambia" | "cambiar" | "andere" | "muda" | "mudar" | "canvia" | "rename") {
        return None;
    }
    let split = tokens.iter().position(|token| matches!(token.as_str(), "to" | "a" | "auf" | "para")).unwrap_or(tokens.len());
    let name = tokens
        .iter()
        .take(split)
        .skip(1)
        .filter(|token| !matches!(token.as_str(), "the" | "el" | "la" | "o" | "my" | "payment" | "pago" | "bill" | "amount" | "name" | "start" | "category" | "account"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return None;
    }
    let mut parsed = Parsed::blank();
    parsed.kind = "edit".into();
    parsed.name = title_word(&name);
    parsed.end_kind.clear();
    if let Some((count, unit)) = find_interval(tokens) {
        parsed.cadence = cadence_for_unit(unit).into();
        parsed.interval_days = count;
    }
    if head == "rename" {
        let renamed = tokens.iter().skip(split + 1).cloned().collect::<Vec<_>>().join(" ");
        if !renamed.is_empty() {
            parsed.new_name = title_word(&renamed);
        }
    }
    let money = first_money(tokens);
    if !money.amount.is_empty() && find_interval(tokens).is_none() {
        parsed.amount = money.amount;
        parsed.currency = money.currency;
    }
    if tokens.iter().any(|token| matches!(token.as_str(), "tomorrow" | "manana" | "morgen" | "amanha" | "dema")) {
        parsed.start_date = iso(starts_on(tokens, today));
    }
    if let Some(category) = labeled_tail(tokens, &["category", "categoria", "kategorie", "categoria"]) {
        parsed.category = category;
    }
    if let Some(account) = account_tail(tokens) {
        parsed.account_name = account;
    }
    if tokens.iter().any(|token| matches!(token.as_str(), "never" | "nunca" | "nie" | "mai")) {
        parsed.end_kind = "none".into();
    }
    if parsed.cadence.is_empty() && parsed.amount.is_empty() && parsed.start_date.is_empty() && parsed.category.is_empty() && parsed.account_name.is_empty() && parsed.end_kind.is_empty() && parsed.new_name.is_empty() {
        return None;
    }
    Some(parsed)
}

fn payment_every(tokens: &[String], today: NaiveDate, default_currency: &str) -> Option<Parsed> {
    let (count, unit) = find_interval(tokens)?;
    let money = first_money(tokens);
    if money.amount.is_empty() {
        return None;
    }
    let direction = if tokens.iter().any(|token| matches!(token.as_str(), "income" | "salary" | "ingreso" | "einnahme" | "receita" | "ingres")) { "income" } else { "expense" };
    let mut parsed = schedule("Payment", direction, "recurring", &money, today);
    if parsed.currency.is_empty() {
        parsed.currency = default_currency.to_string();
    }
    parsed.cadence = cadence_for_unit(unit).into();
    parsed.interval_days = count;
    parsed.end_kind = "none".into();
    parsed.start_date = iso(starts_on(tokens, today));
    if let Some(account) = account_tail(tokens) {
        parsed.account_name = account;
    }
    Some(parsed)
}

fn labeled_tail(tokens: &[String], labels: &[&str]) -> Option<String> {
    let at = tokens.iter().position(|token| labels.contains(&token.as_str()))?;
    let word = tokens.iter().skip(at + 1).find(|token| !matches!(token.as_str(), "to" | "a" | "auf" | "para" | "the" | "el" | "la"))?;
    Some(title_word(word))
}

fn account_tail(tokens: &[String]) -> Option<String> {
    let at = tokens.iter().position(|token| matches!(token.as_str(), "from" | "account" | "card" | "desde" | "cuenta" | "tarjeta" | "von" | "konto" | "karte" | "conta" | "cartao" | "compte" | "targeta"))?;
    let name = tokens.iter().skip(at + 1).filter(|token| !matches!(token.as_str(), "the" | "el" | "la" | "o" | "my")).cloned().collect::<Vec<_>>().join(" ");
    if name.is_empty() { None } else { Some(title_word(&name)) }
}

fn cadence_of(tokens: &[String]) -> String {
    if tokens.iter().any(|token| token.starts_with("year")) {
        "yearly".into()
    } else if tokens.iter().any(|token| token.starts_with("week")) {
        "weekly".into()
    } else if tokens.iter().any(|token| token == "daily" || token == "day") {
        "daily".into()
    } else {
        "monthly".into()
    }
}

fn month_in(tokens: &[String]) -> Option<u32> {
    tokens.iter().find_map(|token| month_number(token))
}

fn month_number(token: &str) -> Option<u32> {
    match token {
        "january" | "enero" | "januar" | "janeiro" | "gener" => Some(1),
        "february" | "febrero" | "februar" | "fevereiro" | "febrer" => Some(2),
        "march" | "marzo" | "marz" | "märz" | "marco" | "març" => Some(3),
        "april" | "abril" | "abr" => Some(4),
        "may" | "mayo" | "mai" | "maio" | "maig" => Some(5),
        "june" | "junio" | "juni" | "junho" | "juny" => Some(6),
        "july" | "julio" | "juli" | "julho" | "juliol" => Some(7),
        "august" | "agosto" | "agost" => Some(8),
        "september" | "septiembre" | "setembro" | "setembre" | "setiembre" => Some(9),
        "october" | "octubre" | "oktober" | "outubro" => Some(10),
        "november" | "noviembre" | "novembro" | "novembre" => Some(11),
        "december" | "diciembre" | "dezember" | "dezembro" | "desembre" => Some(12),
        _ => None,
    }
}

fn next_month_first(month: u32, today: NaiveDate) -> NaiveDate {
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, 1).unwrap_or(today);
    if this_year >= today { this_year } else { NaiveDate::from_ymd_opt(today.year() + 1, month, 1).unwrap_or(this_year) }
}

fn title_before(original: &str, marker: &str) -> Option<String> {
    let folded = fold(original);
    let at = folded.find(marker)?;
    let words: Vec<&str> = folded[..at].split_whitespace().collect();
    let skip = ["a", "the", "my", "is", "it's"];
    let name = words.iter().rev().find(|word| !skip.contains(*word) && currency_code(word).is_none() && !is_number(word))?;
    Some(title_word(name))
}

fn title_word(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn currency_code(word: &str) -> Option<&'static str> {
    match fold(word).as_str() {
        "eur" | "euro" | "euros" => Some("EUR"),
        "usd" | "dollar" | "dollars" | "buck" | "bucks" => Some("USD"),
        "chf" | "franc" | "francs" => Some("CHF"),
        "gbp" | "pound" | "pounds" => Some("GBP"),
        "xmr" | "monero" => Some("XMR"),
        "btc" | "bitcoin" => Some("BTC"),
        "eth" | "ethereum" => Some("ETH"),
        "sol" | "solana" => Some("SOL"),
        _ => None,
    }
}

fn is_number(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') && word.chars().any(|c| c.is_ascii_digit())
}

fn words(folded: &str) -> Vec<String> {
    folded.split_whitespace().map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != ',').to_string()).filter(|word| !word.is_empty()).collect()
}

fn fold(value: &str) -> String {
    value
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
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_on_or_after(start: NaiveDate, target: NaiveDate, cadence: &str, interval: u32) -> i32 {
    if start >= target {
        return 0;
    }
    let guess = match cadence {
        "daily" => (target - start).num_days() as i32,
        "weekly" => ((target - start).num_days() / 7) as i32,
        "every_n_days" if interval >= 1 => ((target - start).num_days() / i64::from(interval)) as i32,
        "every_n_weeks" if interval >= 1 => ((target - start).num_days() / (7 * i64::from(interval))) as i32,
        "monthly" | "yearly" | "every_n_months" | "every_n_years" => {
            let step = month_step(cadence, interval);
            let months = (target.year() - start.year()) * 12 + target.month() as i32 - start.month() as i32;
            (months / step).max(0)
        }
        _ => 0,
    };
    let mut n = guess.max(0);
    while n > 0 {
        let Some(date) = nth_date(start, cadence, interval, n) else { break };
        if date < target {
            break;
        }
        n -= 1;
    }
    for _ in 0..12 {
        let Some(date) = nth_date(start, cadence, interval, n) else { return n };
        if date >= target {
            return n;
        }
        n += 1;
    }
    n
}

fn month_step(cadence: &str, interval: u32) -> i32 {
    match cadence {
        "yearly" => 12,
        "every_n_years" => (interval.max(1) as i32).saturating_mul(12),
        "every_n_months" => interval.max(1) as i32,
        _ => 1,
    }
}

pub fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    let index = date.year() * 12 + date.month() as i32 - 1 + months;
    let year = index.div_euclid(12);
    let month = (index.rem_euclid(12) + 1) as u32;
    let day = date.day();
    for day in (1..=day.min(31)).rev() {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
    }
    None
}

pub fn iso(date: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

impl Parsed {
    fn blank() -> Self {
        Self {
            kind: String::new(),
            name: String::new(),
            amount: String::new(),
            currency: String::new(),
            direction: String::new(),
            purpose: String::new(),
            cadence: String::new(),
            interval_days: 0,
            end_kind: "none".into(),
            end_count: 0,
            start_date: String::new(),
            amount_mode: "fixed".into(),
            payout_currency: String::new(),
            category: String::new(),
            account_name: String::new(),
            account_kind: String::new(),
            balance: String::new(),
            balance_currency: String::new(),
            action: String::new(),
            new_name: String::new(),
        }
    }

    fn status_defaults(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn course_progress_is_paid_against_the_nine() {
        let spec = Spec {
            cadence: "monthly".into(),
            interval_days: 0,
            start: day(2026, 11, 1),
            end_kind: "after_count".into(),
            end_count: 9,
            end_date: None,
            amount_minor: Some(27_000),
            skipped: Vec::new(),
            status: "active".into(),
        };
        let dates = occurrence_dates(&spec, day(2026, 11, 1));
        assert_eq!(dates.len(), 9);
        assert_eq!(dates[8], day(2027, 7, 1));
        let progress = progress(&spec, &dates[..3], day(2027, 2, 1));
        assert_eq!(progress.paid, 3);
        assert_eq!(progress.total, Some(9));
        assert_eq!(progress.remaining, Some(6));
        assert_eq!(progress.remaining_minor, Some(162_000));
    }

    #[test]
    fn open_rent_has_no_lifetime_total() {
        let spec = Spec {
            cadence: "monthly".into(),
            interval_days: 0,
            start: day(2026, 11, 1),
            end_kind: "none".into(),
            end_count: 0,
            end_date: None,
            amount_minor: Some(200_000),
            skipped: Vec::new(),
            status: "active".into(),
        };
        let open = progress(&spec, &[], day(2026, 11, 1));
        assert_eq!(open.mode, "open");
        assert_eq!(open.remaining_minor, None);
        assert_eq!(open.total, None);
        assert_eq!(open.near_term_count, 12);
        assert_eq!(open.near_term_minor, Some(2_400_000));
        let stopped = Spec { status: "stopped".into(), ..spec };
        assert_eq!(progress(&stopped, &[], day(2026, 11, 1)).near_term_minor, Some(0));
    }

    #[test]
    fn nitroeggs_keeps_quote_and_payout_apart() {
        let spec = Spec {
            cadence: "every_n_days".into(),
            interval_days: 10,
            start: day(2026, 11, 1),
            end_kind: "none".into(),
            end_count: 0,
            end_date: None,
            amount_minor: Some(1_400),
            skipped: Vec::new(),
            status: "active".into(),
        };
        let dates = occurrence_dates(&spec, day(2026, 11, 12));
        assert_eq!(&dates[..3], &[day(2026, 11, 1), day(2026, 11, 11), day(2026, 11, 21)]);
        let owed = dates.iter().filter(|date| **date <= day(2026, 11, 12)).count();
        assert_eq!(owed, 2);
        assert_eq!(owed as i64 * 1_400, 2_800);
    }

    #[test]
    fn phrases_program_the_examples() {
        let today = day(2026, 11, 12);
        let course = parse_phrase("I'm paying a course for 9 months at 270 euros", today, "EUR").unwrap();
        assert_eq!(course.kind, "schedule");
        assert_eq!(course.amount, "270");
        assert_eq!(course.currency, "EUR");
        assert_eq!(course.end_count, 9);
        assert_eq!(course.cadence, "monthly");
        assert_eq!(course.direction, "expense");
        let rent = parse_phrase("rent is 2000 francs a month for as long as I live here", today, "EUR").unwrap();
        assert_eq!(rent.name, "Rent");
        assert_eq!(rent.amount, "2000");
        assert_eq!(rent.currency, "CHF");
        assert_eq!(rent.end_kind, "none");
        assert_eq!(rent.cadence, "monthly");
        let eggs = parse_phrase("I have 700 dollars in Nitroeggs that pays 14 dollars every 10 days in XMR", today, "EUR").unwrap();
        assert_eq!(eggs.kind, "payout");
        assert_eq!(eggs.account_name, "Nitroeggs");
        assert_eq!(eggs.balance, "700");
        assert_eq!(eggs.balance_currency, "USD");
        assert_eq!(eggs.amount, "14");
        assert_eq!(eggs.currency, "USD");
        assert_eq!(eggs.amount_mode, "quote");
        assert_eq!(eggs.payout_currency, "XMR");
        assert_eq!(eggs.interval_days, 10);
        let paid = parse_phrase("I got paid 3000 this month", today, "EUR").unwrap();
        assert_eq!(paid.kind, "transaction");
        assert_eq!(paid.direction, "income");
        assert_eq!(paid.amount, "3000");
        assert_eq!(paid.currency, "EUR");
        let spent = parse_phrase("I spent 40 on groceries", today, "CHF").unwrap();
        assert_eq!(spent.kind, "transaction");
        assert_eq!(spent.amount, "40");
        assert_eq!(spent.currency, "CHF");
        assert_eq!(spent.category, "Groceries");
        let june = parse_phrase("I will get 500 euros in June", today, "USD").unwrap();
        assert_eq!(june.kind, "schedule");
        assert_eq!(june.purpose, "one_off");
        assert_eq!(june.direction, "income");
        assert_eq!(june.amount, "500");
        assert_eq!(june.currency, "EUR");
        assert_eq!(june.start_date, "2027-06-01");
        let every = parse_phrase("add a 20 CHF payment every 10 days starting tomorrow", today, "EUR").unwrap();
        assert_eq!(every.kind, "schedule");
        assert_eq!(every.amount, "20");
        assert_eq!(every.currency, "CHF");
        assert_eq!(every.cadence, "every_n_days");
        assert_eq!(every.interval_days, 10);
        assert_eq!(every.start_date, "2026-11-13");
        assert_eq!(every.end_kind, "none");
        let weeks = parse_phrase("cambia el gym a cada 2 semanas", today, "EUR").unwrap();
        assert_eq!(weeks.kind, "edit");
        assert_eq!(weeks.name.to_lowercase(), "gym");
        assert_eq!(weeks.cadence, "every_n_weeks");
        assert_eq!(weeks.interval_days, 2);
        assert_eq!(parse_phrase("ändere gym auf alle 2 wochen", today, "EUR").unwrap().cadence, "every_n_weeks");
        assert_eq!(parse_phrase("muda o gym para cada 2 semanas", today, "EUR").unwrap().interval_days, 2);
        assert_eq!(parse_phrase("canvia el gym a cada 2 setmanes", today, "EUR").unwrap().cadence, "every_n_weeks");
        let months = parse_phrase("add a 15 EUR expense every 3 months", today, "CHF").unwrap();
        assert_eq!(months.cadence, "every_n_months");
        assert_eq!(months.interval_days, 3);
        let years = parse_phrase("a cada 2 anos", today, "EUR");
        assert!(years.is_err() || years.unwrap().cadence == "every_n_years");
        let renamed = parse_phrase("rename gym to fitness", today, "EUR").unwrap();
        assert_eq!(renamed.kind, "edit");
        assert_eq!(renamed.new_name.to_lowercase(), "fitness");
        let amount = parse_phrase("change gym to 30 CHF", today, "EUR").unwrap();
        assert_eq!(amount.amount, "30");
        assert_eq!(amount.currency, "CHF");
        assert_eq!(parse_phrase("delete gym", today, "EUR").unwrap().action, "delete");
        assert_eq!(parse_phrase("yes delete gym", today, "EUR").unwrap().action, "delete_yes");
        assert_eq!(parse_phrase("list recurring payments", today, "EUR").unwrap().kind, "list");
        let anchor = Spec {
            cadence: "monthly".into(),
            interval_days: 0,
            start: day(2024, 1, 31),
            end_kind: "after_count".into(),
            end_count: 4,
            end_date: None,
            amount_minor: Some(1),
            skipped: Vec::new(),
            status: "active".into(),
        };
        let clamped = occurrence_dates(&anchor, day(2024, 1, 31));
        assert_eq!(clamped, vec![day(2024, 1, 31), day(2024, 2, 29), day(2024, 3, 31), day(2024, 4, 30)]);
        let leap = nth_date(day(2024, 2, 29), "yearly", 0, 1).unwrap();
        assert_eq!(leap, day(2025, 2, 28));
        let weeks = Spec { cadence: "every_n_weeks".into(), interval_days: 2, start: day(2026, 11, 1), end_kind: "after_count".into(), end_count: 3, end_date: None, amount_minor: None, skipped: Vec::new(), status: "active".into() };
        assert_eq!(occurrence_dates(&weeks, day(2026, 11, 1)), vec![day(2026, 11, 1), day(2026, 11, 15), day(2026, 11, 29)]);
        let stop = parse_phrase("stop the Netflix subscription", today, "EUR").unwrap();
        assert_eq!(stop.kind, "action");
        assert_eq!(stop.action, "stop");
        assert_eq!(stop.name, "Netflix");
    }

    #[test]
    fn net_worth_stays_per_currency() {
        let rows = worth_rows(&[
            WorthAccount { kind: "bank".into(), currency: "EUR".into(), shown_minor: Some(100_000) },
            WorthAccount { kind: "cash".into(), currency: "EUR".into(), shown_minor: Some(5_000) },
            WorthAccount { kind: "investment".into(), currency: "USD".into(), shown_minor: Some(70_000) },
            WorthAccount { kind: "credit_card".into(), currency: "EUR".into(), shown_minor: Some(20_000) },
        ]);
        let eur = rows.iter().find(|row| row.currency == "EUR").unwrap();
        assert_eq!(eur.cash_minor, 105_000);
        assert_eq!(eur.owed_minor, 20_000);
        assert_eq!(eur.net_minor, 85_000);
        assert_eq!(rows.iter().find(|row| row.currency == "USD").unwrap().invested_minor, 70_000);
        assert_eq!(rows.len(), 2);
        assert_ne!(eur.net_minor, 26_000);
    }
}

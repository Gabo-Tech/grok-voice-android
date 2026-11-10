//! On-device Finance. Accounts, transactions, and recurring rules live in
//! `finance.json`. A payment, due date, or income date is also an event in
//! this app's Calendar so Time and Spoken alerts can see it.
//!
//! Balances are whatever the user last stated. Logging money can move a stated
//! balance. It never creates one. Card numbers, account numbers, and PINs are refused.

use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::Mutex,
};

use chrono::{Datelike, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{
    actions::{Agenda, CalEvent},
    recurrence,
};

const SECRET: &str = "Use a short nickname. Card numbers, account numbers, and PINs are not saved.";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    /// `bank`, `cash`, `credit_card`, `loan`, `investment`, or `other`.
    pub kind: String,
    #[serde(default)]
    pub institution: String,
    #[serde(default)]
    pub balance_minor: Option<i64>,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub balance_as_of: String,
    #[serde(default)]
    pub balance_updated_ms: i64,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub payment_minor: Option<i64>,
    #[serde(default)]
    pub cadence: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub updated_ms: i64,
    #[serde(default)]
    pub ticker: String,
    #[serde(default)]
    pub contribution_minor: Option<i64>,
    #[serde(default)]
    pub contribution_cadence: String,
    #[serde(default)]
    pub contribution_date: String,
    #[serde(default)]
    pub calendar_event_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: String,
    pub amount_minor: i64,
    pub currency: String,
    /// `income`, `expense`, or `payment`.
    pub direction: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub from_account_id: String,
    pub date: String,
    #[serde(default)]
    pub notes: String,
    /// `once`, `recurring`, or `irregular`.
    #[serde(default = "once_class")]
    pub recurrence: String,
    #[serde(default)]
    pub rule_id: String,
    pub created_ms: i64,
    #[serde(default)]
    pub updated_ms: i64,
}

fn once_class() -> String {
    "once".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub amount_minor: Option<i64>,
    #[serde(default)]
    pub currency: String,
    pub direction: String,
    pub cadence: String,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub next_date: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub calendar_event_id: String,
    #[serde(default)]
    pub updated_ms: i64,
    #[serde(default)]
    pub from_account: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Store {
    accounts: Vec<Account>,
    transactions: Vec<Transaction>,
    rules: Vec<Rule>,
    next_id: u64,
    locale_hint: String,
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub accounts: Vec<Account>,
    pub transactions: Vec<Transaction>,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    #[serde(flatten)]
    pub account: Account,
    pub shown_balance_minor: Option<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodTotal {
    pub currency: String,
    pub income_minor: i64,
    pub expense_minor: i64,
    pub payment_minor: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Upcoming {
    pub id: String,
    pub source: String,
    pub title: String,
    pub date: String,
    pub amount_minor: Option<i64>,
    pub currency: String,
    pub direction: String,
    pub calendar_event_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub accounts: Vec<AccountView>,
    pub transactions: Vec<Transaction>,
    pub rules: Vec<Rule>,
    pub totals: Vec<PeriodTotal>,
    pub upcoming: Vec<Upcoming>,
    pub period: String,
    pub default_currency: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDraft {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub institution: String,
    #[serde(default)]
    pub balance: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub payment: String,
    #[serde(default)]
    pub cadence: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub ticker: String,
    #[serde(default)]
    pub contribution: String,
    #[serde(default)]
    pub contribution_cadence: String,
    #[serde(default)]
    pub contribution_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDraft {
    #[serde(default)]
    pub id: String,
    pub amount: String,
    #[serde(default)]
    pub currency: String,
    pub direction: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub from_account_id: String,
    #[serde(default)]
    pub from_account_name: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub recurrence: String,
    #[serde(default)]
    pub cadence: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDraft {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub amount: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub cadence: String,
    #[serde(default)]
    pub next_date: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub weekdays: Vec<u8>,
}

struct Clock {
    ms: i64,
    today: NaiveDate,
    currency: String,
}

enum Role {
    Primary,
    Source,
}

pub struct Finance {
    path: PathBuf,
    inner: Mutex<Store>,
}

impl Finance {
    pub fn load(path: PathBuf) -> Self {
        let inner = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Self { path, inner: Mutex::new(inner) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn persist(path: &PathBuf, store: &Store) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(store).map_err(|err| err.to_string())?;
        crate::files::write_private(path, &bytes).map_err(|err| err.to_string())
    }

    pub fn remember_locale(&self, tag: &str) {
        let tag = tag.trim();
        if tag.is_empty() {
            return;
        }
        let mut store = self.lock();
        if store.locale_hint == tag {
            return;
        }
        store.locale_hint = tag.to_string();
        let _ = Self::persist(&self.path, &store);
    }

    pub fn snapshot(&self, app: &AppHandle) -> Snapshot {
        let store = self.lock();
        snapshot_of(&store, &currency_locale(app, &store), Local::now().date_naive())
    }

    pub fn bundle(&self) -> Bundle {
        let store = self.lock();
        Bundle { accounts: store.accounts.clone(), transactions: store.transactions.clone(), rules: store.rules.clone() }
    }

    pub fn replace(&self, app: &AppHandle, bundle: Bundle) -> Result<(), String> {
        let mut store = self.lock().clone();
        store.accounts = bundle.accounts;
        store.transactions = bundle.transactions;
        store.rules = bundle.rules;
        self.commit(app, store).map(|_| ())
    }

    pub fn save_account(&self, app: &AppHandle, draft: AccountDraft) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        apply_account_form(&mut store, &draft, &clock)?;
        self.commit(app, store)
    }

    pub fn save_transaction(&self, app: &AppHandle, draft: TransactionDraft) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        apply_transaction_form(&mut store, &draft, &clock)?;
        self.commit(app, store)
    }

    pub fn save_rule(&self, app: &AppHandle, draft: RuleDraft) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        apply_rule_form(&mut store, &draft, &clock)?;
        self.commit(app, store)
    }

    pub fn delete_account(&self, app: &AppHandle, id: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        remove_account(&mut store, id)?;
        self.commit(app, store)
    }

    pub fn delete_transaction(&self, app: &AppHandle, id: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let before = store.transactions.len();
        store.transactions.retain(|tx| tx.id != id);
        if store.transactions.len() == before {
            return Err("No transaction with that id.".into());
        }
        self.commit(app, store)
    }

    pub fn delete_rule(&self, app: &AppHandle, id: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let before = store.rules.len();
        store.rules.retain(|rule| rule.id != id);
        if store.rules.len() == before {
            return Err("No recurring item with that id.".into());
        }
        self.commit(app, store)
    }

    fn commit(&self, app: &AppHandle, mut store: Store) -> Result<Snapshot, String> {
        let previous = self.lock().clone();
        link_calendar(app, &mut store, Local::now());
        let snap = snapshot_of(&store, &currency_locale(app, &store), Local::now().date_naive());
        Self::persist(&self.path, &store)?;
        gc_events(app, &previous, &store);
        *self.lock() = store;
        touch(app);
        Ok(snap)
    }
}

fn clock_for(app: &AppHandle, store: &Store) -> Clock {
    let now = Local::now();
    Clock { ms: now.timestamp_millis(), today: now.date_naive(), currency: currency_locale(app, store) }
}

fn currency_locale(app: &AppHandle, store: &Store) -> String {
    if !store.locale_hint.trim().is_empty() {
        return store.locale_hint.clone();
    }
    let saved = app.state::<crate::settings::SettingsStore>().get().locale;
    if !saved.trim().is_empty() {
        return saved;
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().to_string();
            if !value.is_empty() && value != "C" && value != "POSIX" {
                return value;
            }
        }
    }
    String::new()
}

pub fn currency_for_locale(tag: &str) -> &'static str {
    let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
    let tag = tag.split('.').next().unwrap_or(tag.as_str());
    let tag = tag.split('@').next().unwrap_or(tag);
    const ORDERED: &[(&str, &str)] = &[
        ("en-gb", "GBP"),
        ("en-au", "AUD"),
        ("en-ca", "CAD"),
        ("en-nz", "NZD"),
        ("en-ie", "EUR"),
        ("en-in", "INR"),
        ("en-ch", "CHF"),
        ("pt-br", "BRL"),
        ("es-mx", "MXN"),
        ("es-ar", "ARS"),
        ("es-cl", "CLP"),
        ("es-co", "COP"),
        ("es-us", "USD"),
        ("de-ch", "CHF"),
        ("fr-ch", "CHF"),
        ("it-ch", "CHF"),
        ("zh-cn", "CNY"),
        ("zh-tw", "TWD"),
        ("zh-hk", "HKD"),
        ("ja", "JPY"),
        ("ko", "KRW"),
        ("sv", "SEK"),
        ("nb", "NOK"),
        ("nn", "NOK"),
        ("no", "NOK"),
        ("da", "DKK"),
        ("pl", "PLN"),
        ("cs", "CZK"),
        ("hu", "HUF"),
        ("ro", "RON"),
        ("tr", "TRY"),
        ("hi", "INR"),
        ("de", "EUR"),
        ("es", "EUR"),
        ("fr", "EUR"),
        ("it", "EUR"),
        ("nl", "EUR"),
        ("pt", "EUR"),
        ("ca", "EUR"),
        ("eu", "EUR"),
        ("en", "USD"),
    ];
    for (prefix, code) in ORDERED {
        if tag == *prefix || tag.starts_with(&format!("{prefix}-")) {
            return code;
        }
    }
    "USD"
}

pub fn tool_definitions() -> Value {
    json!([
        fn_tool(
            "log_money",
            "Log an expense, income, or card or loan payment on this phone. amount is the number they said. currency is EUR, euros, USD, or whatever they named; omit it only when they did not name one. direction is expense, income, or payment. account is a nickname such as cash or Visa travel. accountKind is bank, cash, credit_card, loan, investment, or other when creating that nickname. fromAccount is the bank or cash a card or loan payment came from. date is YYYY-MM-DD, tomorrow, or a day of the month. recurrence is once, recurring, or irregular. cadence is daily, weekly, monthly, or yearly for a repeating one. Do not invent a balance, due date, or account number. A future payment, due, or income date is also saved on this app's Calendar.",
            json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "category": { "type": "string" },
                    "account": { "type": "string" },
                    "accountKind": { "type": "string" },
                    "fromAccount": { "type": "string" },
                    "date": { "type": "string" },
                    "recurrence": { "type": "string" },
                    "cadence": { "type": "string" },
                    "notes": { "type": "string" }
                },
                "required": ["direction", "amount"]
            })
        ),
        fn_tool(
            "save_recurring",
            "Save a repeating bill, payday, subscription, or automated investment on this phone. name is what they called it. amount may be omitted when they only named a date. cadence is daily, weekly, monthly, or yearly. date is YYYY-MM-DD or a day such as 1. direction is expense, income, or payment. Do not invent an amount or a date they did not say. A date is also saved on this app's Calendar.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "direction": { "type": "string" },
                    "cadence": { "type": "string" },
                    "date": { "type": "string" },
                    "day": { "type": "integer" },
                    "category": { "type": "string" },
                    "account": { "type": "string" },
                    "accountKind": { "type": "string" },
                    "notes": { "type": "string" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "save_account",
            "Save a bank, cash, card, loan, or investment the user described. kind is bank, cash, credit_card, loan, investment, or other. balance is only the figure they stated; omit it to leave the saved balance alone. currency when they name one. dueDate is YYYY-MM-DD or a day of the month. payment is a typical or minimum payment they stated. ticker and contributionAmount are for an investment they named, and contributionCadence is how often that buy repeats. Never ask for or store a card number, account number, or PIN. A due date or contribution date is also saved on this app's Calendar.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "kind": { "type": "string" },
                    "balance": { "type": "string" },
                    "currency": { "type": "string" },
                    "institution": { "type": "string" },
                    "dueDate": { "type": "string" },
                    "payment": { "type": "string" },
                    "cadence": { "type": "string" },
                    "ticker": { "type": "string" },
                    "contributionAmount": { "type": "string" },
                    "contributionCadence": { "type": "string" },
                    "contributionDate": { "type": "string" },
                    "notes": { "type": "string" }
                },
                "required": ["name", "kind"]
            })
        ),
        fn_tool(
            "list_finance",
            "Read Finance saved on this phone, without guessing amounts. view is summary, accounts, activity, upcoming, or investments. month is YYYY-MM. kind is bank, cash, credit_card, loan, or investment. account is a nickname. Speak the detail. Do not invent a balance that is not in the result.",
            json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string" },
                    "month": { "type": "string" },
                    "kind": { "type": "string" },
                    "account": { "type": "string" },
                    "direction": { "type": "string" },
                    "recurrence": { "type": "string" },
                    "query": { "type": "string" }
                }
            })
        ),
        fn_tool(
            "delete_finance",
            "Delete one finance account, transaction, or recurring rule after a clear yes. Call first with confirmed false, say what will be removed, then call with confirmed true. kind is account, transaction, or rule. id comes from list_finance.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string" },
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["kind", "confirmed"]
            })
        )
    ])
}

fn fn_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({ "type": "function", "name": name, "description": description, "parameters": parameters })
}

pub fn try_tool(app: &AppHandle, name: &str, args: &Value) -> Result<Option<Value>, String> {
    if !matches!(name, "log_money" | "save_recurring" | "save_account" | "list_finance" | "delete_finance") {
        return Ok(None);
    }
    let finance = app.state::<Finance>();
    if name == "list_finance" {
        let store = finance.lock().clone();
        let today = Local::now().date_naive();
        let ask = ask_from(args);
        let detail = speak(&store, &ask, today);
        let snap = snapshot_of(&store, &currency_locale(app, &store), today);
        return Ok(Some(json!({
            "ok": true,
            "detail": detail,
            "period": snap.period,
            "totals": snap.totals,
            "upcoming": snap.upcoming,
            "accounts": snap.accounts.iter().map(|row| json!({
                "id": row.account.id,
                "name": row.account.name,
                "kind": row.account.kind,
                "currency": row.account.currency,
                "shownBalanceMinor": row.shown_balance_minor,
                "balanceSaved": row.account.balance_minor.is_some(),
                "dueDate": row.account.due_date,
                "calendarEventId": row.account.calendar_event_id,
            })).collect::<Vec<_>>()
        })));
    }
    if name == "delete_finance" {
        if !args["confirmed"].as_bool().unwrap_or(false) {
            let store = finance.lock().clone();
            let detail = describe_delete(&store, args)?;
            return Ok(Some(json!({ "ok": false, "needsConfirm": true, "detail": detail })));
        }
        let mut store = finance.lock().clone();
        let detail = apply_delete(&mut store, args)?;
        finance.commit(app, store)?;
        return Ok(Some(json!({ "ok": true, "detail": detail })));
    }
    let mut store = finance.lock().clone();
    let clock = clock_for(app, &store);
    let detail = match name {
        "log_money" => log_from_tool(&mut store, args, &clock)?,
        "save_recurring" => rule_from_tool(&mut store, args, &clock)?,
        "save_account" => account_from_tool(&mut store, args, &clock)?,
        _ => return Ok(None),
    };
    finance.commit(app, store)?;
    Ok(Some(json!({ "ok": true, "detail": detail })))
}

fn ask_from(args: &Value) -> Ask {
    Ask {
        view: fold(args["view"].as_str().unwrap_or("")),
        month: args["month"].as_str().unwrap_or("").trim().to_string(),
        kind: normalize_kind(args["kind"].as_str().unwrap_or("")),
        account: args["account"].as_str().unwrap_or("").trim().to_string(),
        direction: args["direction"].as_str().unwrap_or("").trim().to_string(),
        recurrence: normalize_class(args["recurrence"].as_str().unwrap_or("")),
        query: args["query"].as_str().unwrap_or("").trim().to_string(),
    }
}

struct Ask {
    view: String,
    month: String,
    kind: String,
    account: String,
    direction: String,
    recurrence: String,
    query: String,
}

fn log_from_tool(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let direction = normalize_direction(args["direction"].as_str().unwrap_or(""))?;
    let (amount_minor, currency) = resolve_money(
        &amount_text(args.get("amount")),
        args["currency"].as_str().unwrap_or(""),
        &clock.currency,
    )?;
    let cadence = normalize_cadence(args["cadence"].as_str().unwrap_or(""));
    let class = classify(args["recurrence"].as_str().unwrap_or(""), &cadence);
    let provided = require_date(args["date"].as_str().unwrap_or(""), &cadence, clock.today)?;
    let date = if provided.is_empty() { iso(clock.today) } else { provided.clone() };
    let account_id = resolve_account(
        store,
        "",
        args["account"].as_str().unwrap_or(""),
        args["accountKind"].as_str().unwrap_or(""),
        &currency,
        clock,
        true,
    )?;
    let from_account_id = resolve_account(
        store,
        "",
        args["fromAccount"].as_str().unwrap_or(""),
        "",
        &currency,
        clock,
        true,
    )?;
    let category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    let notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    let link = class == "recurring" || is_link_date(&provided, &direction, clock.today);
    let rule_id = if link {
        let name = if category.is_empty() {
            account_name(store, &account_id).unwrap_or_else(|| direction.clone())
        } else {
            category.clone()
        };
        let rule_cadence = if class == "irregular" { String::new() } else { cadence.clone() };
        Some(upsert_named_rule(
            store,
            "",
            &name,
            Some(amount_minor),
            &currency,
            &direction,
            &rule_cadence,
            &provided,
            &category,
            &account_id,
            &notes,
            &[],
            false,
            clock,
        )?)
    } else {
        None
    };
    let linked = rule_id.is_some();
    let id = alloc(store, "ft");
    store.transactions.push(Transaction {
        id,
        amount_minor,
        currency: currency.clone(),
        direction: direction.clone(),
        category: category.clone(),
        account_id: account_id.clone(),
        from_account_id,
        date: date.clone(),
        notes,
        recurrence: class.into(),
        rule_id: rule_id.unwrap_or_default(),
        created_ms: clock.ms,
        updated_ms: clock.ms,
    });
    let label = if category.is_empty() { direction.clone() } else { category };
    let mut detail = format!("Saved {label} of {} on {date}.", format_minor(amount_minor, &currency));
    if let Some(name) = account_name(store, &account_id) {
        detail.push_str(&format!(" Account: {name}."));
        detail.push_str(&balance_clause(store, &account_id));
    }
    if linked {
        detail.push_str(&calendar_clause(&cadence, &provided));
    }
    Ok(detail)
}

fn rule_from_tool(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let name = clean_text(first_str(args, &["name", "title"]), 80)?;
    if name.is_empty() {
        return Err("A recurring item needs a name.".into());
    }
    let cadence = normalize_cadence(args["cadence"].as_str().unwrap_or(""));
    if !recurrence::is_repeating(&cadence) && args["cadence"].as_str().map(str::trim).filter(|s| !s.is_empty()).is_some() {
        return Err("Cadence is daily, weekly, monthly, or yearly.".into());
    }
    let date_raw = if let Some(day) = args["day"].as_i64().filter(|day| (1..=31).contains(day)) {
        iso(next_month_day(day as u32, clock.today))
    } else {
        require_date(first_str(args, &["date", "dueDate", "nextDate"]), &cadence, clock.today)?
    };
    let (amount, currency) = if amount_text(args.get("amount")).trim().is_empty() && args.get("amount").is_none() {
        (None, args["currency"].as_str().unwrap_or("").trim().to_string())
    } else if amount_text(args.get("amount")).trim().is_empty() {
        (None, String::new())
    } else {
        let (minor, currency) = resolve_money(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(""), &clock.currency)?;
        (Some(minor), currency)
    };
    let direction = if args["direction"].as_str().map(str::trim).filter(|s| !s.is_empty()).is_some() {
        normalize_direction(args["direction"].as_str().unwrap_or(""))?
    } else if fold(&name).contains("salary") || fold(&name).contains("paycheck") {
        "income".into()
    } else {
        "expense".into()
    };
    let account_id = resolve_account(store, "", args["account"].as_str().unwrap_or(""), args["accountKind"].as_str().unwrap_or(""), if currency.is_empty() { &clock.currency } else { &currency }, clock, true)?;
    let category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    let notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    let id = args["id"].as_str().unwrap_or("").trim().to_string();
    let saved = upsert_named_rule(store, &id, &name, amount, &currency, &direction, &cadence, &date_raw, &category, &account_id, &notes, &[], false, clock)?;
    let money = match amount {
        Some(minor) => format_minor(minor, if currency.is_empty() { &clock.currency } else { &currency }),
        None => "no amount".into(),
    };
    let mut detail = format!("Saved {name}, {money}.");
    if recurrence::is_repeating(&cadence) {
        detail.push_str(&format!(" Repeats {cadence}."));
    }
    detail.push_str(&calendar_clause(&cadence, &date_raw));
    let _ = saved;
    Ok(detail)
}

fn account_from_tool(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let name = clean_text(args["name"].as_str().unwrap_or(""), 80)?;
    if name.is_empty() {
        return Err("An account needs a nickname.".into());
    }
    let kind = normalize_kind(args["kind"].as_str().unwrap_or(""));
    if kind.is_empty() {
        return Err("Kind is bank, cash, credit_card, loan, investment, or other.".into());
    }
    let id = match find_account(store, args["id"].as_str().unwrap_or(""), &name)? {
        Some(index) => store.accounts[index].id.clone(),
        None => String::new(),
    };
    let mut draft = AccountDraft {
        id,
        name,
        kind,
        institution: clean_text(args["institution"].as_str().unwrap_or(""), 40)?,
        balance: String::new(),
        currency: args["currency"].as_str().unwrap_or("").trim().to_string(),
        due_date: String::new(),
        payment: String::new(),
        cadence: normalize_cadence(args["cadence"].as_str().unwrap_or("")),
        notes: clean_text(args["notes"].as_str().unwrap_or(""), 500)?,
        ticker: clean_text(args["ticker"].as_str().unwrap_or(""), 12)?,
        contribution: String::new(),
        contribution_cadence: normalize_cadence(args["contributionCadence"].as_str().unwrap_or("")),
        contribution_date: String::new(),
    };
    let partial = id_present(store, &draft.id);
    if args.get("balance").is_some() || args.get("balance").and_then(|v| v.as_str()).is_some() {
        draft.balance = amount_text(args.get("balance"));
    } else if !partial {
        draft.balance.clear();
    }
    if args.get("dueDate").is_some() {
        draft.due_date = require_date(args["dueDate"].as_str().unwrap_or(""), &draft.cadence, clock.today)?;
    }
    if args.get("payment").is_some() {
        draft.payment = amount_text(args.get("payment"));
    }
    if args.get("contributionAmount").is_some() {
        draft.contribution = amount_text(args.get("contributionAmount"));
    }
    if args.get("contributionDate").is_some() {
        draft.contribution_date = require_date(args["contributionDate"].as_str().unwrap_or(""), &draft.contribution_cadence, clock.today)?;
    }
    let present_balance = args.get("balance").is_some();
    let present_due = args.get("dueDate").is_some();
    let present_payment = args.get("payment").is_some();
    let present_contribution = args.get("contributionAmount").is_some();
    let present_contribution_date = args.get("contributionDate").is_some();
    apply_account_fields(
        store,
        &draft,
        clock,
        FieldMask {
            balance: present_balance || !partial,
            due: present_due || !partial,
            payment: present_payment || !partial,
            contribution: present_contribution || !partial,
            contribution_date: present_contribution_date || !partial,
            replace_text: !partial,
        },
    )
}

fn id_present(store: &Store, id: &str) -> bool {
    !id.is_empty() && store.accounts.iter().any(|account| account.id == id)
}

struct FieldMask {
    balance: bool,
    due: bool,
    payment: bool,
    contribution: bool,
    contribution_date: bool,
    replace_text: bool,
}

fn apply_account_form(store: &mut Store, draft: &AccountDraft, clock: &Clock) -> Result<String, String> {
    apply_account_fields(
        store,
        draft,
        clock,
        FieldMask { balance: true, due: true, payment: true, contribution: true, contribution_date: true, replace_text: true },
    )
}

fn apply_account_fields(store: &mut Store, draft: &AccountDraft, clock: &Clock, mask: FieldMask) -> Result<String, String> {
    let name = clean_text(&draft.name, 80)?;
    if name.is_empty() {
        return Err("An account needs a nickname.".into());
    }
    let kind = {
        let explicit = normalize_kind(&draft.kind);
        if explicit.is_empty() { infer_kind(&name, "") } else { explicit }
    };
    let mut account = if draft.id.trim().is_empty() {
        let id = alloc(store, "fa");
        empty_account(id)
    } else {
        store.accounts.iter().find(|account| account.id == draft.id.trim()).cloned().ok_or("No account with that id.")?
    };
    account.name = name;
    account.kind = kind;
    if mask.replace_text || !draft.institution.trim().is_empty() {
        account.institution = clean_text(&draft.institution, 40)?;
    }
    if mask.replace_text || !draft.notes.trim().is_empty() {
        account.notes = clean_text(&draft.notes, 500)?;
    }
    if mask.replace_text || !draft.ticker.trim().is_empty() {
        account.ticker = clean_text(&draft.ticker, 12)?;
    }
    let fallback_currency = if account.currency.is_empty() { clock.currency.clone() } else { account.currency.clone() };
    if !draft.currency.trim().is_empty() {
        account.currency = canonical_currency(&draft.currency)?.to_string();
    } else if account.currency.is_empty() {
        account.currency = currency_for_locale(&fallback_currency).into();
    }
    if mask.balance {
        if draft.balance.trim().is_empty() {
            account.balance_minor = None;
            account.balance_as_of.clear();
            account.balance_updated_ms = 0;
        } else {
            let (minor, currency) = resolve_money(&draft.balance, &account.currency, &clock.currency)?;
            if account.balance_minor != Some(minor) || account.currency != currency {
                account.balance_minor = Some(minor);
                account.currency = currency;
                account.balance_updated_ms = clock.ms;
                account.balance_as_of = iso(clock.today);
            }
        }
    }
    if mask.due {
        account.due_date = require_date(&draft.due_date, &draft.cadence, clock.today)?;
    }
    if !draft.cadence.trim().is_empty() || mask.due {
        let cadence = normalize_cadence(&draft.cadence);
        if !draft.cadence.trim().is_empty() && !cadence.is_empty() && !recurrence::is_repeating(&cadence) {
            return Err("Cadence is daily, weekly, monthly, or yearly.".into());
        }
        if mask.due || !draft.cadence.trim().is_empty() {
            account.cadence = cadence;
        }
    }
    if mask.payment {
        account.payment_minor = if draft.payment.trim().is_empty() {
            None
        } else {
            Some(resolve_money(&draft.payment, &account.currency, &clock.currency)?.0)
        };
    }
    if !draft.contribution_cadence.trim().is_empty() {
        account.contribution_cadence = normalize_cadence(&draft.contribution_cadence);
    } else if mask.contribution_date {
        account.contribution_cadence.clear();
    }
    if mask.contribution_date {
        account.contribution_date = require_date(&draft.contribution_date, &account.contribution_cadence, clock.today)?;
    }
    if mask.contribution {
        account.contribution_minor = if draft.contribution.trim().is_empty() {
            None
        } else {
            Some(resolve_money(&draft.contribution, &account.currency, &clock.currency)?.0)
        };
    }
    account.updated_ms = clock.ms;
    let id = account.id.clone();
    let detail = account_detail(&account);
    if let Some(existing) = store.accounts.iter_mut().find(|row| row.id == id) {
        *existing = account.clone();
    } else {
        store.accounts.push(account.clone());
    }
    sync_generated_rule(store, &account, clock);
    Ok(detail)
}

fn account_detail(account: &Account) -> String {
    let mut parts = vec![format!("Saved {}.", account.name)];
    match account.balance_minor {
        Some(minor) => parts.push(format!("Stated balance {}.", format_minor(minor, &account.currency))),
        None => parts.push("No balance was stated.".into()),
    }
    let dated = if account.kind == "investment" { account.contribution_date.as_str() } else { account.due_date.as_str() };
    if dated.is_empty() {
        parts.push("No due date was set, so Calendar was left alone.".into());
    } else {
        parts.push(format!("Date {dated} is on this app's Calendar."));
    }
    parts.join(" ")
}

fn sync_generated_rule(store: &mut Store, account: &Account, clock: &Clock) {
    let (amount, cadence, date, direction) = if account.kind == "investment" {
        (account.contribution_minor, account.contribution_cadence.clone(), account.contribution_date.clone(), "payment".into())
    } else if account.kind == "credit_card" || account.kind == "loan" {
        (account.payment_minor, account.cadence.clone(), account.due_date.clone(), "payment".into())
    } else {
        (account.payment_minor, account.cadence.clone(), account.due_date.clone(), "expense".into())
    };
    let wanted = recurrence::is_repeating(&cadence) || !date.is_empty();
    if !wanted {
        store.rules.retain(|rule| !(rule.from_account && rule.account_id == account.id));
        return;
    }
    if let Some(rule) = store.rules.iter_mut().find(|rule| rule.from_account && rule.account_id == account.id) {
        rule.name = account.name.clone();
        rule.amount_minor = amount;
        rule.currency = account.currency.clone();
        rule.direction = direction;
        rule.cadence = cadence;
        rule.next_date = date;
        rule.category = account.kind.clone();
        rule.updated_ms = clock.ms;
        return;
    }
    let id = alloc(store, "fr");
    store.rules.push(Rule {
        id,
        name: account.name.clone(),
        amount_minor: amount,
        currency: account.currency.clone(),
        direction,
        cadence,
        weekdays: Vec::new(),
        next_date: date,
        category: account.kind.clone(),
        account_id: account.id.clone(),
        notes: String::new(),
        calendar_event_id: String::new(),
        updated_ms: clock.ms,
        from_account: true,
    });
}

fn apply_transaction_form(store: &mut Store, draft: &TransactionDraft, clock: &Clock) -> Result<(), String> {
    let direction = normalize_direction(&draft.direction)?;
    let (amount_minor, currency) = resolve_money(&draft.amount, &draft.currency, &clock.currency)?;
    let cadence = normalize_cadence(&draft.cadence);
    let class = classify(&draft.recurrence, &cadence);
    let provided = require_date(&draft.date, &cadence, clock.today)?;
    let date = if provided.is_empty() { iso(clock.today) } else { provided.clone() };
    let account_id = resolve_account(store, &draft.account_id, &draft.account_name, "", &currency, clock, true)?;
    let from_account_id = resolve_account(store, &draft.from_account_id, &draft.from_account_name, "", &currency, clock, true)?;
    let category = clean_text(&draft.category, 80)?;
    let notes = clean_text(&draft.notes, 500)?;
    let link = class == "recurring" || is_link_date(&provided, &direction, clock.today);
    let rule_id = if link {
        let name = if category.is_empty() { direction.clone() } else { category.clone() };
        let rule_cadence = if class == "irregular" { String::new() } else { cadence.clone() };
        upsert_named_rule(store, "", &name, Some(amount_minor), &currency, &direction, &rule_cadence, &provided, &category, &account_id, &notes, &[], false, clock)?
    } else {
        String::new()
    };
    let tx = Transaction {
        id: if draft.id.trim().is_empty() { alloc(store, "ft") } else { draft.id.trim().to_string() },
        amount_minor,
        currency,
        direction,
        category,
        account_id,
        from_account_id,
        date,
        notes,
        recurrence: class.into(),
        rule_id,
        created_ms: clock.ms,
        updated_ms: clock.ms,
    };
    if let Some(existing) = store.transactions.iter_mut().find(|row| row.id == tx.id) {
        let created = existing.created_ms;
        *existing = tx;
        existing.created_ms = created;
        existing.updated_ms = clock.ms;
    } else if !draft.id.trim().is_empty() {
        return Err("No transaction with that id.".into());
    } else {
        store.transactions.push(tx);
    }
    Ok(())
}

fn apply_rule_form(store: &mut Store, draft: &RuleDraft, clock: &Clock) -> Result<(), String> {
    let name = clean_text(&draft.name, 80)?;
    if name.is_empty() {
        return Err("A recurring item needs a name.".into());
    }
    let cadence = normalize_cadence(&draft.cadence);
    let date = require_date(&draft.next_date, &cadence, clock.today)?;
    let (amount, currency) = if draft.amount.trim().is_empty() {
        (None, canonical_or_default(&draft.currency, &clock.currency)?)
    } else {
        let (minor, currency) = resolve_money(&draft.amount, &draft.currency, &clock.currency)?;
        (Some(minor), currency)
    };
    let direction = if draft.direction.trim().is_empty() { "expense".into() } else { normalize_direction(&draft.direction)? };
    let account_id = resolve_account(store, &draft.account_id, &draft.account_name, "", &currency, clock, true)?;
    let category = clean_text(&draft.category, 80)?;
    let notes = clean_text(&draft.notes, 500)?;
    upsert_named_rule(
        store,
        draft.id.trim(),
        &name,
        amount,
        &currency,
        &direction,
        &cadence,
        &date,
        &category,
        &account_id,
        &notes,
        &draft.weekdays,
        false,
        clock,
    )?;
    Ok(())
}

fn upsert_named_rule(
    store: &mut Store,
    id: &str,
    name: &str,
    amount: Option<i64>,
    currency: &str,
    direction: &str,
    cadence: &str,
    date: &str,
    category: &str,
    account_id: &str,
    notes: &str,
    weekdays: &[u8],
    from_account: bool,
    clock: &Clock,
) -> Result<String, String> {
    let existing = if !id.is_empty() {
        Some(store.rules.iter().position(|rule| rule.id == id).ok_or("No recurring item with that id.")?)
    } else {
        store.rules.iter().position(|rule| !rule.from_account && fold(&rule.name) == fold(name) && rule.account_id == account_id)
    };
    if let Some(index) = existing {
        let rule = &mut store.rules[index];
        rule.name = name.to_string();
        rule.amount_minor = amount;
        if !currency.is_empty() {
            rule.currency = currency.to_string();
        }
        rule.direction = direction.to_string();
        rule.cadence = cadence.to_string();
        rule.next_date = date.to_string();
        rule.category = category.to_string();
        rule.account_id = account_id.to_string();
        rule.notes = notes.to_string();
        if !weekdays.is_empty() {
            rule.weekdays = weekdays.to_vec();
        }
        rule.updated_ms = clock.ms;
        rule.from_account = from_account;
        return Ok(rule.id.clone());
    }
    let id = alloc(store, "fr");
    store.rules.push(Rule {
        id: id.clone(),
        name: name.into(),
        amount_minor: amount,
        currency: currency.into(),
        direction: direction.into(),
        cadence: cadence.into(),
        weekdays: weekdays.to_vec(),
        next_date: date.into(),
        category: category.into(),
        account_id: account_id.into(),
        notes: notes.into(),
        calendar_event_id: String::new(),
        updated_ms: clock.ms,
        from_account,
    });
    Ok(id)
}

fn remove_account(store: &mut Store, id: &str) -> Result<(), String> {
    let before = store.accounts.len();
    store.accounts.retain(|account| account.id != id);
    if store.accounts.len() == before {
        return Err("No account with that id.".into());
    }
    store.rules.retain(|rule| !(rule.from_account && rule.account_id == id));
    for rule in &mut store.rules {
        if rule.account_id == id {
            rule.account_id.clear();
        }
    }
    for tx in &mut store.transactions {
        if tx.account_id == id {
            tx.account_id.clear();
        }
        if tx.from_account_id == id {
            tx.from_account_id.clear();
        }
    }
    Ok(())
}

fn apply_delete(store: &mut Store, args: &Value) -> Result<String, String> {
    let kind = fold(args["kind"].as_str().unwrap_or(""));
    let id = args["id"].as_str().unwrap_or("").trim().to_string();
    let name = args["name"].as_str().unwrap_or("").trim().to_string();
    match kind.as_str() {
        "account" => {
            let index = find_account(store, &id, &name)?.ok_or("Which account?")?;
            let label = store.accounts[index].name.clone();
            let id = store.accounts[index].id.clone();
            remove_account(store, &id)?;
            Ok(format!("Deleted the account {label}. Transactions stay in Activity."))
        }
        "transaction" | "entry" => {
            let before = store.transactions.len();
            store.transactions.retain(|tx| tx.id != id);
            if store.transactions.len() == before {
                return Err("No transaction with that id.".into());
            }
            Ok("Deleted that transaction.".into())
        }
        "rule" | "recurring" => {
            let id = if id.is_empty() {
                store.rules.iter().find(|rule| fold(&rule.name) == fold(&name)).map(|rule| rule.id.clone()).ok_or("Which recurring item?")?
            } else {
                id
            };
            let before = store.rules.len();
            store.rules.retain(|rule| rule.id != id);
            if store.rules.len() == before {
                return Err("No recurring item with that id.".into());
            }
            Ok("Deleted that recurring item and its Calendar date.".into())
        }
        _ => Err("Kind is account, transaction, or rule.".into()),
    }
}

fn describe_delete(store: &Store, args: &Value) -> Result<String, String> {
    let kind = fold(args["kind"].as_str().unwrap_or(""));
    let id = args["id"].as_str().unwrap_or("").trim();
    let name = args["name"].as_str().unwrap_or("").trim();
    let label = match kind.as_str() {
        "account" => find_account(store, id, name)?.map(|index| store.accounts[index].name.clone()).ok_or("Which account?")?,
        "transaction" | "entry" => store.transactions.iter().find(|tx| tx.id == id).map(|tx| format!("{} {}", tx.direction, format_minor(tx.amount_minor, &tx.currency))).ok_or("Which transaction?")?,
        "rule" | "recurring" => {
            if !id.is_empty() {
                store.rules.iter().find(|rule| rule.id == id).map(|rule| rule.name.clone()).ok_or("Which recurring item?")?
            } else {
                store.rules.iter().find(|rule| fold(&rule.name) == fold(name)).map(|rule| rule.name.clone()).ok_or("Which recurring item?")?
            }
        }
        _ => return Err("Kind is account, transaction, or rule.".into()),
    };
    Ok(format!("Delete {label}? Say yes and I will remove it."))
}

fn find_account(store: &Store, id: &str, name: &str) -> Result<Option<usize>, String> {
    if !id.trim().is_empty() {
        return store.accounts.iter().position(|account| account.id == id.trim()).map(Some).ok_or_else(|| "No account with that id.".into());
    }
    match match_named(&store.accounts.iter().map(|account| account.name.as_str()).collect::<Vec<_>>(), name) {
        NameMatch::None => Ok(None),
        NameMatch::One(index) => Ok(Some(index)),
        NameMatch::Many(names) => Err(format!("Which account? I found {}.", names.join(" and "))),
    }
}

fn resolve_account(store: &mut Store, id: &str, name: &str, kind: &str, currency: &str, clock: &Clock, create: bool) -> Result<String, String> {
    if id.trim().is_empty() && name.trim().is_empty() {
        return Ok(String::new());
    }
    if let Some(index) = find_account(store, id, name)? {
        return Ok(store.accounts[index].id.clone());
    }
    if !id.trim().is_empty() {
        return Err("No account with that id.".into());
    }
    if !create {
        return Ok(String::new());
    }
    let name = clean_text(name, 80)?;
    let kind = infer_kind(&name, kind);
    let id = alloc(store, "fa");
    let mut account = empty_account(id.clone());
    account.name = name;
    account.kind = kind;
    account.currency = if currency.is_empty() { currency_for_locale(&clock.currency).into() } else { currency.into() };
    account.updated_ms = clock.ms;
    store.accounts.push(account);
    Ok(id)
}

fn empty_account(id: String) -> Account {
    Account {
        id,
        name: String::new(),
        kind: "other".into(),
        institution: String::new(),
        balance_minor: None,
        currency: String::new(),
        balance_as_of: String::new(),
        balance_updated_ms: 0,
        due_date: String::new(),
        payment_minor: None,
        cadence: String::new(),
        notes: String::new(),
        updated_ms: 0,
        ticker: String::new(),
        contribution_minor: None,
        contribution_cadence: String::new(),
        contribution_date: String::new(),
        calendar_event_id: String::new(),
    }
}

fn alloc(store: &mut Store, prefix: &str) -> String {
    store.next_id = store.next_id.saturating_add(1);
    if store.accounts.len() + store.transactions.len() + store.rules.len() > 5_000 {
        store.next_id = store.next_id.saturating_sub(1);
    }
    format!("{prefix}{}", store.next_id)
}

pub fn shown_balance(account: &Account, transactions: &[Transaction]) -> Option<i64> {
    let balance = account.balance_minor?;
    if account.kind == "investment" {
        return Some(balance);
    }
    let mut rows: Vec<&Transaction> = transactions.iter().filter(|tx| tx.account_id == account.id || tx.from_account_id == account.id).collect();
    rows.sort_by_key(|tx| (tx.date.clone(), tx.created_ms));
    let mut balance = balance;
    for tx in rows {
        if !account.currency.is_empty() && !tx.currency.is_empty() && tx.currency != account.currency {
            continue;
        }
        if account.balance_updated_ms > 0 && tx.created_ms <= account.balance_updated_ms {
            continue;
        }
        if !account.balance_as_of.is_empty() && tx.date.as_str() < account.balance_as_of.as_str() {
            continue;
        }
        let role = if tx.account_id == account.id { Role::Primary } else { Role::Source };
        balance = step_balance(balance, &account.kind, role, &tx.direction, tx.amount_minor);
    }
    Some(balance)
}

fn step_balance(balance: i64, kind: &str, role: Role, direction: &str, amount: i64) -> i64 {
    let delta = match role {
        Role::Source => -amount,
        Role::Primary => match (kind, direction) {
            ("credit_card" | "loan", "expense") => amount,
            ("credit_card" | "loan", "payment") => -amount,
            ("credit_card" | "loan", _) => 0,
            ("investment", _) => 0,
            (_, "income") => amount,
            (_, "expense" | "payment") => -amount,
            _ => 0,
        },
    };
    balance.saturating_add(delta)
}

fn snapshot_of(store: &Store, locale: &str, today: NaiveDate) -> Snapshot {
    let period = format!("{:04}-{:02}", today.year(), today.month());
    let mut accounts: Vec<AccountView> = store
        .accounts
        .iter()
        .map(|account| AccountView { shown_balance_minor: shown_balance(account, &store.transactions), account: account.clone() })
        .collect();
    accounts.sort_by(|a, b| a.account.kind.cmp(&b.account.kind).then(a.account.name.to_ascii_lowercase().cmp(&b.account.name.to_ascii_lowercase())));
    let mut transactions = store.transactions.clone();
    transactions.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_ms.cmp(&a.created_ms)));
    let mut rules = store.rules.clone();
    rules.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    let totals = period_totals(&transactions, &period, "", "", "");
    let upcoming = upcoming_items(store, today);
    Snapshot { accounts, transactions, rules, totals, upcoming, period, default_currency: currency_for_locale(locale).into() }
}

fn period_totals(transactions: &[Transaction], period: &str, account_id: &str, direction: &str, class: &str) -> Vec<PeriodTotal> {
    let mut totals: Vec<PeriodTotal> = Vec::new();
    for tx in transactions {
        if !period.is_empty() && !tx.date.starts_with(period) {
            continue;
        }
        if !account_id.is_empty() && tx.account_id != account_id && tx.from_account_id != account_id {
            continue;
        }
        if !direction.is_empty() && tx.direction != direction {
            continue;
        }
        if !class.is_empty() && class != "once" && tx.recurrence != class {
            continue;
        }
        if class == "once" && tx.recurrence != "once" {
            continue;
        }
        let row = totals.iter_mut().find(|row| row.currency == tx.currency);
        let row = if let Some(row) = row {
            row
        } else {
            totals.push(PeriodTotal { currency: tx.currency.clone(), income_minor: 0, expense_minor: 0, payment_minor: 0 });
            totals.last_mut().unwrap()
        };
        match tx.direction.as_str() {
            "income" => row.income_minor = row.income_minor.saturating_add(tx.amount_minor),
            "expense" => row.expense_minor = row.expense_minor.saturating_add(tx.amount_minor),
            "payment" => row.payment_minor = row.payment_minor.saturating_add(tx.amount_minor),
            _ => {}
        }
    }
    totals.sort_by(|a, b| a.currency.cmp(&b.currency));
    totals
}

fn upcoming_items(store: &Store, today: NaiveDate) -> Vec<Upcoming> {
    let mut rows = Vec::new();
    for rule in &store.rules {
        let Some(date) = next_upcoming_date(&rule.next_date, &rule.cadence, today) else { continue };
        rows.push(Upcoming {
            id: rule.id.clone(),
            source: "rule".into(),
            title: rule.name.clone(),
            date: iso(date),
            amount_minor: rule.amount_minor,
            currency: rule.currency.clone(),
            direction: rule.direction.clone(),
            calendar_event_id: rule.calendar_event_id.clone(),
        });
    }
    rows.sort_by(|a, b| a.date.cmp(&b.date).then(a.title.cmp(&b.title)));
    rows.truncate(12);
    rows
}

fn speak(store: &Store, ask: &Ask, today: NaiveDate) -> String {
    let month = if ask.month.len() == 7 { ask.month.clone() } else { format!("{:04}-{:02}", today.year(), today.month()) };
    let kind = if !ask.kind.is_empty() {
        ask.kind.clone()
    } else if fold(&ask.query) == "cash" || fold(&ask.account) == "cash" {
        "cash".into()
    } else if fold(&ask.query).contains("card") {
        "credit_card".into()
    } else if fold(&ask.query).contains("loan") {
        "loan".into()
    } else if fold(&ask.query).contains("invest") {
        "investment".into()
    } else {
        String::new()
    };
    let view = if !ask.view.is_empty() {
        ask.view.clone()
    } else if !kind.is_empty() && ask.query.to_ascii_lowercase().contains("spend") {
        "summary".into()
    } else if !kind.is_empty() {
        "accounts".into()
    } else if ask.query.to_ascii_lowercase().contains("next") || ask.query.to_ascii_lowercase().contains("when") {
        "upcoming".into()
    } else {
        "summary".into()
    };
    if view == "upcoming" {
        return speak_upcoming(&upcoming_items(store, today));
    }
    if view == "accounts" || view == "investments" {
        let kind = if view == "investments" && kind.is_empty() { "investment".into() } else { kind };
        return speak_accounts(store, &kind, &ask.account);
    }
    let account_id = find_account(store, "", &ask.account).ok().flatten().map(|index| store.accounts[index].id.clone()).unwrap_or_default();
    let direction = normalize_direction(&ask.direction).unwrap_or_default();
    let class = if ask.recurrence == "once" { "" } else { ask.recurrence.as_str() };
    let totals = period_totals(&store.transactions, &month, &account_id, &direction, &class);
    let mut detail = speak_totals(&totals, &month);
    if !account_id.is_empty() || !kind.is_empty() {
        detail.push(' ');
        detail.push_str(&speak_accounts(store, &kind, &ask.account));
    }
    detail
}

fn speak_totals(totals: &[PeriodTotal], month: &str) -> String {
    if totals.is_empty() {
        return format!("No income or expenses are saved for {month}.");
    }
    totals
        .iter()
        .map(|row| {
            let spent = if row.expense_minor == 0 { "nothing".into() } else { format_minor(row.expense_minor, &row.currency) };
            let got = if row.income_minor == 0 { "nothing".into() } else { format_minor(row.income_minor, &row.currency) };
            let mut line = format!("Spent {spent} and received {got} in {month} in {}.", row.currency);
            if row.payment_minor > 0 {
                line.push_str(&format!(" Payments were {}.", format_minor(row.payment_minor, &row.currency)));
            }
            line
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn speak_accounts(store: &Store, kind: &str, account: &str) -> String {
    let indexes: Vec<usize> = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, row)| kind.is_empty() || row.kind == kind)
        .filter(|(_, row)| account.trim().is_empty() || fold(&row.name).contains(&fold(account)) || fold(account).contains(&fold(&row.name)))
        .map(|(index, _)| index)
        .collect();
    if indexes.is_empty() {
        return match kind {
            "cash" => "No cash account is saved.".into(),
            "credit_card" => "No credit card is saved.".into(),
            "loan" => "No loan is saved.".into(),
            "investment" => "No investment is saved.".into(),
            "bank" => "No bank account is saved.".into(),
            _ => "No matching account is saved.".into(),
        };
    }
    indexes
        .into_iter()
        .map(|index| {
            let account = &store.accounts[index];
            match shown_balance(account, &store.transactions) {
                None => format!("{} has no balance saved.", account.name),
                Some(minor) => match account.kind.as_str() {
                    "credit_card" | "loan" => format!("{} owes {}.", account.name, format_minor(minor, &account.currency)),
                    "investment" => format!("{} is worth {}.", account.name, format_minor(minor, &account.currency)),
                    _ => format!("{} is {}.", account.name, format_minor(minor, &account.currency)),
                },
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn speak_upcoming(rows: &[Upcoming]) -> String {
    if rows.is_empty() {
        return "Nothing is scheduled.".into();
    }
    let body = rows
        .iter()
        .take(3)
        .map(|row| match row.amount_minor {
            Some(amount) if amount > 0 && !row.currency.is_empty() => format!("{} on {} ({})", row.title, row.date, format_minor(amount, &row.currency)),
            _ => format!("{} on {}", row.title, row.date),
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("Next: {body}.")
}

fn balance_clause(store: &Store, account_id: &str) -> String {
    let Some(account) = store.accounts.iter().find(|account| account.id == account_id) else {
        return String::new();
    };
    match shown_balance(account, &store.transactions) {
        Some(minor) => format!(" Shown balance {}.", format_minor(minor, &account.currency)),
        None => " No balance is saved, so none was invented.".into(),
    }
}

fn is_link_date(date: &str, direction: &str, today: NaiveDate) -> bool {
    let Some(date) = parse_iso_date(date) else {
        return false;
    };
    date > today || (date == today && direction != "expense")
}

fn calendar_clause(cadence: &str, date: &str) -> String {
    if date.is_empty() {
        " No date was set, so Calendar was left alone.".into()
    } else if recurrence::is_repeating(cadence) {
        " That date repeats on this app's Calendar.".into()
    } else {
        " That date is on this app's Calendar.".into()
    }
}

fn account_name(store: &Store, id: &str) -> Option<String> {
    store.accounts.iter().find(|account| account.id == id).map(|account| account.name.clone())
}

struct Plan {
    title: String,
    when_ms: i64,
    recurrence: String,
    weekdays: Vec<u8>,
    location: String,
}

fn plan_event(title: &str, date: &str, cadence: &str, weekdays: &[u8], location: &str, now: chrono::DateTime<Local>) -> Option<Plan> {
    let date = parse_iso_date(date)?;
    let title = clamp(title, 80);
    if title.is_empty() {
        return None;
    }
    let cadence = normalize_cadence(cadence);
    let when = upcoming_start(date, &cadence, weekdays, now)?;
    let recurrence = if recurrence::is_repeating(&cadence) { cadence } else { String::new() };
    Some(Plan { title, when_ms: when.timestamp_millis(), recurrence, weekdays: weekdays.to_vec(), location: clamp(location, 80) })
}

fn upcoming_start(date: NaiveDate, cadence: &str, weekdays: &[u8], now: chrono::DateTime<Local>) -> Option<chrono::DateTime<Local>> {
    let initial = morning(date)?;
    if initial > now {
        return Some(initial);
    }
    if !recurrence::is_repeating(cadence) {
        if date == now.date_naive() {
            return Some(now + Duration::minutes(1));
        }
        return None;
    }
    let mut days = weekdays.to_vec();
    if days.is_empty() {
        days.push(date.weekday().number_from_monday() as u8);
    }
    let spec = recurrence::Spec {
        kind: cadence.into(),
        weekdays: days,
        hour: 9,
        minute: 0,
        month: date.month(),
        day: date.day(),
        year: Some(date.year()),
        lead_days: 0,
        until: None,
    };
    recurrence::next_after(&spec, now)
}

fn morning(date: NaiveDate) -> Option<chrono::DateTime<Local>> {
    let naive = date.and_hms_opt(9, 0, 0)?;
    naive.and_local_timezone(Local).earliest().or_else(|| naive.and_local_timezone(Local).latest())
}

fn link_calendar(app: &AppHandle, store: &mut Store, now: chrono::DateTime<Local>) {
    let agenda = app.state::<Agenda>();
    let rules = store.rules.clone();
    for rule in rules {
        let location = rule.amount_minor.map(|amount| format_minor(amount, &rule.currency)).unwrap_or_default();
        let owned = format!("local-fin-{}", rule.id);
        match plan_event(&rule.name, &rule.next_date, &rule.cadence, &rule.weekdays, &location, now) {
            Some(plan) => {
                let id = if rule.calendar_event_id.starts_with("local-fin-") { rule.calendar_event_id.clone() } else { owned };
                if let Ok(event) = agenda.upsert_local_event(app, plan_to_event(&plan, id)) {
                    if let Some(row) = store.rules.iter_mut().find(|row| row.id == rule.id) {
                        row.calendar_event_id = event.id;
                    }
                }
            }
            None => {
                release_owned(app, &rule.calendar_event_id, &owned);
                if let Some(row) = store.rules.iter_mut().find(|row| row.id == rule.id) {
                    row.calendar_event_id.clear();
                }
            }
        }
    }
    let covered: HashSet<String> = store.rules.iter().filter(|rule| !rule.calendar_event_id.is_empty()).map(|rule| rule.account_id.clone()).collect();
    let accounts = store.accounts.clone();
    for account in accounts {
        let owned = format!("local-fin-acct-{}", account.id);
        if covered.contains(&account.id) {
            if account.calendar_event_id == owned {
                release_owned(app, &owned, &owned);
            }
            let shared = store
                .rules
                .iter()
                .find(|rule| rule.from_account && rule.account_id == account.id && !rule.calendar_event_id.is_empty())
                .map(|rule| rule.calendar_event_id.clone())
                .unwrap_or_default();
            if let Some(row) = store.accounts.iter_mut().find(|row| row.id == account.id) {
                row.calendar_event_id = shared;
            }
            continue;
        }
        let (date, cadence, amount, title) = if account.kind == "investment" {
            (account.contribution_date.clone(), account.contribution_cadence.clone(), account.contribution_minor, format!("Invest in {}", account.name))
        } else {
            (account.due_date.clone(), account.cadence.clone(), account.payment_minor, account.name.clone())
        };
        let location = amount.map(|value| format_minor(value, &account.currency)).unwrap_or_default();
        match plan_event(&title, &date, &cadence, &[], &location, now) {
            Some(plan) => {
                if let Ok(event) = agenda.upsert_local_event(app, plan_to_event(&plan, owned)) {
                    if let Some(row) = store.accounts.iter_mut().find(|row| row.id == account.id) {
                        row.calendar_event_id = event.id;
                    }
                }
            }
            None => {
                release_owned(app, &account.calendar_event_id, &owned);
                if let Some(row) = store.accounts.iter_mut().find(|row| row.id == account.id) {
                    if row.calendar_event_id == owned {
                        row.calendar_event_id.clear();
                    }
                }
            }
        }
    }
}

fn plan_to_event(plan: &Plan, id: String) -> CalEvent {
    CalEvent {
        id,
        title: plan.title.clone(),
        start_ms: plan.when_ms,
        end_ms: plan.when_ms.saturating_add(30 * 60 * 1000),
        location: plan.location.clone(),
        all_day: false,
        source: "local".into(),
        recurrence: plan.recurrence.clone(),
        weekdays: plan.weekdays.clone(),
        until_ms: 0,
        alarm_id: 0,
        rrule: String::new(),
    }
}

fn release_owned(app: &AppHandle, id: &str, owned: &str) {
    if !id.is_empty() && id == owned {
        let _ = app.state::<Agenda>().delete_local_event(app, id);
    }
}

fn gc_events(app: &AppHandle, previous: &Store, store: &Store) {
    let keep: HashSet<String> = event_ids(store).into_iter().collect();
    for id in event_ids(previous) {
        if !keep.contains(&id) {
            let _ = app.state::<Agenda>().delete_local_event(app, &id);
        }
    }
}

fn event_ids(store: &Store) -> Vec<String> {
    store
        .rules
        .iter()
        .map(|rule| rule.calendar_event_id.clone())
        .chain(store.accounts.iter().map(|account| account.calendar_event_id.clone()))
        .filter(|id| id.starts_with("local-fin-"))
        .collect()
}

fn touch(app: &AppHandle) {
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::diagnostics::Hub>>() {
        hub.emit("voice://finance", json!({}));
    }
}

fn normalize_kind(value: &str) -> String {
    match fold(value).as_str() {
        "bank" | "checking" | "savings" | "current" => "bank".into(),
        "cash" | "wallet" => "cash".into(),
        "credit" | "credit card" | "creditcard" | "card" => "credit_card".into(),
        "loan" | "debt" | "mortgage" => "loan".into(),
        "investment" | "invest" | "brokerage" | "holding" => "investment".into(),
        "other" => "other".into(),
        _ => String::new(),
    }
}

fn infer_kind(name: &str, explicit: &str) -> String {
    let explicit = normalize_kind(explicit);
    if !explicit.is_empty() {
        return explicit;
    }
    let name = fold(name);
    if name == "cash" || name == "wallet" || name.contains("cash") {
        return "cash".into();
    }
    if name.contains("visa") || name.contains("mastercard") || name.contains("amex") || name.contains("card") {
        return "credit_card".into();
    }
    if name.contains("loan") || name.contains("mortgage") {
        return "loan".into();
    }
    "other".into()
}

fn normalize_direction(value: &str) -> Result<String, String> {
    match fold(value).as_str() {
        "income" | "in" | "earned" | "salary" | "payday" => Ok("income".into()),
        "expense" | "spent" | "spend" | "out" | "purchase" | "charge" => Ok("expense".into()),
        "payment" | "pay" | "transfer" | "contribution" | "invest" => Ok("payment".into()),
        "" => Ok(String::new()),
        _ => Err("Direction is income, expense, or payment.".into()),
    }
}

fn normalize_class(value: &str) -> String {
    match fold(value).as_str() {
        "recurring" | "repeat" | "repeating" | "subscription" => "recurring".into(),
        "irregular" | "non recurring" | "nonrecurring" | "variable" | "sometimes" => "irregular".into(),
        "once" | "one time" | "onetime" => "once".into(),
        _ => String::new(),
    }
}

fn classify(class: &str, cadence: &str) -> &'static str {
    let class = normalize_class(class);
    if class == "irregular" {
        "irregular"
    } else if class == "recurring" || recurrence::is_repeating(&normalize_cadence(cadence)) {
        "recurring"
    } else {
        "once"
    }
}

fn normalize_cadence(value: &str) -> String {
    let kind = recurrence::normalize_kind(value);
    if recurrence::is_repeating(&kind) { kind } else { String::new() }
}

pub fn format_minor(amount: i64, currency: &str) -> String {
    let exp = exponent(currency);
    let sign = if amount < 0 { "-" } else { "" };
    let amount = amount.abs();
    if exp == 0 {
        return format!("{sign}{amount} {currency}");
    }
    let scale = 10i64.pow(exp);
    format!("{sign}{}.{:0width$} {currency}", amount / scale, amount % scale, width = exp as usize)
}

fn exponent(currency: &str) -> u32 {
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" => 0,
        "BHD" | "KWD" | "OMR" => 3,
        _ => 2,
    }
}

fn resolve_money(amount: &str, currency_arg: &str, fallback_locale: &str) -> Result<(i64, String), String> {
    let (number, embedded) = peel_currency(amount);
    let currency = if !currency_arg.trim().is_empty() {
        canonical_currency(currency_arg)?
    } else if let Some(embedded) = embedded {
        embedded
    } else {
        currency_for_locale(fallback_locale).to_string()
    };
    let minor = parse_minor(&number, exponent(&currency))?;
    Ok((minor, currency))
}

fn canonical_or_default(currency: &str, fallback_locale: &str) -> Result<String, String> {
    if currency.trim().is_empty() {
        Ok(currency_for_locale(fallback_locale).into())
    } else {
        Ok(canonical_currency(currency)?)
    }
}

fn canonical_currency(value: &str) -> Result<String, String> {
    let token = fold(value).replace(' ', "");
    let code = match token.as_str() {
        "eur" | "euro" | "euros" | "€" => "EUR",
        "usd" | "dollar" | "dollars" | "bucks" | "$" | "us$" => "USD",
        "gbp" | "pound" | "pounds" | "£" => "GBP",
        "chf" | "franc" | "francs" => "CHF",
        "brl" | "real" | "reais" | "r$" => "BRL",
        "jpy" | "yen" | "¥" => "JPY",
        "cad" => "CAD",
        "aud" => "AUD",
        "nzd" => "NZD",
        "mxn" | "peso" | "pesos" => "MXN",
        "inr" | "rupee" | "rupees" => "INR",
        "pln" | "zloty" | "zlotys" => "PLN",
        "sek" | "krona" | "kronor" => "SEK",
        "nok" | "krone" | "kroner" => "NOK",
        "dkk" => "DKK",
        "cny" | "yuan" | "rmb" => "CNY",
        other if other.len() == 3 && other.chars().all(|c| c.is_ascii_alphabetic()) => return Ok(other.to_ascii_uppercase()),
        _ => return Err("Say the currency, such as euros or USD.".into()),
    };
    Ok(code.into())
}

fn peel_currency(raw: &str) -> (String, Option<String>) {
    let mut words = Vec::new();
    let mut found = None;
    for word in raw.split_whitespace() {
        let stripped = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '€' && c != '$' && c != '£' && c != '¥');
        if found.is_none() && let Ok(code) = canonical_currency(stripped) && !stripped.chars().all(|c| c.is_ascii_digit()) {
            found = Some(code);
            continue;
        }
        words.push(word);
    }
    let joined = if words.is_empty() { raw.trim().to_string() } else { words.join(" ") };
    (joined, found)
}

fn parse_minor(raw: &str, exp: u32) -> Result<i64, String> {
    let raw = raw.trim().replace(' ', "");
    if raw.is_empty() {
        return Err("The amount has to be more than zero.".into());
    }
    let negative = raw.starts_with('-');
    let raw = raw.trim_start_matches(['+', '-']);
    let normalized = normalize_decimal(raw)?;
    let (whole, frac) = match normalized.split_once('.') {
        Some((whole, frac)) => (whole, frac),
        None => (normalized.as_str(), ""),
    };
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return Err("The amount has to be more than zero.".into());
    }
    let whole: i128 = whole.parse().map_err(|_| "That amount is too large.".to_string())?;
    let exp_usize = exp as usize;
    let mut digits: Vec<u8> = frac.bytes().map(|byte| byte - b'0').collect();
    let mut round_up = false;
    if digits.len() > exp_usize {
        round_up = digits.get(exp_usize).copied().unwrap_or(0) >= 5;
        digits.truncate(exp_usize);
    }
    while digits.len() < exp_usize {
        digits.push(0);
    }
    let mut frac_val: i128 = 0;
    for digit in &digits {
        frac_val = frac_val * 10 + i128::from(*digit);
    }
    let scale = 10i128.pow(exp);
    let mut minor = whole * scale + frac_val;
    if round_up {
        minor += 1;
    }
    if negative {
        minor = -minor;
    }
    if minor <= 0 {
        return Err("The amount has to be more than zero.".into());
    }
    if minor > 100_000_000_000_000 {
        return Err("That amount is too large.".into());
    }
    i64::try_from(minor).map_err(|_| "That amount is too large.".to_string())
}

fn normalize_decimal(raw: &str) -> Result<String, String> {
    let last_comma = raw.rfind(',');
    let last_dot = raw.rfind('.');
    let decimal_is_comma = match (last_comma, last_dot) {
        (Some(comma), Some(dot)) => comma > dot,
        (Some(_), None) => !is_thousands(raw, ','),
        _ => false,
    };
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if cleaned.is_empty() {
        return Err("The amount has to be more than zero.".into());
    }
    if decimal_is_comma {
        Ok(cleaned.replace('.', "").replace(',', "."))
    } else {
        Ok(cleaned.replace(',', ""))
    }
}

fn is_thousands(raw: &str, sep: char) -> bool {
    let parts: Vec<&str> = raw.split(sep).filter(|part| !part.is_empty()).collect();
    parts.len() >= 2
        && parts[0].len() <= 3
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1..].iter().all(|part| part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
}

fn require_date(value: &str, cadence: &str, today: NaiveDate) -> Result<String, String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    let parsed = interpret_date(value, cadence, today);
    if parsed.is_empty() {
        Err("Use a date like 2026-12-01, tomorrow, or the day of the month.".into())
    } else {
        Ok(parsed)
    }
}

fn interpret_date(value: &str, cadence: &str, today: NaiveDate) -> String {
    let lower = fold(value);
    if lower == "today" {
        return iso(today);
    }
    if lower == "tomorrow" {
        return iso(today + Duration::days(1));
    }
    if let Some(date) = parse_iso_date(value.trim()) {
        return iso(date);
    }
    if let Some(day) = day_number(value) {
        let _ = cadence;
        return iso(next_month_day(day, today));
    }
    String::new()
}

fn parse_iso_date(value: &str) -> Option<NaiveDate> {
    let mut parts = value.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month, day)
}

fn day_number(value: &str) -> Option<u32> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let day = digits.parse::<u32>().ok()?;
    (1..=31).contains(&day).then_some(day)
}

fn next_month_day(day: u32, today: NaiveDate) -> NaiveDate {
    let this_month = clamped_day(today.year(), today.month(), day);
    if this_month >= today { this_month } else { add_months(this_month, 1).unwrap_or(this_month) }
}

fn next_upcoming_date(anchor: &str, cadence: &str, today: NaiveDate) -> Option<NaiveDate> {
    let anchor = parse_iso_date(anchor)?;
    if anchor >= today {
        return Some(anchor);
    }
    let cadence = normalize_cadence(cadence);
    if !recurrence::is_repeating(&cadence) {
        return None;
    }
    let mut cursor = anchor;
    for _ in 0..240 {
        cursor = step_date(cursor, &cadence)?;
        if cursor >= today {
            return Some(cursor);
        }
    }
    None
}

fn step_date(date: NaiveDate, cadence: &str) -> Option<NaiveDate> {
    match cadence {
        "daily" => Some(date + Duration::days(1)),
        "weekly" => Some(date + Duration::days(7)),
        "monthly" => add_months(date, 1),
        "yearly" => add_months(date, 12),
        _ => None,
    }
}

fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    let index = date.year() * 12 + date.month() as i32 - 1 + months;
    let year = index.div_euclid(12);
    let month = (index.rem_euclid(12) + 1) as u32;
    Some(clamped_day(year, month, date.day()))
}

fn clamped_day(year: i32, month: u32, day: u32) -> NaiveDate {
    for day in (1..=day.min(31)).rev() {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return date;
        }
    }
    NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap())
}

fn iso(date: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn amount_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string().trim_matches('"').to_string(),
    }
}

fn first_str<'a>(args: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|key| args[*key].as_str()).unwrap_or("").trim()
}

fn clean_text(value: &str, max: usize) -> Result<String, String> {
    let text = clamp(value, max);
    if sensitive(&text) {
        return Err(SECRET.into());
    }
    Ok(text)
}

fn clamp(value: &str, max: usize) -> String {
    value.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}

fn sensitive(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }
    let mut run = 0;
    let mut longest = 0;
    let mut digits = 0;
    let mut letters = 0;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
            run += 1;
            longest = longest.max(run);
        } else if ch.is_whitespace() || ch == '-' {
            // A card number may be grouped. The run continues across those separators.
        } else {
            run = 0;
            if ch.is_ascii_alphabetic() {
                letters += 1;
            }
        }
    }
    if longest >= 8 {
        return true;
    }
    if digits >= 8 && letters == 0 {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    (lower.contains("cvv") || lower.contains("cvc") || lower.contains("pin")) && (3..=8).contains(&digits) && letters < 8
}

fn fold(value: &str) -> String {
    value
        .trim()
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .filter(|ch| ch.is_alphanumeric() || ch.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

enum NameMatch {
    None,
    One(usize),
    Many(Vec<String>),
}

fn match_named(names: &[&str], query: &str) -> NameMatch {
    let query = fold(query);
    if query.is_empty() {
        return NameMatch::None;
    }
    if let Some(index) = names.iter().position(|name| fold(name) == query) {
        return NameMatch::One(index);
    }
    let hits: Vec<usize> = names.iter().enumerate().filter(|(_, name)| fold(name).contains(&query) || query.contains(&fold(name))).map(|(index, _)| index).collect();
    match hits.as_slice() {
        [] => NameMatch::None,
        [index] => NameMatch::One(*index),
        _ => NameMatch::Many(hits.iter().map(|index| names[*index].to_string()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn clock(today: NaiveDate) -> Clock {
        Clock { ms: 5_000, today, currency: "de-DE".into() }
    }

    fn cash(balance: Option<i64>, updated: i64) -> Account {
        let mut account = empty_account("fa1".into());
        account.name = "Cash".into();
        account.kind = "cash".into();
        account.balance_minor = balance;
        account.currency = "EUR".into();
        account.balance_as_of = "2026-11-10".into();
        account.balance_updated_ms = updated;
        account
    }

    fn tx(direction: &str, amount: i64, account: &str, from: &str, date: &str, created: i64) -> Transaction {
        Transaction {
            id: format!("ft{created}"),
            amount_minor: amount,
            currency: "EUR".into(),
            direction: direction.into(),
            category: String::new(),
            account_id: account.into(),
            from_account_id: from.into(),
            date: date.into(),
            notes: String::new(),
            recurrence: "once".into(),
            rule_id: String::new(),
            created_ms: created,
            updated_ms: created,
        }
    }

    #[test]
    fn expense_does_not_invent_a_balance() {
        let account = cash(None, 0);
        let spent = tx("expense", 1_200, "fa1", "", "2026-11-10", 9_000);
        assert_eq!(shown_balance(&account, &[spent]), None);
    }

    #[test]
    fn stated_balances_move_only_after_the_statement() {
        let account = cash(Some(4_000), 1_000);
        let later = tx("expense", 1_200, "fa1", "", "2026-11-10", 2_000);
        let earlier = tx("expense", 500, "fa1", "", "2026-11-09", 500);
        assert_eq!(shown_balance(&account, &[later, earlier]), Some(2_800));
        let card = Account { kind: "credit_card".into(), name: "Visa travel".into(), ..cash(Some(40_000), 1_000) };
        let charge = tx("expense", 4_000, "fa1", "", "2026-11-10", 2_000);
        let payment = tx("payment", 10_000, "fa1", "bank", "2026-11-10", 3_000);
        assert_eq!(shown_balance(&card, &[charge.clone(), payment.clone()]), Some(34_000));
        let loan = Account { kind: "loan".into(), name: "Car loan".into(), ..cash(Some(28_000), 1_000) };
        assert_eq!(shown_balance(&loan, &[payment]), Some(18_000));
        let fund = Account { kind: "investment".into(), name: "VT".into(), ..cash(Some(50_000), 1_000) };
        assert_eq!(shown_balance(&fund, &[charge]), Some(50_000));
        let bank = Account { kind: "bank".into(), id: "bank".into(), name: "Checking".into(), ..cash(Some(20_000), 1_000) };
        assert_eq!(shown_balance(&bank, &[tx("payment", 10_000, "fa1", "bank", "2026-11-10", 3_000)]), Some(10_000));
    }

    #[test]
    fn other_currency_and_past_dates_do_not_rewrite_a_stated_balance() {
        let account = cash(Some(4_000), 1_000);
        let mut other = tx("expense", 1_200, "fa1", "", "2026-11-10", 2_000);
        other.currency = "USD".into();
        let past = tx("expense", 1_200, "fa1", "", "2026-11-01", 3_000);
        assert_eq!(shown_balance(&account, &[other, past]), Some(4_000));
    }

    #[test]
    fn locale_picks_the_currency_when_the_user_does_not() {
        assert_eq!(currency_for_locale("de-DE"), "EUR");
        assert_eq!(currency_for_locale("en-US"), "USD");
        assert_eq!(currency_for_locale("en-GB"), "GBP");
        assert_eq!(currency_for_locale("es-MX"), "MXN");
        assert_eq!(currency_for_locale("pt-BR"), "BRL");
        assert_eq!(currency_for_locale("ca-ES-valencia"), "EUR");
        assert_eq!(currency_for_locale(""), "USD");
        assert_eq!(resolve_money("12", "", "de-DE").unwrap(), (1_200, "EUR".into()));
        assert_eq!(resolve_money("12 euros", "", "en-US").unwrap(), (1_200, "EUR".into()));
        assert_eq!(resolve_money("15.99", "USD", "de-DE").unwrap(), (1_599, "USD".into()));
        assert_eq!(resolve_money("12,50", "", "de").unwrap(), (1_250, "EUR".into()));
        assert_eq!(resolve_money("1.234,56", "", "de").unwrap(), (123_456, "EUR".into()));
        assert_eq!(resolve_money("1,234.56", "", "en-US").unwrap(), (123_456, "USD".into()));
    }

    #[test]
    fn card_numbers_and_pins_are_refused() {
        assert!(clean_text("4111 1111 1111 1111", 40).is_err());
        assert!(clean_text("123456789", 40).is_err());
        assert!(clean_text("PIN 1234", 40).is_err());
        assert_eq!(clean_text("Visa travel", 40).unwrap(), "Visa travel");
        assert_eq!(clean_text("N26", 40).unwrap(), "N26");
    }

    #[test]
    fn a_date_lands_on_this_apps_calendar_plan() {
        let now = morning(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).unwrap() + Duration::hours(6);
        assert!(plan_event("Netflix", "", "monthly", &[], "15.99 EUR", now).is_none());
        let rent = plan_event("Rent", "2026-12-01", "monthly", &[], "1200.00 EUR", now).unwrap();
        assert_eq!(rent.recurrence, "monthly");
        assert_eq!(rent.title, "Rent");
        let when = chrono::DateTime::from_timestamp_millis(rent.when_ms).unwrap().with_timezone(&Local);
        assert_eq!(when.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!((when.hour(), when.minute()), (9, 0));
        let past = plan_event("Rent", "2026-11-01", "monthly", &[], "", now).unwrap();
        let next = chrono::DateTime::from_timestamp_millis(past.when_ms).unwrap().with_timezone(&Local);
        assert_eq!(next.date_naive(), NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert!(plan_event("Lunch", "2026-11-01", "", &[], "", now).is_none());
    }

    #[test]
    fn logging_and_questions_do_not_invent_figures() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        let detail = log_from_tool(
            &mut store,
            &json!({"direction": "expense", "amount": "12 euros", "category": "lunch", "account": "cash", "accountKind": "cash"}),
            &clock,
        )
        .unwrap();
        assert!(detail.contains("12.00 EUR"));
        assert!(detail.contains("No balance is saved"));
        assert!(store.accounts[0].balance_minor.is_none());
        assert_eq!(store.transactions[0].recurrence, "once");
        let spoken = speak(&store, &Ask { view: "summary".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new() }, clock.today);
        assert!(spoken.contains("Spent 12.00 EUR"));
        assert!(spoken.contains("received nothing"));
        let cash = speak(&store, &Ask { view: String::new(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: "cash".into() }, clock.today);
        assert!(cash.contains("no balance saved"));
        rule_from_tool(&mut store, &json!({"name": "Rent", "date": "1", "cadence": "monthly"}), &clock).unwrap();
        let rent = store.rules.iter().find(|rule| rule.name == "Rent").unwrap();
        assert!(rent.amount_minor.is_none());
        assert_eq!(rent.next_date, "2026-12-01");
        let upcoming = speak(&store, &Ask { view: "upcoming".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new() }, clock.today);
        assert!(upcoming.contains("Rent on 2026-12-01"));
        assert!(!upcoming.contains("0.00"));
    }

    #[test]
    fn irregular_stays_irregular_and_tools_are_listed() {
        assert_eq!(classify("irregular", "monthly"), "irregular");
        assert_eq!(classify("once", ""), "once");
        assert_eq!(classify("", "monthly"), "recurring");
        let names: Vec<_> = tool_definitions().as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"log_money".into()));
        assert!(names.contains(&"save_recurring".into()));
        assert!(names.contains(&"save_account".into()));
        assert!(names.contains(&"list_finance".into()));
        assert!(names.contains(&"delete_finance".into()));
        let hint = crate::actions::lookup_hint();
        assert!(hint.contains("list_finance"));
        assert!(hint.contains("Do not invent"));
    }

    #[test]
    fn delete_waits_for_a_clear_yes() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        account_from_tool(&mut store, &json!({"name": "Cash", "kind": "cash", "balance": "40", "currency": "EUR"}), &clock).unwrap();
        let ask = describe_delete(&store, &json!({"kind": "account", "name": "Cash"})).unwrap();
        assert!(ask.contains("Say yes"));
        assert_eq!(store.accounts.len(), 1);
        apply_delete(&mut store, &json!({"kind": "account", "name": "Cash", "confirmed": true})).unwrap();
        assert!(store.accounts.is_empty());
    }
}

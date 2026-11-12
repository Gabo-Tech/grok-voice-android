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

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::{Datelike, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{
    actions::{Agenda, CalEvent},
    recurrence,
};

const SECRET: &str = "Use a short nickname. Card numbers, account numbers, and PINs are not saved.";

thread_local! {
    static SPOKEN_LOCALE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

struct LocaleGuard(String);

impl Drop for LocaleGuard {
    fn drop(&mut self) {
        let prev = std::mem::take(&mut self.0);
        SPOKEN_LOCALE.with(|cell| *cell.borrow_mut() = prev);
    }
}

fn push_locale(locale: &str) -> LocaleGuard {
    let next = if locale.trim().is_empty() { "en".to_string() } else { locale.to_string() };
    let prev = SPOKEN_LOCALE.with(|cell| cell.replace(next));
    LocaleGuard(prev)
}

fn spoken(template: &str, pairs: &[(&str, &str)]) -> String {
    let locale = SPOKEN_LOCALE.with(|cell| {
        let current = cell.borrow();
        if current.is_empty() { "en".to_string() } else { current.clone() }
    });
    crate::i18n::fill(&locale, template, pairs)
}

fn and_list<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for (index, name) in names.into_iter().enumerate() {
        if index > 0 {
            out.push_str(&spoken(" and ", &[]));
        }
        out.push_str(name);
    }
    out
}

fn cadence_spoken(cadence: &str) -> String {
    let key = match cadence {
        "daily" | "weekly" | "monthly" | "yearly" => cadence,
        "every_n_days" => "every few days",
        _ => return cadence.to_string(),
    };
    spoken(key, &[])
}

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
    /// User color, `#RRGGBB`. Empty until a palette color is assigned.
    #[serde(default)]
    pub color: String,
    /// `stock`, `fund`, `crypto`, or `custom` for an investment.
    #[serde(default)]
    pub investment_type: String,
    /// Decimal count when worth is units times a stated price. Empty for a stated worth.
    #[serde(default)]
    pub units: String,
    #[serde(default)]
    pub price_minor: Option<i64>,
    /// Days before the due date to remind. Missing means two days, and again on the day.
    #[serde(default)]
    pub lead_days: Option<u32>,
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
    /// Occurrence date this entry settled. Empty on a plain transaction.
    #[serde(default)]
    pub occurrence_date: String,
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
    /// Days before the date to remind. Missing means two days, and again on the day.
    #[serde(default)]
    pub lead_days: Option<u32>,
    /// In-app Time reminder that fires ahead of the date. 0 means none.
    #[serde(default)]
    pub reminder_id: i32,
    /// `one_off`, `recurring`, `subscription`, or `investment_payout`.
    #[serde(default)]
    pub purpose: String,
    /// `active`, `paused`, `stopped`, or `completed`. Empty means active.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub start_date: String,
    /// `none`, `after_count`, or `on_date`.
    #[serde(default)]
    pub end_kind: String,
    #[serde(default)]
    pub end_count: u32,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub interval_days: u32,
    /// `fixed` amount in `currency`, or `quote` value settled in `payout_currency`.
    #[serde(default)]
    pub amount_mode: String,
    #[serde(default)]
    pub payout_currency: String,
    #[serde(default)]
    pub skipped_dates: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Store {
    accounts: Vec<Account>,
    transactions: Vec<Transaction>,
    rules: Vec<Rule>,
    next_id: u64,
    locale_hint: String,
    /// Set by the user. Empty means seed from the phone locale until they choose one.
    #[serde(default)]
    default_currency: String,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default)]
    fx: crate::finance_fx::FxBook,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub accounts: Vec<Account>,
    pub transactions: Vec<Transaction>,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub default_currency: String,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub fx: crate::finance_fx::FxBook,
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
    pub days_until: i64,
    #[serde(default)]
    pub lead_days: u32,
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
    pub categories: Vec<Category>,
    pub fx: FxStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FxStatus {
    pub as_of: String,
    pub fetched_ms: i64,
    pub stale: bool,
    pub per_eur: Vec<crate::finance_fx::FxLeg>,
    pub manual: Vec<crate::finance_fx::ManualRate>,
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
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub investment_type: String,
    #[serde(default)]
    pub units: String,
    #[serde(default)]
    pub unit_price: String,
    /// `stated` or `units`. Empty leaves the saved mode alone on a partial update.
    #[serde(default)]
    pub value_mode: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub new_name: String,
    #[serde(default)]
    pub lead_days: Option<u32>,
    /// The form sets this when the user picked a kind. Voice sets it only on an explicit change.
    #[serde(default)]
    #[allow(dead_code)]
    pub change_kind: bool,
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
    #[serde(default)]
    pub lead_days: Option<u32>,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_kind: String,
    #[serde(default)]
    pub end_count: u32,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub interval_days: u32,
    #[serde(default)]
    pub amount_mode: String,
    #[serde(default)]
    pub payout_currency: String,
    #[serde(default)]
    pub account_kind: String,
}

struct Clock {
    ms: i64,
    today: NaiveDate,
    /// Locale tag kept so a seeded default can be explained in tests.
    #[allow(dead_code)]
    currency: String,
    /// ISO code used when the user does not name one and the account has none.
    default_currency: String,
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
        let mut inner = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Store>(&bytes).ok()).unwrap_or_default();
        let dirty = hydrate_store(&mut inner);
        let finance = Self { path, inner: Mutex::new(inner) };
        if dirty {
            let store = finance.lock().clone();
            let _ = Self::persist(&finance.path, &store);
        }
        finance
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
        Bundle {
            accounts: store.accounts.clone(),
            transactions: store.transactions.clone(),
            rules: store.rules.clone(),
            default_currency: store.default_currency.clone(),
            categories: store.categories.clone(),
            fx: store.fx.clone(),
        }
    }

    pub fn replace(&self, app: &AppHandle, bundle: Bundle) -> Result<(), String> {
        let mut store = self.lock().clone();
        store.accounts = bundle.accounts;
        store.transactions = bundle.transactions;
        store.rules = bundle.rules;
        store.default_currency = bundle.default_currency;
        store.categories = bundle.categories;
        store.fx = bundle.fx;
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

    pub fn set_default_currency(&self, app: &AppHandle, code: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        store.default_currency = canonical_currency(code)?;
        self.commit(app, store)
    }

    pub fn rename_category_name(&self, app: &AppHandle, from: &str, to: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        rename_category(&mut store, from, to)?;
        self.commit(app, store)
    }

    pub fn mark_paid_on(&self, app: &AppHandle, id: &str, date: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        mark_paid(&mut store, &json!({ "id": id, "name": "", "date": date }), &clock)?;
        self.commit(app, store)
    }

    pub fn export_pdf(&self, app: &AppHandle, request: &PdfRequest) -> Result<ExportedPdf, String> {
        let store = self.lock().clone();
        let today = Local::now().date_naive();
        let locale = if request.locale.trim().is_empty() { currency_locale(app, &store) } else { request.locale.clone() };
        let pages = pdf_pages(&store, request, today, &locale);
        let bytes = crate::finance_pdf::render(&pages);
        let dir = app.path().app_cache_dir().map_err(|err| err.to_string())?.join("finance");
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let file_name = format!("grok-finance-{:04}-{:02}-{:02}.pdf", today.year(), today.month(), today.day());
        let path = dir.join(&file_name);
        crate::files::write_private(&path, &bytes).map_err(|err| err.to_string())?;
        let shared = path.to_str().is_some_and(|path| app.state::<crate::native::NativeBridge>().share_file(path, "application/pdf", "Finance summary").is_ok());
        Ok(ExportedPdf {
            file_name,
            data_b64: if shared { String::new() } else { B64.encode(&bytes) },
            shared,
            pages: pages.len(),
        })
    }

    pub fn refresh_rates(&self, app: &AppHandle) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        store.fx = crate::finance_fx::fetch_book(&store.fx, Local::now().timestamp_millis())?;
        self.commit(app, store)
    }

    pub fn set_manual_rate(&self, app: &AppHandle, base: &str, quote: &str, rate: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        if rate.trim().is_empty() {
            store.fx.manual.retain(|row| !(row.base.eq_ignore_ascii_case(base) && row.quote.eq_ignore_ascii_case(quote)));
        } else {
            let value: f64 = rate.trim().parse().map_err(|_| "The rate has to be a number above zero.".to_string())?;
            let as_of = store.fx.as_of.clone();
            crate::finance_fx::upsert_manual(&mut store.fx, base, quote, value, &as_of)?;
        }
        self.commit(app, store)
    }

    pub fn schedule_action(&self, app: &AppHandle, id: &str, action: &str, date: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        let index = store.rules.iter().position(|rule| rule.id == id).ok_or("No recurring item with that id.")?;
        match action {
            "pause" => store.rules[index].status = "paused".into(),
            "stop" => store.rules[index].status = "stopped".into(),
            "resume" => store.rules[index].status = "active".into(),
            "skip" => {
                let occurrence = if date.is_empty() { store.rules[index].next_date.clone() } else { date.to_string() };
                if !occurrence.is_empty() && !store.rules[index].skipped_dates.iter().any(|row| row == &occurrence) {
                    store.rules[index].skipped_dates.push(occurrence);
                }
                let _ = roll_schedule(&mut store, index, None, &clock);
            }
            _ => return Err("Action is pause, stop, resume, or skip.".into()),
        }
        if action != "skip" {
            store.rules[index].updated_ms = clock.ms;
        }
        self.commit(app, store)
    }

    fn commit(&self, app: &AppHandle, mut store: Store) -> Result<Snapshot, String> {
        let _ = hydrate_store(&mut store);
        let previous = self.lock().clone();
        link_calendar(app, &mut store, Local::now());
        let snap = snapshot_of(&store, &currency_locale(app, &store), Local::now().date_naive());
        Self::persist(&self.path, &store)?;
        gc_events(app, &previous, &store);
        gc_reminders(app, &previous, &store);
        *self.lock() = store;
        touch(app);
        Ok(snap)
    }
}

fn clock_for(app: &AppHandle, store: &Store) -> Clock {
    let now = Local::now();
    let locale = currency_locale(app, store);
    Clock { ms: now.timestamp_millis(), today: now.date_naive(), default_currency: effective_default(store, &locale), currency: locale }
}

fn effective_default(store: &Store, locale: &str) -> String {
    if !store.default_currency.trim().is_empty() {
        return canonical_currency(&store.default_currency).unwrap_or_else(|_| currency_for_locale(locale).to_string());
    }
    currency_for_locale(locale).to_string()
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
            "Log an expense, income, or a payment toward a card, loan, or other debt on this phone. amount is the number they said. currency is whatever they named; omit it to use that account's saved currency, or the Finance default. Do not assume USD. direction is expense, income, or payment. account is a nickname. accountKind is bank, cash, credit_card, loan, other_debt, investment, or other when creating that nickname. fromAccount is the bank or cash a payment came from. date is YYYY-MM-DD, tomorrow, or a day of the month. recurrence is once, recurring, or irregular. cadence is daily, weekly, monthly, or yearly. leadDays is how many days before a future payment to remind them, in this app only. Do not invent a balance, due date, or account number. A future payment, due, or income date is also saved on this app's Calendar and Time list.",
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
                    "leadDays": { "type": "integer" },
                    "notes": { "type": "string" }
                },
                "required": ["direction", "amount"]
            })
        ),
        fn_tool(
            "save_recurring",
            "Save a repeating bill, payday, subscription, card payment, or automated investment on this phone. name is the nickname. amount may be omitted when they only named a date. cadence is daily, weekly, monthly, yearly, or every N days with intervalDays. occurrences is how many times a fixed run lasts, such as 9. openEnded true means it does not end. date is YYYY-MM-DD, tomorrow, or a day such as 1. direction is expense, income, or payment. leadDays is how many days before to remind them in this app. Default is 2, and the day itself is reminded too. 0 means only on the day. Do not invent an amount or a date. The date is saved on this app's Calendar and Time list, not the phone Clock. Pass phrase instead when they describe the whole plan in one sentence.",
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
                    "leadDays": { "type": "integer" },
                    "notes": { "type": "string" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "save_schedule",
            "Program income or an expense on this phone: a fixed run such as a course for 9 months, open-ended rent, a subscription, a one-off, planned future income, or an investment payout. Pass phrase as the user's sentence when you can, for example a course for 9 months at 270 euros, rent for as long as they live there, or 700 dollars in Nitroeggs that pays 14 dollars every 10 days in XMR. phrase programs the plan and does not mark it paid. Structured fields: name, amount, currency, direction expense or income, purpose one_off, recurring, subscription, or investment_payout, cadence, intervalDays, occurrences, openEnded, date, payoutCurrency, amountMode fixed or quote, account, accountKind. action pause, stop, or resume with name. Do not invent a price for the investment or an exchange rate. A future item stays planned until mark_paid.",
            json!({
                "type": "object",
                "properties": {
                    "phrase": { "type": "string" },
                    "name": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "direction": { "type": "string" },
                    "purpose": { "type": "string" },
                    "cadence": { "type": "string" },
                    "intervalDays": { "type": "integer" },
                    "occurrences": { "type": "integer" },
                    "openEnded": { "type": "boolean" },
                    "date": { "type": "string" },
                    "payoutCurrency": { "type": "string" },
                    "amountMode": { "type": "string" },
                    "account": { "type": "string" },
                    "accountKind": { "type": "string" },
                    "action": { "type": "string" },
                    "category": { "type": "string" },
                    "leadDays": { "type": "integer" }
                }
            })
        ),
        fn_tool(
            "save_account",
            "Save or update a bank, cash, card, loan, other debt, or investment the user described. Match the exact nickname. Never merge different kinds that only share part of a name, such as ZKB and ZKB credit card. kind is bank, cash, credit_card, loan, other_debt, investment, or other. changeKind true only when they explicitly change the kind of that same nickname. balance or worth is only a figure they stated. units and unitPrice are a stated count times a stated price, such as 2 at 50 CHF. A later price or worth replaces only what they stated. currency when they name one; otherwise that account's currency, or the Finance default. defaultCurrency sets that default and can be sent alone, for example CHF. color is a hex color such as #E25B45. investmentType is stock, fund, crypto, or custom. newName renames that nickname. leadDays is how many days before a due date to remind them in this app. Do not invent a balance, a price, or an exchange rate. Never store a card number, account number, or PIN.",
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
                    "color": { "type": "string" },
                    "investmentType": { "type": "string" },
                    "units": { "type": "string" },
                    "unitPrice": { "type": "string" },
                    "newName": { "type": "string" },
                    "changeKind": { "type": "boolean" },
                    "defaultCurrency": { "type": "string" },
                    "leadDays": { "type": "integer" },
                    "notes": { "type": "string" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "list_finance",
            "Read Finance saved on this phone, without guessing amounts or exchange rates. view is summary, accounts, activity, upcoming, or investments. month is YYYY-MM. kind is bank, cash, credit_card, loan, other_debt, or investment. account is a nickname. Speak the detail, including upcoming payments with their dates. Do not invent a balance that is not in the result.",
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
        ),
        fn_tool(
            "mark_paid",
            "Mark a saved bill, rent, subscription, or card payment as paid. This logs the payment or income they stated, updates the on-device balance, and moves a repeating reminder to the next date. name is the nickname. amount only if they stated one and none is saved. Do not invent an amount. confirmed is not required. Nothing is uploaded.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "date": { "type": "string" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "export_finance_pdf",
            "Write a Finance PDF on this phone and open the share sheet. Nothing is uploaded and no exchange rate is invented. Call first with confirmed false, say the period, then call with confirmed true. period is this_month, last_month, last_6, year, all, or YYYY-MM. account, category, currency, and direction filter the summary the same way as the screen. Charts use nicknames and the user's colors.",
            json!({
                "type": "object",
                "properties": {
                    "period": { "type": "string" },
                    "account": { "type": "string" },
                    "category": { "type": "string" },
                    "currency": { "type": "string" },
                    "direction": { "type": "string" },
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["confirmed"]
            })
        )
    ])
}

fn fn_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({ "type": "function", "name": name, "description": description, "parameters": parameters })
}

pub fn try_tool(app: &AppHandle, name: &str, args: &Value) -> Result<Option<Value>, String> {
    if !matches!(
        name,
        "log_money" | "save_recurring" | "save_schedule" | "save_account" | "list_finance" | "delete_finance" | "mark_paid" | "export_finance_pdf"
    ) {
        return Ok(None);
    }
    let locale = {
        let finance = app.state::<Finance>();
        let store = finance.lock().clone();
        currency_locale(app, &store)
    };
    let _guard = push_locale(&locale);
    try_tool_inner(app, name, args).map_err(|err| crate::i18n::line(&locale, &err))
}

fn try_tool_inner(app: &AppHandle, name: &str, args: &Value) -> Result<Option<Value>, String> {
    if !matches!(
        name,
        "log_money" | "save_recurring" | "save_schedule" | "save_account" | "list_finance" | "delete_finance" | "mark_paid" | "export_finance_pdf"
    ) {
        return Ok(None);
    }
    let finance = app.state::<Finance>();
    if name == "export_finance_pdf" {
        return export_from_tool(app, &finance, args);
    }
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
            "defaultCurrency": snap.default_currency,
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
        "log_money" => {
            if let Some(phrase) = args["phrase"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
                apply_phrase(&mut store, phrase, &clock)?
            } else {
                log_from_tool(&mut store, args, &clock)?
            }
        }
        "save_recurring" | "save_schedule" => {
            if let Some(phrase) = args["phrase"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
                apply_phrase(&mut store, phrase, &clock)?
            } else if matches!(fold(args["action"].as_str().unwrap_or("")).as_str(), "pause" | "stop" | "resume") {
                phrase_action(&mut store, &crate::finance_schedule::parse_phrase(&format!("{} {}", args["action"].as_str().unwrap_or(""), args["name"].as_str().unwrap_or("")), clock.today, &clock.default_currency)?, &clock)?
            } else {
                rule_from_tool(&mut store, args, &clock, false)?
            }
        }
        "save_account" => account_from_tool(&mut store, args, &clock)?,
        "mark_paid" => mark_paid(&mut store, args, &clock)?,
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
    let account_label = args["account"].as_str().unwrap_or("");
    let account_kind = args["accountKind"].as_str().unwrap_or("");
    let known = locate_account(store, "", account_label, account_kind)?;
    let saved_currency = known.map(|found| store.accounts[found.index].currency.clone()).unwrap_or_default();
    let (amount_minor, currency) = money_for(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(""), &saved_currency, &clock.default_currency)?;
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
    let pay_name = if category.is_empty() {
        account_name(store, &account_id).unwrap_or_else(|| direction.clone())
    } else {
        category.clone()
    };
    let settled = matches!(direction.as_str(), "payment" | "income") && class != "recurring";
    let (rule_id, rolled) = if let Some(index) = repeating_rule_index(store, &pay_name, &account_id, &direction) {
        if settled {
            let id = store.rules[index].id.clone();
            let note = advance_rule_at(store, index, clock);
            (Some(id), Some(note))
        } else if link {
            let rule_cadence = if class == "irregular" { String::new() } else { cadence.clone() };
            (Some(upsert_named_rule(store, "", &pay_name, Some(amount_minor), &currency, &direction, &rule_cadence, &provided, &category, &account_id, &notes, &[], false, lead_arg(args), clock)?), None)
        } else {
            (None, None)
        }
    } else if link {
        let rule_cadence = if class == "irregular" { String::new() } else { cadence.clone() };
        (Some(upsert_named_rule(store, "", &pay_name, Some(amount_minor), &currency, &direction, &rule_cadence, &provided, &category, &account_id, &notes, &[], false, lead_arg(args), clock)?), None)
    } else {
        (None, None)
    };
    let linked = rule_id.is_some();
    let future = parse_iso_date(&date).is_some_and(|value| value > clock.today);
    if future && linked {
        let label = if category.is_empty() { direction } else { category };
        let amount = format_minor(amount_minor, &currency);
        return Ok(spoken(
            "Programmed {label} of {amount} on {date}. It stays planned until you mark it.",
            &[("label", &label), ("amount", &amount), ("date", &date)],
        ));
    }
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
        occurrence_date: String::new(),
    });
    let label = if category.is_empty() { direction.clone() } else { category.clone() };
    let amount = format_minor(amount_minor, &currency);
    let mut detail = spoken("Saved {label} of {amount} on {date}.", &[("label", &label), ("amount", &amount), ("date", &date)]);
    if let Some(name) = account_name(store, &account_id) {
        detail.push_str(&spoken(" Account: {name}.", &[("name", &name)]));
        detail.push_str(&balance_clause(store, &account_id));
    }
    if linked {
        detail.push_str(&calendar_clause(&cadence, &provided));
    }
    remember_category(store, &category);
    if let Some(rolled) = rolled {
        detail.push(' ');
        detail.push_str(&rolled);
    }
    Ok(detail)
}

fn rule_from_tool(store: &mut Store, args: &Value, clock: &Clock, programmed: bool) -> Result<String, String> {
    let name = clean_text(first_str(args, &["name", "title"]), 80)?;
    if name.is_empty() {
        return Err("A recurring item needs a name.".into());
    }
    let cadence = normalize_cadence(args["cadence"].as_str().unwrap_or(""));
    if args["cadence"].as_str().map(str::trim).filter(|value| !value.is_empty()).is_some() && cadence.is_empty() {
        return Err("Cadence is daily, weekly, monthly, yearly, or every N days.".into());
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
        let (minor, currency) = resolve_money(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(""), &clock.default_currency)?;
        (Some(minor), currency)
    };
    let direction = if args["direction"].as_str().map(str::trim).filter(|s| !s.is_empty()).is_some() {
        normalize_direction(args["direction"].as_str().unwrap_or(""))?
    } else if fold(&name).contains("salary") || fold(&name).contains("paycheck") {
        "income".into()
    } else {
        "expense".into()
    };
    let account_id = resolve_account(store, "", args["account"].as_str().unwrap_or(""), args["accountKind"].as_str().unwrap_or(""), if currency.is_empty() { &clock.default_currency } else { &currency }, clock, true)?;
    let category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    let notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    let id = args["id"].as_str().unwrap_or("").trim().to_string();
    let saved = upsert_named_rule(store, &id, &name, amount, &currency, &direction, &cadence, &date_raw, &category, &account_id, &notes, &[], false, lead_arg(args), clock)?;
    remember_category(store, &category);
    let money = match amount {
        Some(minor) => format_minor(minor, if currency.is_empty() { &clock.default_currency } else { &currency }),
        None => spoken("no amount", &[]),
    };
    let template = if programmed { "Programmed {name}, {money}." } else { "Saved {name}, {money}." };
    let mut detail = spoken(template, &[("name", &name), ("money", &money)]);
    if recurrence::is_repeating(&cadence) {
        let word = cadence_spoken(&cadence);
        detail.push_str(&spoken(" Repeats {cadence}.", &[("cadence", &word)]));
    }
    detail.push_str(&calendar_clause(&cadence, &date_raw));
    let _ = saved;
    if let Some(rule) = store.rules.iter_mut().find(|rule| rule.id == saved) {
        paint_schedule_args(rule, args);
        let _ = hydrate_rule(rule);
    }
    Ok(detail)
}

fn paint_schedule_args(rule: &mut Rule, args: &Value) {
    if let Some(purpose) = args["purpose"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        rule.purpose = purpose.to_string();
    }
    if let Some(count) = args.get("occurrences").and_then(|value| value.as_u64()).or_else(|| args.get("endCount").and_then(|value| value.as_u64())) {
        if count > 0 {
            rule.end_kind = "after_count".into();
            rule.end_count = count as u32;
        }
    }
    if args["openEnded"].as_bool().unwrap_or(false) {
        rule.end_kind = "none".into();
        rule.end_count = 0;
    }
    if let Some(end) = args["endDate"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        rule.end_date = end.to_string();
        if rule.end_kind.is_empty() || rule.end_kind == "none" {
            rule.end_kind = "on_date".into();
        }
    }
    if let Some(days) = args.get("intervalDays").and_then(|value| value.as_u64()).filter(|value| *value > 0) {
        rule.interval_days = days as u32;
        if rule.cadence.is_empty() {
            rule.cadence = "every_n_days".into();
        }
    }
    if let Some(mode) = args["amountMode"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        rule.amount_mode = mode.to_string();
    }
    if let Some(payout) = args["payoutCurrency"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        rule.payout_currency = canonical_currency(payout).unwrap_or_else(|_| payout.to_ascii_uppercase());
    }
    if let Some(status) = args["status"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        rule.status = status.to_string();
    }
}

fn apply_phrase(store: &mut Store, phrase: &str, clock: &Clock) -> Result<String, String> {
    let parsed = crate::finance_schedule::parse_phrase(phrase, clock.today, &clock.default_currency)?;
    match parsed.kind.as_str() {
        "transaction" => log_from_tool(
            store,
            &json!({
                "direction": parsed.direction,
                "amount": parsed.amount,
                "currency": parsed.currency,
                "category": parsed.category,
                "date": parsed.start_date
            }),
            clock,
        ),
        "action" => phrase_action(store, &parsed, clock),
        "payout" => phrase_payout(store, &parsed, clock),
        _ => phrase_schedule(store, &parsed, clock),
    }
}

fn phrase_schedule(store: &mut Store, parsed: &crate::finance_schedule::Parsed, clock: &Clock) -> Result<String, String> {
    let args = json!({
        "name": parsed.name,
        "amount": parsed.amount,
        "currency": parsed.currency,
        "direction": parsed.direction,
        "cadence": parsed.cadence,
        "date": parsed.start_date,
        "category": parsed.category,
        "account": parsed.account_name,
        "accountKind": parsed.account_kind,
        "purpose": parsed.purpose,
        "occurrences": parsed.end_count,
        "intervalDays": parsed.interval_days,
        "amountMode": parsed.amount_mode,
        "payoutCurrency": parsed.payout_currency,
        "openEnded": parsed.end_kind == "none"
    });
    let detail = rule_from_tool(store, &args, clock, true)?;
    if let Some(rule) = store.rules.iter_mut().rev().find(|rule| fold(&rule.name) == fold(&parsed.name)) {
        rule.start_date = parsed.start_date.clone();
        rule.next_date = parsed.start_date.clone();
        if parsed.end_kind == "none" {
            rule.end_kind = "none".into();
            rule.end_count = 0;
        }
    }
    Ok(detail)
}

fn phrase_payout(store: &mut Store, parsed: &crate::finance_schedule::Parsed, clock: &Clock) -> Result<String, String> {
    if let Some(found) = locate_account(store, "", &parsed.account_name, "investment")? {
        if store.accounts[found.index].kind != "investment" {
            let found_name = store.accounts[found.index].name.clone();
            let found_kind = store.accounts[found.index].kind.clone();
            return Err(spoken("{name} is already a {kind}. Say if that should change.", &[("name", &found_name), ("kind", &found_kind)]));
        }
    }
    account_from_tool(
        store,
        &json!({
            "name": parsed.account_name,
            "kind": "investment",
            "balance": parsed.balance,
            "currency": parsed.balance_currency,
            "investmentType": "custom"
        }),
        clock,
    )?;
    let mut detail = phrase_schedule(store, parsed, clock)?;
    detail.push_str(&spoken(" Principal stays the amount you entered.", &[]));
    Ok(detail)
}

fn phrase_action(store: &mut Store, parsed: &crate::finance_schedule::Parsed, clock: &Clock) -> Result<String, String> {
    let index = rule_by_name(store, &parsed.name)?;
    let status = match parsed.action.as_str() {
        "pause" => "paused",
        "resume" => "active",
        _ => "stopped",
    };
    store.rules[index].status = status.into();
    store.rules[index].updated_ms = clock.ms;
    let name = store.rules[index].name.clone();
    let word = match status {
        "paused" => spoken("paused", &[]),
        "stopped" => spoken("stopped", &[]),
        _ => spoken("active", &[]),
    };
    Ok(spoken("{name} is {status}.", &[("name", &name), ("status", &word)]))
}

fn rule_by_name(store: &Store, name: &str) -> Result<usize, String> {
    let query = fold(name);
    let exact: Vec<usize> = store.rules.iter().enumerate().filter(|(_, rule)| fold(&rule.name) == query).map(|(index, _)| index).collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        let names = and_list(exact.iter().map(|index| store.rules[*index].name.as_str()));
        return Err(spoken("Which payment? I found {names}.", &[("names", &names)]));
    }
    let hits: Vec<usize> = store
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            let folded = fold(&rule.name);
            !folded.is_empty() && (folded.contains(&query) || query.contains(&folded))
        })
        .map(|(index, _)| index)
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err("Which payment?".into()),
        _ => {
            let names = and_list(hits.iter().map(|index| store.rules[*index].name.as_str()));
            Err(spoken("Which payment? I found {names}.", &[("names", &names)]))
        }
    }
}

fn account_from_tool(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    if let Some(code) = args.get("defaultCurrency").and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()) {
        store.default_currency = canonical_currency(code)?;
        if args["name"].as_str().unwrap_or("").trim().is_empty() {
            return Ok(spoken("Default currency is {code} when you do not name one.", &[("code", &store.default_currency)]));
        }
    }
    let name = clean_text(args["name"].as_str().unwrap_or(""), 80)?;
    if name.is_empty() {
        return Err("An account needs a nickname.".into());
    }
    let explicit = normalize_kind(args["kind"].as_str().unwrap_or(""));
    let located = locate_account(store, args["id"].as_str().unwrap_or(""), &name, &explicit)?;
    let (id, allow_kind, kept_kind) = match located {
        Some(found) => {
            let existing = &store.accounts[found.index];
            let allow = found.exact && args["changeKind"].as_bool().unwrap_or(false);
            (existing.id.clone(), allow, existing.kind.clone())
        }
        None => (String::new(), true, String::new()),
    };
    let mut display = name.clone();
    if let Some(renamed) = args.get("newName").and_then(|value| value.as_str()) {
        let renamed = clean_text(renamed, 80)?;
        if !renamed.is_empty() {
            display = renamed;
        }
    }
    let kind = if allow_kind { explicit } else { kept_kind };
    let mut draft = AccountDraft {
        id,
        name: display,
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
        color: args["color"].as_str().unwrap_or("").trim().to_string(),
        investment_type: args["investmentType"].as_str().unwrap_or("").trim().to_string(),
        units: amount_text(args.get("units")),
        unit_price: amount_text(args.get("unitPrice")),
        value_mode: String::new(),
        new_name: String::new(),
        lead_days: lead_arg(args),
        change_kind: allow_kind,
    };
    if args.get("units").is_some() && args.get("unitPrice").is_some() {
        draft.value_mode = "units".into();
    } else if args.get("balance").is_some() || args.get("worth").is_some() {
        draft.value_mode = "stated".into();
    }
    let partial = id_present(store, &draft.id);
    let balance_value = if args.get("balance").is_some() { args.get("balance") } else { args.get("worth") };
    if balance_value.is_some() {
        draft.balance = amount_text(balance_value);
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
    let present_balance = balance_value.is_some();
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
            allow_kind_change: allow_kind || !partial,
            color: args.get("color").is_some() || !partial,
            investment_type: args.get("investmentType").is_some() || !partial,
            units: args.get("units").is_some() || !partial,
            price: args.get("unitPrice").is_some() || !partial,
            lead: args.get("leadDays").is_some() || !partial,
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
    allow_kind_change: bool,
    color: bool,
    investment_type: bool,
    units: bool,
    price: bool,
    lead: bool,
}

fn apply_account_form(store: &mut Store, draft: &AccountDraft, clock: &Clock) -> Result<String, String> {
    apply_account_fields(
        store,
        draft,
        clock,
        FieldMask {
            balance: true,
            due: true,
            payment: true,
            contribution: true,
            contribution_date: true,
            replace_text: true,
            allow_kind_change: true,
            color: true,
            investment_type: true,
            units: true,
            price: true,
            lead: true,
        },
    )
}

fn apply_account_fields(store: &mut Store, draft: &AccountDraft, clock: &Clock, mask: FieldMask) -> Result<String, String> {
    let name = clean_text(&draft.name, 80)?;
    if name.is_empty() {
        return Err("An account needs a nickname.".into());
    }
    let creating = draft.id.trim().is_empty();
    let mut account = if creating {
        let id = alloc(store, "fa");
        empty_account(id)
    } else {
        store.accounts.iter().find(|account| account.id == draft.id.trim()).cloned().ok_or("No account with that id.")?
    };
    let kind = if !creating && !mask.allow_kind_change {
        account.kind.clone()
    } else {
        infer_kind(&name, &draft.kind)
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
    if !draft.currency.trim().is_empty() {
        account.currency = canonical_currency(&draft.currency)?;
    }
    if mask.color {
        account.color = if draft.color.trim().is_empty() { account.color.clone() } else { normalize_color(&draft.color)? };
    }
    if account.color.is_empty() {
        account.color = next_color(store);
    }
    if account.kind == "investment" && (mask.investment_type || account.investment_type.is_empty()) {
        let chosen = if draft.investment_type.trim().is_empty() { account.investment_type.clone() } else { normalize_investment_type(&draft.investment_type) };
        account.investment_type = if chosen.is_empty() { infer_investment_type(&account.name, &account.ticker) } else { chosen };
    }
    if mask.lead {
        account.lead_days = draft.lead_days.map(|days| days.min(30));
    }
    let units_mode = draft.value_mode == "units" || (mask.units && mask.price && !draft.units.trim().is_empty() && !draft.unit_price.trim().is_empty());
    if units_mode {
        if mask.units {
            account.units = normalize_units(&draft.units)?;
        }
        if mask.price {
            account.price_minor = if draft.unit_price.trim().is_empty() {
                None
            } else {
                Some(resolve_money(&draft.unit_price, &account.currency, &clock.default_currency)?.0)
            };
        }
        if let (false, Some(price)) = (account.units.is_empty(), account.price_minor) {
            let worth = units_times_price(&account.units, price)?;
            account.balance_minor = Some(worth);
            account.balance_updated_ms = clock.ms;
            account.balance_as_of = iso(clock.today);
        }
    } else if mask.balance {
        if draft.value_mode == "stated" || mask.replace_text {
            account.units.clear();
            account.price_minor = None;
        }
        if draft.balance.trim().is_empty() {
            account.balance_minor = None;
            account.balance_as_of.clear();
            account.balance_updated_ms = 0;
        } else {
            let (minor, currency) = money_for(&draft.balance, &draft.currency, &account.currency, &clock.default_currency)?;
            if account.balance_minor != Some(minor) || account.currency != currency {
                account.balance_minor = Some(minor);
                account.currency = currency;
                account.balance_updated_ms = clock.ms;
                account.balance_as_of = iso(clock.today);
            }
        }
    } else if mask.price || mask.units {
        if mask.units && !draft.units.trim().is_empty() {
            account.units = normalize_units(&draft.units)?;
        }
        if mask.price && !draft.unit_price.trim().is_empty() {
            account.price_minor = Some(resolve_money(&draft.unit_price, &account.currency, &clock.default_currency)?.0);
        }
        if !account.units.is_empty() && account.price_minor.is_some() {
            let worth = units_times_price(&account.units, account.price_minor.unwrap_or(0))?;
            account.balance_minor = Some(worth);
            account.balance_updated_ms = clock.ms;
            account.balance_as_of = iso(clock.today);
        }
    }
    if account.currency.is_empty() {
        account.currency = clock.default_currency.clone();
    }
    if mask.due {
        account.due_date = require_date(&draft.due_date, &draft.cadence, clock.today)?;
    }
    if !draft.cadence.trim().is_empty() || mask.due {
        let cadence = normalize_cadence(&draft.cadence);
        if !draft.cadence.trim().is_empty() && cadence.is_empty() {
            return Err("Cadence is daily, weekly, monthly, yearly, or every N days.".into());
        }
        if mask.due || !draft.cadence.trim().is_empty() {
            account.cadence = cadence;
        }
    }
    if mask.payment {
        account.payment_minor = if draft.payment.trim().is_empty() {
            None
        } else {
            Some(resolve_money(&draft.payment, &account.currency, &clock.default_currency)?.0)
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
            Some(resolve_money(&draft.contribution, &account.currency, &clock.default_currency)?.0)
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
    let mut parts = vec![spoken("Saved {name}.", &[("name", &account.name)])];
    match account.balance_minor {
        Some(minor) => {
            let amount = format_minor(minor, &account.currency);
            parts.push(spoken("Stated balance {amount}.", &[("amount", &amount)]));
        }
        None => parts.push(spoken("No balance was stated.", &[])),
    }
    let dated = if account.kind == "investment" { account.contribution_date.as_str() } else { account.due_date.as_str() };
    if dated.is_empty() {
        parts.push(spoken("No due date was set, so Calendar was left alone.", &[]));
    } else {
        parts.push(spoken("Date {dated} is on this app's Calendar.", &[("dated", dated)]));
    }
    parts.join(" ")
}

fn sync_generated_rule(store: &mut Store, account: &Account, clock: &Clock) {
    let (amount, cadence, date, direction) = if account.kind == "investment" {
        (account.contribution_minor, account.contribution_cadence.clone(), account.contribution_date.clone(), "payment".into())
    } else if is_debt(&account.kind) {
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
        rule.next_date = date.clone();
        rule.category = account.kind.clone();
        rule.updated_ms = clock.ms;
        if account.lead_days.is_some() {
            rule.lead_days = account.lead_days;
        } else if rule.lead_days.is_none() && !date.is_empty() {
            rule.lead_days = Some(2);
        }
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
        next_date: date.clone(),
        category: account.kind.clone(),
        account_id: account.id.clone(),
        notes: String::new(),
        calendar_event_id: String::new(),
        updated_ms: clock.ms,
        from_account: true,
        lead_days: account.lead_days.or(if date.is_empty() { None } else { Some(2) }),
        reminder_id: 0,
        purpose: "recurring".into(),
        status: "active".into(),
        start_date: date.clone(),
        end_kind: "none".into(),
        end_count: 0,
        end_date: String::new(),
        interval_days: 0,
        amount_mode: "fixed".into(),
        payout_currency: String::new(),
        skipped_dates: Vec::new(),
    });
}

fn apply_transaction_form(store: &mut Store, draft: &TransactionDraft, clock: &Clock) -> Result<(), String> {
    let direction = normalize_direction(&draft.direction)?;
    let known = locate_account(store, &draft.account_id, &draft.account_name, "")?;
    let saved_currency = known.map(|found| store.accounts[found.index].currency.clone()).unwrap_or_default();
    let (amount_minor, currency) = money_for(&draft.amount, &draft.currency, &saved_currency, &clock.default_currency)?;
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
        upsert_named_rule(store, "", &name, Some(amount_minor), &currency, &direction, &rule_cadence, &provided, &category, &account_id, &notes, &[], false, None, clock)?
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
        occurrence_date: String::new(),
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
    remember_category(store, &draft.category);
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
        (None, canonical_or_default(&draft.currency, &clock.default_currency)?)
    } else {
        let (minor, currency) = resolve_money(&draft.amount, &draft.currency, &clock.default_currency)?;
        (Some(minor), currency)
    };
    let direction = if draft.direction.trim().is_empty() { "expense".into() } else { normalize_direction(&draft.direction)? };
    let account_id = resolve_account(store, &draft.account_id, &draft.account_name, &draft.account_kind, &currency, clock, true)?;
    let category = clean_text(&draft.category, 80)?;
    let notes = clean_text(&draft.notes, 500)?;
    let saved = upsert_named_rule(
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
        draft.lead_days,
        clock,
    )?;
    if let Some(rule) = store.rules.iter_mut().find(|rule| rule.id == saved) {
        if !draft.purpose.is_empty() {
            rule.purpose = draft.purpose.clone();
        }
        if !draft.status.is_empty() {
            rule.status = draft.status.clone();
        }
        if !draft.start_date.is_empty() {
            rule.start_date = draft.start_date.clone();
            if rule.next_date.is_empty() {
                rule.next_date = draft.start_date.clone();
            }
        }
        if !draft.end_kind.is_empty() {
            rule.end_kind = draft.end_kind.clone();
        }
        if draft.end_count > 0 {
            rule.end_count = draft.end_count;
            if rule.end_kind.is_empty() {
                rule.end_kind = "after_count".into();
            }
        }
        if !draft.end_date.is_empty() {
            rule.end_date = draft.end_date.clone();
            if rule.end_kind.is_empty() {
                rule.end_kind = "on_date".into();
            }
        }
        if draft.interval_days > 0 {
            rule.interval_days = draft.interval_days;
            if rule.cadence.is_empty() {
                rule.cadence = "every_n_days".into();
            }
        }
        if !draft.amount_mode.is_empty() {
            rule.amount_mode = draft.amount_mode.clone();
        }
        if !draft.payout_currency.trim().is_empty() {
            rule.payout_currency = canonical_currency(&draft.payout_currency).unwrap_or_else(|_| draft.payout_currency.trim().to_ascii_uppercase());
        }
        let _ = hydrate_rule(rule);
    }
    remember_category(store, &category);
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
    lead: Option<u32>,
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
        if let Some(days) = lead {
            rule.lead_days = Some(days.min(30));
        } else if rule.lead_days.is_none() && !date.is_empty() {
            rule.lead_days = Some(2);
        }
        return Ok(rule.id.clone());
    }
    let id = alloc(store, "fr");
    let lead_days = lead.or(if date.is_empty() { None } else { Some(2) }).map(|days| days.min(30));
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
        lead_days,
        reminder_id: 0,
        purpose: String::new(),
        status: "active".into(),
        start_date: date.into(),
        end_kind: String::new(),
        end_count: 0,
        end_date: String::new(),
        interval_days: 0,
        amount_mode: "fixed".into(),
        payout_currency: String::new(),
        skipped_dates: Vec::new(),
    });
    let _ = hydrate_rule(store.rules.last_mut().unwrap());
    Ok(id)
}

fn hydrate_rule(rule: &mut Rule) -> bool {
    let mut dirty = false;
    if rule.status.is_empty() {
        rule.status = "active".into();
        dirty = true;
    }
    if rule.start_date.is_empty() && !rule.next_date.is_empty() {
        rule.start_date = rule.next_date.clone();
        dirty = true;
    }
    let repeating = schedule_repeats(&rule.cadence, rule.interval_days);
    if rule.purpose.is_empty() {
        rule.purpose = if repeating {
            "recurring".into()
        } else if !rule.next_date.is_empty() || !rule.start_date.is_empty() {
            "one_off".into()
        } else {
            "recurring".into()
        };
        dirty = true;
    }
    if rule.end_kind.is_empty() {
        if rule.purpose == "one_off" || !repeating {
            rule.end_kind = "after_count".into();
            if rule.end_count == 0 {
                rule.end_count = 1;
            }
        } else {
            rule.end_kind = "none".into();
        }
        dirty = true;
    }
    if rule.amount_mode.is_empty() {
        rule.amount_mode = "fixed".into();
        dirty = true;
    }
    dirty
}

fn hydrate_store(store: &mut Store) -> bool {
    let mut dirty = false;
    for rule in &mut store.rules {
        dirty |= hydrate_rule(rule);
    }
    dirty
}

fn schedule_repeats(cadence: &str, interval: u32) -> bool {
    cadence == "every_n_days" && interval > 0 || recurrence::is_repeating(cadence)
}

fn fx_status(store: &Store, _today: NaiveDate) -> FxStatus {
    FxStatus {
        as_of: store.fx.as_of.clone(),
        fetched_ms: store.fx.fetched_ms,
        stale: crate::finance_fx::stale(&store.fx, Local::now().timestamp_millis()),
        per_eur: store.fx.per_eur.clone(),
        manual: store.fx.manual.clone(),
    }
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
            Ok(spoken("Deleted the account {label}. Transactions stay in Activity.", &[("label", &label)]))
        }
        "transaction" | "entry" => {
            let before = store.transactions.len();
            store.transactions.retain(|tx| tx.id != id);
            if store.transactions.len() == before {
                return Err("No transaction with that id.".into());
            }
            Ok(spoken("Deleted that transaction.", &[]))
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
            Ok(spoken("Deleted that recurring item and its Calendar date.", &[]))
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
    Ok(spoken("Delete {label}? Say yes and I will remove it.", &[("label", &label)]))
}

#[derive(Debug)]
struct Found {
    index: usize,
    exact: bool,
}

fn find_account(store: &Store, id: &str, name: &str) -> Result<Option<usize>, String> {
    Ok(locate_account(store, id, name, "")?.map(|found| found.index))
}

/// Exact folded names win. A substring match is kept only when it is unique and
/// the kinds are the same, or the incoming kind was not specified.
fn locate_account(store: &Store, id: &str, name: &str, kind: &str) -> Result<Option<Found>, String> {
    if !id.trim().is_empty() {
        let index = store.accounts.iter().position(|account| account.id == id.trim()).ok_or("No account with that id.")?;
        return Ok(Some(Found { index, exact: true }));
    }
    let query = fold(name);
    if query.is_empty() {
        return Ok(None);
    }
    let exact: Vec<usize> = store.accounts.iter().enumerate().filter(|(_, account)| fold(&account.name) == query).map(|(index, _)| index).collect();
    if exact.len() == 1 {
        return Ok(Some(Found { index: exact[0], exact: true }));
    }
    if exact.len() > 1 {
        return Err(which_account(store, &exact));
    }
    let incoming = normalize_kind(kind);
    let hits: Vec<usize> = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| {
            let folded = fold(&account.name);
            if folded.is_empty() || folded == query {
                return false;
            }
            let overlaps = folded.contains(&query) || query.contains(&folded);
            overlaps && (incoming.is_empty() || incoming == account.kind)
        })
        .map(|(index, _)| index)
        .collect();
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(Found { index: hits[0], exact: false })),
        _ => Err(which_account(store, &hits)),
    }
}

fn which_account(store: &Store, indexes: &[usize]) -> String {
    let names: Vec<&str> = indexes.iter().map(|index| store.accounts[*index].name.as_str()).collect();
    let found = and_list(names);
    spoken("Which account? I found {names}.", &[("names", &found)])
}

fn resolve_account(store: &mut Store, id: &str, name: &str, kind: &str, currency: &str, clock: &Clock, create: bool) -> Result<String, String> {
    if id.trim().is_empty() && name.trim().is_empty() {
        return Ok(String::new());
    }
    if let Some(found) = locate_account(store, id, name, kind)? {
        return Ok(store.accounts[found.index].id.clone());
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
    account.currency = if currency.is_empty() { clock.default_currency.clone() } else { currency.to_string() };
    account.color = next_color(store);
    account.updated_ms = clock.ms;
    if account.kind == "investment" {
        account.investment_type = infer_investment_type(&account.name, "");
    }
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
        color: String::new(),
        investment_type: String::new(),
        units: String::new(),
        price_minor: None,
        lead_days: None,
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
    if account.kind == "investment" {
        if !account.units.is_empty() {
            if let Some(price) = account.price_minor {
                return units_times_price(&account.units, price).ok();
            }
        }
        return account.balance_minor;
    }
    let balance = account.balance_minor?;
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
            ("credit_card" | "loan" | "other_debt", "expense") => amount,
            ("credit_card" | "loan" | "other_debt", "payment") => -amount,
            ("credit_card" | "loan" | "other_debt", _) => 0,
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
    Snapshot {
        accounts,
        transactions,
        rules,
        totals,
        upcoming,
        period,
        default_currency: effective_default(store, locale),
        categories: store.categories.clone(),
        fx: fx_status(store, today),
    }
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
        if matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
            continue;
        }
        let Some(date) = parse_iso_date(&rule.next_date) else { continue };
        rows.push(Upcoming {
            id: rule.id.clone(),
            source: "rule".into(),
            title: rule.name.clone(),
            date: iso(date),
            amount_minor: rule.amount_minor,
            currency: rule.currency.clone(),
            direction: rule.direction.clone(),
            calendar_event_id: rule.calendar_event_id.clone(),
            days_until: date.signed_duration_since(today).num_days(),
            lead_days: rule.lead_days.unwrap_or(2),
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
    let account_id = match locate_account(store, "", &ask.account, &kind) {
        Ok(Some(found)) => store.accounts[found.index].id.clone(),
        Ok(None) => String::new(),
        Err(message) => return message,
    };
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
        return spoken("No income or expenses are saved for {month}.", &[("month", month)]);
    }
    totals
        .iter()
        .map(|row| {
            let spent = if row.expense_minor == 0 { spoken("nothing", &[]) } else { format_minor(row.expense_minor, &row.currency) };
            let got = if row.income_minor == 0 { spoken("nothing", &[]) } else { format_minor(row.income_minor, &row.currency) };
            let mut line = spoken(
                "Spent {spent} and received {got} in {month} in {code}.",
                &[("spent", &spent), ("got", &got), ("month", month), ("code", &row.currency)],
            );
            if row.payment_minor > 0 {
                let amount = format_minor(row.payment_minor, &row.currency);
                line.push_str(&spoken(" Payments were {amount}.", &[("amount", &amount)]));
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
            "cash" => spoken("No cash account is saved.", &[]),
            "credit_card" => spoken("No credit card is saved.", &[]),
            "loan" => spoken("No loan is saved.", &[]),
            "other_debt" => spoken("No other debt is saved.", &[]),
            "investment" => spoken("No investment is saved.", &[]),
            "bank" => spoken("No bank account is saved.", &[]),
            _ => spoken("No matching account is saved.", &[]),
        };
    }
    indexes
        .into_iter()
        .map(|index| {
            let account = &store.accounts[index];
            match shown_balance(account, &store.transactions) {
                None => spoken("{name} has no balance saved.", &[("name", &account.name)]),
                Some(minor) => {
                    let amount = format_minor(minor, &account.currency);
                    match account.kind.as_str() {
                        "credit_card" | "loan" | "other_debt" => spoken("{name} owes {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        "investment" => spoken("{name} is worth {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        _ => spoken("{name} is {amount}.", &[("name", &account.name), ("amount", &amount)]),
                    }
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn speak_upcoming(rows: &[Upcoming]) -> String {
    if rows.is_empty() {
        return spoken("Nothing is scheduled.", &[]);
    }
    let body = rows
        .iter()
        .take(3)
        .map(|row| {
            let when = relative_days(row.days_until);
            match row.amount_minor {
                Some(amount) if amount > 0 && !row.currency.is_empty() => {
                    let money = format_minor(amount, &row.currency);
                    spoken(
                        "{title} on {date} ({amount}, {when})",
                        &[("title", &row.title), ("date", &row.date), ("amount", &money), ("when", &when)],
                    )
                }
                _ => spoken("{title} on {date} ({when})", &[("title", &row.title), ("date", &row.date), ("when", &when)]),
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    spoken("Next: {body}.", &[("body", &body)])
}

fn balance_clause(store: &Store, account_id: &str) -> String {
    let Some(account) = store.accounts.iter().find(|account| account.id == account_id) else {
        return String::new();
    };
    match shown_balance(account, &store.transactions) {
        Some(minor) => {
            let amount = format_minor(minor, &account.currency);
            spoken(" Shown balance {amount}.", &[("amount", &amount)])
        }
        None => spoken(" No balance is saved, so none was invented.", &[]),
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
        spoken(" No date was set, so Calendar was left alone.", &[])
    } else if recurrence::is_repeating(cadence) {
        spoken(" That date repeats on this app's Calendar.", &[])
    } else {
        spoken(" That date is on this app's Calendar.", &[])
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
        if matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
            let owned = format!("local-fin-{}", rule.id);
            release_owned(app, &rule.calendar_event_id, &owned);
            if let Some(row) = store.rules.iter_mut().find(|row| row.id == rule.id) {
                row.calendar_event_id.clear();
            }
            continue;
        }
        let location = rule.amount_minor.map(|amount| format_minor(amount, &rule.currency)).unwrap_or_default();
        let owned = format!("local-fin-{}", rule.id);
        let title = pay_title(&rule);
        match plan_event(&title, &rule.next_date, &rule.cadence, &rule.weekdays, &location, now) {
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
    let rule_ids: Vec<String> = store.rules.iter().map(|rule| rule.id.clone()).collect();
    for id in rule_ids {
        sync_pay_reminder(app, store, &id, now);
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
        "loan" | "mortgage" => "loan".into(),
        "other debt" | "otherdebt" | "other_debt" => "other_debt".into(),
        "debt" | "owe" | "owed" | "owing" => "other_debt".into(),
        "investment" | "invest" | "brokerage" | "holding" => "investment".into(),
        "other" => "other".into(),
        _ => String::new(),
    }
}

fn infer_kind(name: &str, explicit: &str) -> String {
    let explicit = normalize_kind(explicit);
    let folded = fold(name);
    let card = folded.contains("visa") || folded.contains("mastercard") || folded.contains("amex") || folded.contains("card");
    if card && (explicit.is_empty() || explicit == "other_debt" || explicit == "loan" || explicit == "other") {
        return "credit_card".into();
    }
    if !explicit.is_empty() {
        return explicit;
    }
    if folded == "cash" || folded == "wallet" || folded.contains("cash") {
        return "cash".into();
    }
    if folded.contains("loan") || folded.contains("mortgage") {
        return "loan".into();
    }
    if folded.contains("debt") || folded.contains("owe") {
        return "other_debt".into();
    }
    if folded.contains("invest") || folded.contains("stock") || folded.contains("fund") || folded.contains("crypto") || folded.contains("bitcoin") {
        return "investment".into();
    }
    "other".into()
}

fn is_debt(kind: &str) -> bool {
    matches!(kind, "credit_card" | "loan" | "other_debt")
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
    let raw = value.trim().to_ascii_lowercase();
    let folded = fold(value);
    if folded == "every day" || folded == "everyday" {
        return "daily".into();
    }
    if raw == "every_n_days" || (folded.contains("every") && folded.contains("day") && folded.chars().any(|c| c.is_ascii_digit())) {
        return "every_n_days".into();
    }
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
    if exp > 2 {
        let mut frac = format!("{:0width$}", amount % scale, width = exp as usize);
        while frac.len() > 2 && frac.ends_with('0') {
            frac.pop();
        }
        return format!("{sign}{}.{} {currency}", amount / scale, frac);
    }
    format!("{sign}{}.{:0width$} {currency}", amount / scale, amount % scale, width = exp as usize)
}

fn exponent(currency: &str) -> u32 {
    crate::finance_fx::exponent(currency)
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
        "usd" | "dollar" | "dollars" | "buck" | "bucks" | "$" | "us$" => "USD",
        "xmr" | "monero" => "XMR",
        "btc" | "bitcoin" => "BTC",
        "eth" | "ethereum" => "ETH",
        "sol" | "solana" => "SOL",
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
        let token = fold(stripped);
        if matches!(token.as_str(), "debt" | "owed" | "owing" | "owe" | "balance" | "worth" | "about") {
            continue;
        }
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

fn lead_arg(args: &Value) -> Option<u32> {
    args.get("leadDays").and_then(|value| value.as_u64().or_else(|| value.as_i64().map(|n| n.max(0) as u64))).map(|days| days.min(30) as u32)
}

fn money_for(amount: &str, currency_arg: &str, account_currency: &str, default_code: &str) -> Result<(i64, String), String> {
    let (number, embedded) = peel_currency(amount);
    let named = if !currency_arg.trim().is_empty() {
        Some(canonical_currency(currency_arg)?)
    } else {
        embedded
    };
    let currency = if let Some(code) = named {
        code
    } else if !account_currency.trim().is_empty() {
        canonical_currency(account_currency).unwrap_or_else(|_| account_currency.trim().to_ascii_uppercase())
    } else if let Ok(code) = canonical_currency(default_code) {
        code
    } else {
        currency_for_locale(default_code).to_string()
    };
    let minor = parse_minor(&number, exponent(&currency))?;
    Ok((minor, currency))
}

fn normalize_color(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let hex = trimmed.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Ok(format!("#{}", hex.to_ascii_uppercase()));
    }
    Err("Use a color like #E25B45.".into())
}

const PALETTE: [&str; 10] = ["#4C78D8", "#6E8B74", "#A56B66", "#C4B5A0", "#7A6A58", "#3E6B73", "#8C6A3F", "#B8A48A", "#5C6B58", "#9A7B6A"];

fn next_color(store: &Store) -> String {
    let used: HashSet<&str> = store.accounts.iter().map(|account| account.color.as_str()).collect();
    PALETTE.into_iter().find(|color| !used.contains(color)).unwrap_or(PALETTE[store.accounts.len() % PALETTE.len()]).to_string()
}

fn normalize_investment_type(value: &str) -> String {
    match fold(value).as_str() {
        "stock" | "stocks" | "share" | "shares" | "equity" => "stock".into(),
        "fund" | "etf" | "index" => "fund".into(),
        "crypto" | "cryptocurrency" | "coin" | "token" => "crypto".into(),
        "custom" | "other" => "custom".into(),
        _ => String::new(),
    }
}

fn infer_investment_type(name: &str, ticker: &str) -> String {
    let blob = fold(&format!("{name} {ticker}"));
    if blob.contains("crypto") || blob.contains("bitcoin") || blob.contains("btc") || blob.contains("eth") {
        return "crypto".into();
    }
    if blob.contains("fund") || blob.contains("etf") {
        return "fund".into();
    }
    if blob.contains("stock") || blob.contains("share") {
        return "stock".into();
    }
    "custom".into()
}

fn normalize_units(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }
    let normalized = normalize_decimal(raw)?;
    if normalized.split('.').next().unwrap_or("").is_empty() {
        return Err("The amount has to be more than zero.".into());
    }
    Ok(normalized)
}

fn units_times_price(units: &str, price_minor: i64) -> Result<i64, String> {
    let normalized = normalize_units(units)?;
    let (whole, frac) = match normalized.split_once('.') {
        Some((whole, frac)) => (whole, frac),
        None => (normalized.as_str(), ""),
    };
    let whole: i128 = whole.parse().map_err(|_| "That amount is too large.".to_string())?;
    let mut frac_digits: Vec<u8> = frac.bytes().take(6).map(|byte| byte - b'0').collect();
    while frac_digits.len() < 6 {
        frac_digits.push(0);
    }
    let mut frac_val: i128 = 0;
    for digit in &frac_digits {
        frac_val = frac_val * 10 + i128::from(*digit);
    }
    let product = whole * i128::from(price_minor) + (i128::from(price_minor) * frac_val + 500_000) / 1_000_000;
    if product <= 0 || product > 100_000_000_000_000 {
        return Err("That amount is too large.".into());
    }
    i64::try_from(product).map_err(|_| "That amount is too large.".to_string())
}

fn remember_category(store: &mut Store, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    if store.categories.iter().any(|category| fold(&category.name) == fold(name)) {
        return;
    }
    let color = PALETTE[store.categories.len() % PALETTE.len()].to_string();
    store.categories.push(Category { name: name.to_string(), color });
}

fn rename_category(store: &mut Store, from: &str, to: &str) -> Result<(), String> {
    let to = clean_text(to, 80)?;
    if to.is_empty() {
        return Err("A category needs a name.".into());
    }
    let Some(index) = store.categories.iter().position(|category| fold(&category.name) == fold(from)) else {
        return Err("No category with that name.".into());
    };
    let previous = store.categories[index].name.clone();
    store.categories[index].name = to.clone();
    for tx in &mut store.transactions {
        if fold(&tx.category) == fold(&previous) {
            tx.category = to.clone();
        }
    }
    for rule in &mut store.rules {
        if fold(&rule.category) == fold(&previous) {
            rule.category = to.clone();
        }
    }
    Ok(())
}

fn repeating_rule_index(store: &Store, name: &str, account_id: &str, direction: &str) -> Option<usize> {
    let wanted = fold(name);
    if wanted.is_empty() {
        return None;
    }
    let hits: Vec<usize> = store
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| recurrence::is_repeating(&rule.cadence) || !rule.next_date.is_empty())
        .filter(|(_, rule)| fold(&rule.name) == wanted || (!account_id.is_empty() && rule.account_id == account_id && fold(&rule.name) == wanted))
        .filter(|(_, rule)| direction.is_empty() || rule.direction == direction || (direction == "payment" && rule.direction == "expense") || (direction == "expense" && rule.direction == "payment"))
        .map(|(index, _)| index)
        .collect();
    (hits.len() == 1).then(|| hits[0])
}

fn advance_rule_at(store: &mut Store, index: usize, clock: &Clock) -> String {
    let occurrence = parse_iso_date(&store.rules[index].next_date);
    roll_schedule(store, index, occurrence, clock)
}

fn roll_schedule(store: &mut Store, index: usize, extra: Option<NaiveDate>, clock: &Clock) -> String {
    let _ = hydrate_rule(&mut store.rules[index]);
    let name = store.rules[index].name.clone();
    let id = store.rules[index].id.clone();
    let spec = rule_spec(&store.rules[index]);
    let mut settled = settled_dates(store, &id);
    if let Some(extra) = extra {
        if !settled.contains(&extra) {
            settled.push(extra);
        }
    }
    match crate::finance_schedule::next_open(&spec, &settled, clock.today) {
        Some(next) => {
            store.rules[index].next_date = iso(next);
            store.rules[index].updated_ms = clock.ms;
            if store.rules[index].status == "completed" {
                store.rules[index].status = "active".into();
            }
            let date = iso(next);
            spoken("Next {name} is {date}.", &[("name", &name), ("date", &date)])
        }
        None => {
            store.rules[index].next_date.clear();
            store.rules[index].updated_ms = clock.ms;
            if spec.end_kind == "after_count" || spec.cadence.is_empty() {
                store.rules[index].status = "completed".into();
            }
            if store.rules[index].direction == "income" {
                spoken("{name} is marked received.", &[("name", &name)])
            } else {
                spoken("{name} is marked paid.", &[("name", &name)])
            }
        }
    }
}

fn rule_spec(rule: &Rule) -> crate::finance_schedule::Spec {
    let repeating = schedule_repeats(&rule.cadence, rule.interval_days);
    let start = parse_iso_date(if rule.start_date.is_empty() { &rule.next_date } else { &rule.start_date }).unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    crate::finance_schedule::Spec {
        cadence: if repeating { rule.cadence.clone() } else { String::new() },
        interval_days: rule.interval_days,
        start,
        end_kind: if rule.end_kind.is_empty() { if repeating { "none".into() } else { "after_count".into() } } else { rule.end_kind.clone() },
        end_count: if rule.end_count == 0 && !repeating { 1 } else { rule.end_count },
        end_date: parse_iso_date(&rule.end_date),
        amount_minor: rule.amount_minor,
        skipped: rule.skipped_dates.iter().filter_map(|date| parse_iso_date(date)).collect(),
        status: if rule.status.is_empty() { "active".into() } else { rule.status.clone() },
    }
}

fn settled_dates(store: &Store, rule_id: &str) -> Vec<NaiveDate> {
    store
        .transactions
        .iter()
        .filter(|tx| tx.rule_id == rule_id)
        .filter_map(|tx| parse_iso_date(if tx.occurrence_date.is_empty() { tx.date.as_str() } else { tx.occurrence_date.as_str() }))
        .collect()
}

fn mark_paid(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let id = args["id"].as_str().unwrap_or("").trim();
    let name = args["name"].as_str().unwrap_or("").trim();
    let index = if !id.is_empty() {
        store.rules.iter().position(|rule| rule.id == id).ok_or("No recurring item with that id.")?
    } else {
        let hits: Vec<usize> = store.rules.iter().enumerate().filter(|(_, rule)| fold(&rule.name) == fold(name)).map(|(index, _)| index).collect();
        match hits.len() {
            1 => hits[0],
            0 => return Err("Which payment?".into()),
            _ => {
                let names = and_list(hits.iter().map(|index| store.rules[*index].name.as_str()));
                return Err(spoken("Which payment? I found {names}.", &[("names", &names)]));
            }
        }
    };
    let rule = store.rules[index].clone();
    let saved_currency = if rule.currency.is_empty() { clock.default_currency.clone() } else { rule.currency.clone() };
    let (amount, currency) = if amount_text(args.get("amount")).trim().is_empty() {
        let minor = rule.amount_minor.ok_or("Say the amount. None is saved on that payment.")?;
        (minor, saved_currency)
    } else {
        money_for(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(""), &saved_currency, &clock.default_currency)?
    };
    let direction = if matches!(rule.direction.as_str(), "income" | "expense" | "payment") { rule.direction.clone() } else { "payment".into() };
    let explicit = require_date(args["date"].as_str().unwrap_or(""), "", clock.today)?;
    let occurrence = if !explicit.is_empty() {
        explicit.clone()
    } else if !rule.next_date.is_empty() {
        rule.next_date.clone()
    } else {
        iso(clock.today)
    };
    if store.transactions.iter().any(|tx| tx.rule_id == rule.id && tx.occurrence_date == occurrence) {
        return Ok(spoken("{name} is already marked for {date}.", &[("name", &rule.name), ("date", &occurrence)]));
    }
    let posted = if explicit.is_empty() { iso(clock.today) } else { explicit };
    let id = alloc(store, "ft");
    store.transactions.push(Transaction {
        id,
        amount_minor: amount,
        currency: currency.clone(),
        direction: direction.clone(),
        category: rule.name.clone(),
        account_id: rule.account_id.clone(),
        from_account_id: String::new(),
        date: posted,
        notes: if rule.amount_mode == "quote" && !rule.payout_currency.is_empty() { format!("Settled in {}.", rule.payout_currency) } else { String::new() },
        recurrence: "once".into(),
        rule_id: rule.id.clone(),
        created_ms: clock.ms,
        updated_ms: clock.ms,
        occurrence_date: occurrence.clone(),
    });
    let rolled = roll_schedule(store, index, parse_iso_date(&occurrence), clock);
    let verb = spoken(if direction == "income" { "received" } else { "paid" }, &[]);
    let amount_label = format_minor(amount, &currency);
    Ok(spoken(
        "Marked {name} {verb}, {amount}. {rolled}",
        &[("name", &rule.name), ("verb", &verb), ("amount", &amount_label), ("rolled", &rolled)],
    ))
}

fn relative_days(days: i64) -> String {
    match days {
        i64::MIN..=-1 => spoken("overdue", &[]),
        0 => spoken("today", &[]),
        1 => spoken("tomorrow", &[]),
        n => {
            let count = n.to_string();
            spoken("in {n} days", &[("n", &count)])
        }
    }
}

fn pay_title(rule: &Rule) -> String {
    let name = rule.name.trim();
    if rule.direction == "income" {
        if fold(name).starts_with("payday") { name.to_string() } else { format!("Payday {name}") }
    } else if fold(name).starts_with("pay ") {
        name.to_string()
    } else {
        format!("Pay {name}")
    }
}

fn reminder_slot(due: NaiveDate, lead: u32, now: chrono::DateTime<Local>) -> Option<(chrono::DateTime<Local>, u32)> {
    if lead == 0 {
        return None;
    }
    let due_morning = morning(due)?;
    let notify = due_morning - Duration::days(i64::from(lead));
    if notify > now + Duration::minutes(1) {
        return Some((notify, lead));
    }
    if due_morning > now + Duration::hours(20) {
        return Some((now + Duration::minutes(2), lead));
    }
    None
}

fn sync_pay_reminder(app: &AppHandle, store: &mut Store, rule_id: &str, now: chrono::DateTime<Local>) {
    let Some(rule) = store.rules.iter().find(|rule| rule.id == rule_id).cloned() else { return };
    let agenda = app.state::<crate::actions::Agenda>();
    if matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
        clear_reminder(app, store, rule_id, rule.reminder_id);
        return;
    }
    let due = parse_iso_date(&rule.next_date);
    let lead = rule.lead_days.unwrap_or(2).min(30);
    let title = pay_title(&rule);
    let note = rule.amount_minor.map(|amount| format_minor(amount, &rule.currency)).unwrap_or_default();
    let Some(date) = due else {
        clear_reminder(app, store, rule_id, rule.reminder_id);
        return;
    };
    let Some((at, stored_lead)) = reminder_slot(date, lead, now) else {
        clear_reminder(app, store, rule_id, rule.reminder_id);
        return;
    };
    let schedule = crate::actions::ScheduleMeta { lead_days: stored_lead, ..Default::default() };
    let existing = (rule.reminder_id > 0).then_some(rule.reminder_id);
    let saved = agenda.upsert_item(app, existing, "reminder", &title, &note, at.timestamp_millis(), Some(schedule.clone()), None).or_else(|_| {
        agenda.upsert_item(app, None, "reminder", &title, &note, at.timestamp_millis(), Some(schedule), None)
    });
    if let Ok(item) = saved {
        if let Some(row) = store.rules.iter_mut().find(|row| row.id == rule_id) {
            row.reminder_id = item.id;
        }
    }
}

fn clear_reminder(app: &AppHandle, store: &mut Store, rule_id: &str, reminder_id: i32) {
    if reminder_id > 0 {
        let _ = app.state::<crate::actions::Agenda>().cancel(app, reminder_id);
    }
    if let Some(row) = store.rules.iter_mut().find(|row| row.id == rule_id) {
        row.reminder_id = 0;
    }
}

fn gc_reminders(app: &AppHandle, previous: &Store, store: &Store) {
    let keep: HashSet<i32> = store.rules.iter().map(|rule| rule.reminder_id).filter(|id| *id > 0).collect();
    for id in previous.rules.iter().map(|rule| rule.reminder_id).filter(|id| *id > 0) {
        if !keep.contains(&id) {
            let _ = app.state::<crate::actions::Agenda>().cancel(app, id);
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedPdf {
    pub file_name: String,
    pub data_b64: String,
    pub shared: bool,
    pub pages: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfRequest {
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub locale: String,
}

fn pdf_pages(store: &Store, request: &PdfRequest, today: NaiveDate, locale: &str) -> Vec<crate::finance_pdf::Page> {
    use crate::finance_pdf::{Chart, Page, Row, Slice};
    let span = period_span(&request.period, today, &request.from, &request.to);
    let account_id = locate_account(store, "", &request.account, "").ok().flatten().map(|found| store.accounts[found.index].id.clone()).unwrap_or_default();
    let direction = normalize_direction(&request.direction).unwrap_or_default();
    let txs: Vec<&Transaction> = store
        .transactions
        .iter()
        .filter(|tx| in_span(&tx.date, &span))
        .filter(|tx| request.currency.trim().is_empty() || tx.currency.eq_ignore_ascii_case(request.currency.trim()))
        .filter(|tx| request.category.trim().is_empty() || fold(&tx.category) == fold(&request.category))
        .filter(|tx| direction.is_empty() || tx.direction == direction)
        .filter(|tx| account_id.is_empty() || tx.account_id == account_id || tx.from_account_id == account_id)
        .collect();
    let accounts: Vec<&Account> = store
        .accounts
        .iter()
        .filter(|account| request.currency.trim().is_empty() || account.currency.eq_ignore_ascii_case(request.currency.trim()))
        .filter(|account| account_id.is_empty() || account.id == account_id)
        .collect();
    let mut nets: Vec<(String, i64, i64, i64)> = Vec::new();
    for account in &accounts {
        let Some(shown) = shown_balance(account, &store.transactions) else { continue };
        let row = nets.iter_mut().find(|row| row.0 == account.currency);
        let row = if let Some(row) = row {
            row
        } else {
            nets.push((account.currency.clone(), 0, 0, 0));
            nets.last_mut().unwrap()
        };
        if is_debt(&account.kind) {
            row.2 += shown;
        } else if account.kind == "investment" {
            row.3 += shown;
        } else {
            row.1 += shown;
        }
    }
    nets.sort_by(|a, b| a.0.cmp(&b.0));
    let heading = tr(locale, "Finance summary");
    let mut summary = Page {
        title: heading,
        subtitle: format!("{} · {}", tr(locale, "On this phone. No bank login."), span.label),
        notes: vec![tr(locale, "Balances are what you last said."), tr(locale, "No exchange rate is applied.")],
        numbers: nets
            .iter()
            .map(|(currency, money, debt, invest)| (format!("{} {}", tr(locale, "Net worth"), currency), format_minor(money + invest - debt, currency)))
            .collect(),
        rows: Vec::new(),
        chart: None,
    };
    for account in &accounts {
        let group = if is_debt(&account.kind) { tr(locale, "Debts") } else if account.kind == "investment" { tr(locale, "Investments") } else { tr(locale, "Money") };
        let amount = shown_balance(account, &store.transactions).map(|minor| format_minor(minor, &account.currency)).unwrap_or_else(|| tr(locale, "Not set"));
        summary.rows.push(Row { name: account.name.clone(), detail: group, amount, color: if account.color.is_empty() { "#C4B5A0".into() } else { account.color.clone() } });
    }
    let display = effective_default(store, locale);
    let parts: Vec<(i64, String)> = nets.iter().map(|row| (row.1 + row.3 - row.2, row.0.clone())).filter(|row| row.0 != 0).collect();
    if let Some(combined) = crate::finance_fx::combine(&parts, &display, &store.fx, Local::now().timestamp_millis()) {
        if combined.source != "identity" {
            summary.numbers.push((format!("Combined {}", combined.currency), format_minor(combined.amount_minor, &combined.currency)));
            let fresh = if combined.stale { " Rates are stale." } else { "" };
            summary.notes.push(format!("Combined in {} as of {}.{}", combined.currency, if combined.as_of.is_empty() { "the saved rate" } else { combined.as_of.as_str() }, fresh));
            summary.notes.retain(|note| note != "No exchange rate is applied." && note != &tr(locale, "No exchange rate is applied."));
        }
    }
    let mut pages = vec![summary];
    let currencies: Vec<String> = nets.iter().map(|row| row.0.clone()).collect();
    for currency in &currencies {
        let money = nets.iter().find(|row| &row.0 == currency).map(|row| row.1).unwrap_or(0);
        let debt = nets.iter().find(|row| &row.0 == currency).map(|row| row.2).unwrap_or(0);
        let invest = nets.iter().find(|row| &row.0 == currency).map(|row| row.3).unwrap_or(0);
        let slices = vec![
            Slice { label: tr(locale, "Money"), value: money.abs() as f64, color: "#6E8B74".into(), caption: format_minor(money, currency) },
            Slice { label: tr(locale, "Debts"), value: debt.abs() as f64, color: "#A56B66".into(), caption: format_minor(debt, currency) },
            Slice { label: tr(locale, "Investments"), value: invest.abs() as f64, color: "#C4B5A0".into(), caption: format_minor(invest, currency) },
        ];
        if slices.iter().all(|slice| slice.value == 0.0) {
            continue;
        }
        pages.push(Page {
            title: format!("{} · {currency}", tr(locale, "Money, debts, and investments")),
            subtitle: span.label.clone(),
            notes: vec![],
            numbers: vec![],
            rows: vec![],
            chart: Some(Chart::Donut { center_label: tr(locale, "Net worth"), center_value: format_minor(money + invest - debt, currency), slices }),
        });
    }
    let bars: Vec<Slice> = accounts
        .iter()
        .filter_map(|account| {
            let shown = shown_balance(account, &store.transactions)?;
            Some(Slice { label: account.name.clone(), value: shown.abs() as f64, color: color_or(&account.color), caption: format_minor(shown, &account.currency) })
        })
        .collect();
    if !bars.is_empty() {
        pages.push(Page { title: tr(locale, "Every account"), subtitle: span.label.clone(), notes: vec![], numbers: vec![], rows: vec![], chart: Some(Chart::Bars { rows: bars }) });
    }
    let totals = period_totals_filtered(&txs);
    if !totals.is_empty() {
        let captions: Vec<String> = totals.iter().map(|row| row.currency.clone()).collect();
        pages.push(Page {
            title: tr(locale, "Income and expenses"),
            subtitle: span.label.clone(),
            notes: vec![],
            numbers: vec![],
            rows: vec![],
            chart: Some(Chart::Groups {
                captions,
                series: vec![tr(locale, "Income"), tr(locale, "Expenses"), tr(locale, "Payments")],
                values: vec![
                    totals.iter().map(|row| row.income_minor as f64).collect(),
                    totals.iter().map(|row| row.expense_minor as f64).collect(),
                    totals.iter().map(|row| row.payment_minor as f64).collect(),
                ],
                colors: vec!["#6E8B74".into(), "#A56B66".into(), "#C4B5A0".into()],
            }),
        });
    }
    let mut categories: Vec<(String, i64, String)> = Vec::new();
    for tx in txs.iter().filter(|tx| tx.direction == "expense") {
        let name = if tx.category.is_empty() { tr(locale, "Expense") } else { tx.category.clone() };
        if let Some(row) = categories.iter_mut().find(|row| fold(&row.0) == fold(&name) && row.2 == tx.currency) {
            row.1 += tx.amount_minor;
        } else {
            let color = store.categories.iter().find(|category| fold(&category.name) == fold(&name)).map(|category| category.color.clone()).unwrap_or_else(|| "#8C6A3F".into());
            categories.push((name, tx.amount_minor, tx.currency.clone()));
            let _ = color;
        }
    }
    if !categories.is_empty() {
        let rows = categories
            .iter()
            .map(|(name, amount, currency)| Slice {
                label: name.clone(),
                value: *amount as f64,
                color: store.categories.iter().find(|category| fold(&category.name) == fold(name)).map(|category| category.color.clone()).unwrap_or_else(|| "#8C6A3F".into()),
                caption: format_minor(*amount, currency),
            })
            .collect();
        pages.push(Page { title: tr(locale, "By category"), subtitle: span.label.clone(), notes: vec![], numbers: vec![], rows: vec![], chart: Some(Chart::Bars { rows }) });
    }
    let trend = month_trend(&store.transactions, today, &request);
    if trend.iter().any(|point| point.1 != 0 || point.2 != 0) {
        pages.push(Page {
            title: tr(locale, "Six months"),
            subtitle: String::new(),
            notes: vec![],
            numbers: vec![],
            rows: vec![],
            chart: Some(Chart::Trend {
                labels: trend.iter().map(|point| point.0.clone()).collect(),
                income: trend.iter().map(|point| point.1 as f64).collect(),
                expense: trend.iter().map(|point| point.2 as f64).collect(),
                net: trend.iter().map(|point| (point.1 - point.2) as f64).collect(),
            }),
        });
    }
    let debts: Vec<Slice> = accounts
        .iter()
        .filter(|account| is_debt(&account.kind))
        .filter_map(|account| {
            let shown = shown_balance(account, &store.transactions)?;
            Some(Slice { label: account.name.clone(), value: shown as f64, color: color_or(&account.color), caption: format_minor(shown, &account.currency) })
        })
        .collect();
    if !debts.is_empty() {
        pages.push(Page { title: tr(locale, "Debts owed"), subtitle: String::new(), notes: vec![], numbers: vec![], rows: vec![], chart: Some(Chart::Bars { rows: debts }) });
    }
    let holdings: Vec<Slice> = accounts
        .iter()
        .filter(|account| account.kind == "investment")
        .filter_map(|account| {
            let shown = shown_balance(account, &store.transactions)?;
            Some(Slice { label: account.name.clone(), value: shown.abs() as f64, color: color_or(&account.color), caption: format_minor(shown, &account.currency) })
        })
        .collect();
    if !holdings.is_empty() {
        pages.push(Page {
            title: tr(locale, "Holdings"),
            subtitle: String::new(),
            notes: vec![],
            numbers: vec![],
            rows: vec![],
            chart: Some(Chart::Donut { center_label: tr(locale, "Worth"), center_value: String::new(), slices: holdings }),
        });
    }
    if !txs.is_empty() {
        let mut activity = Page { title: tr(locale, "Activity"), subtitle: span.label.clone(), notes: vec![], numbers: vec![], rows: vec![], chart: None };
        for tx in txs.iter().take(12) {
            activity.rows.push(Row {
                name: if tx.category.is_empty() { tx.direction.clone() } else { tx.category.clone() },
                detail: tx.date.clone(),
                amount: format_minor(tx.amount_minor, &tx.currency),
                color: "#C4B5A0".into(),
            });
        }
        pages.push(activity);
    }
    pages
}

struct DateSpan {
    start: String,
    end: String,
    label: String,
}

fn period_span(period: &str, today: NaiveDate, from: &str, to: &str) -> DateSpan {
    let key = fold(period);
    if !from.trim().is_empty() || !to.trim().is_empty() || key == "custom" {
        return DateSpan { start: from.trim().to_string(), end: to.trim().to_string(), label: format!("{} – {}", from.trim(), to.trim()) };
    }
    if key == "all" || key == "all time" {
        return DateSpan { start: String::new(), end: String::new(), label: "All time".into() };
    }
    if key.len() == 7 && key.as_bytes().get(4) == Some(&b'-') {
        return month_span_key(&key);
    }
    if key == "last" || key == "last month" || key == "last_month" {
        let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        let prev = first.pred_opt().unwrap_or(first);
        return month_span(prev.year(), prev.month());
    }
    if key == "year" || key == "this year" {
        return DateSpan { start: format!("{:04}-01-01", today.year()), end: format!("{:04}-12-31", today.year()), label: today.year().to_string() };
    }
    if key == "six" || key == "last 6" || key == "last_6" || key == "last 6 months" {
        let start = add_months(NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today), -5).unwrap_or(today);
        let end = end_of_month(today);
        return DateSpan { start: iso(start), end: iso(end), label: "Last 6 months".into() };
    }
    month_span(today.year(), today.month())
}

fn month_span(year: i32, month: u32) -> DateSpan {
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    DateSpan { start: iso(start), end: iso(end_of_month(start)), label: format!("{year:04}-{month:02}") }
}

fn month_span_key(key: &str) -> DateSpan {
    let mut parts = key.split('-');
    let year = parts.next().and_then(|value| value.parse().ok()).unwrap_or(2000);
    let month = parts.next().and_then(|value| value.parse().ok()).unwrap_or(1);
    month_span(year, month.clamp(1, 12))
}

fn end_of_month(date: NaiveDate) -> NaiveDate {
    add_months(NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date), 1).and_then(|next| next.pred_opt()).unwrap_or(date)
}

fn in_span(date: &str, span: &DateSpan) -> bool {
    (span.start.is_empty() || date >= span.start.as_str()) && (span.end.is_empty() || date <= span.end.as_str())
}

fn period_totals_filtered(transactions: &[&Transaction]) -> Vec<PeriodTotal> {
    let owned: Vec<Transaction> = transactions.iter().map(|tx| (*tx).clone()).collect();
    period_totals(&owned, "", "", "", "")
}

fn month_trend(transactions: &[Transaction], today: NaiveDate, request: &PdfRequest) -> Vec<(String, i64, i64)> {
    let start = add_months(NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today), -5).unwrap_or(today);
    let mut points = Vec::new();
    let mut cursor = start;
    for _ in 0..6 {
        let key = format!("{:04}-{:02}", cursor.year(), cursor.month());
        let mut income = 0i64;
        let mut expense = 0i64;
        for tx in transactions {
            if !tx.date.starts_with(&key) {
                continue;
            }
            if !request.currency.trim().is_empty() && !tx.currency.eq_ignore_ascii_case(request.currency.trim()) {
                continue;
            }
            if !request.category.trim().is_empty() && fold(&tx.category) != fold(&request.category) {
                continue;
            }
            if tx.direction == "income" {
                income += tx.amount_minor;
            }
            if tx.direction == "expense" {
                expense += tx.amount_minor;
            }
        }
        points.push((format!("{:02}", cursor.month()), income, expense));
        cursor = add_months(cursor, 1).unwrap_or(cursor);
    }
    points
}

fn color_or(color: &str) -> String {
    if color.trim().is_empty() { "#C4B5A0".into() } else { color.to_string() }
}

fn tr(locale: &str, key: &str) -> String {
    let lang = crate::i18n::normalize(locale);
    let table: &[(&str, &str)] = match lang {
        "es" => ES_PDF,
        "de" => DE_PDF,
        "pt" => PT_PDF,
        "ca-ES-valencia" => CA_PDF,
        _ => &[],
    };
    table.iter().find(|(id, _)| *id == key).map(|(_, text)| (*text).to_string()).unwrap_or_else(|| key.to_string())
}

const ES_PDF: &[(&str, &str)] = &[
    ("Finance summary", "Resumen de finanzas"),
    ("On this phone. No bank login.", "En este teléfono. Sin acceso al banco."),
    ("Balances are what you last said.", "Los saldos son lo último que dijiste."),
    ("No exchange rate is applied.", "No se aplica ningún tipo de cambio."),
    ("Net", "Neto"),
    ("Net worth", "Patrimonio neto"),
    ("Money", "Dinero"),
    ("Debts", "Deudas"),
    ("Investments", "Inversiones"),
    ("Not set", "Sin indicar"),
    ("Money, debts, and investments", "Dinero, deudas e inversiones"),
    ("Every account", "Cada cuenta"),
    ("Income and expenses", "Ingresos y gastos"),
    ("Income", "Ingresos"),
    ("Expenses", "Gastos"),
    ("Payments", "Pagos"),
    ("By category", "Por categoría"),
    ("Expense", "Gasto"),
    ("Six months", "Seis meses"),
    ("Debts owed", "Deudas pendientes"),
    ("Holdings", "Posiciones"),
    ("Worth", "Valor"),
    ("Activity", "Movimientos"),
];

const DE_PDF: &[(&str, &str)] = &[
    ("Finance summary", "Finanzüberblick"),
    ("On this phone. No bank login.", "Auf diesem Telefon. Kein Bankzugang."),
    ("Balances are what you last said.", "Die Salden sind, was du zuletzt gesagt hast."),
    ("No exchange rate is applied.", "Es wird kein Wechselkurs angewendet."),
    ("Net", "Saldo"),
    ("Net worth", "Vermögen"),
    ("Money", "Geld"),
    ("Debts", "Schulden"),
    ("Investments", "Anlagen"),
    ("Not set", "Nicht angegeben"),
    ("Money, debts, and investments", "Geld, Schulden und Anlagen"),
    ("Every account", "Jedes Konto"),
    ("Income and expenses", "Einnahmen und Ausgaben"),
    ("Income", "Einnahmen"),
    ("Expenses", "Ausgaben"),
    ("Payments", "Zahlungen"),
    ("By category", "Nach Kategorie"),
    ("Expense", "Ausgabe"),
    ("Six months", "Sechs Monate"),
    ("Debts owed", "Offene Schulden"),
    ("Holdings", "Bestände"),
    ("Worth", "Wert"),
    ("Activity", "Bewegungen"),
];

const PT_PDF: &[(&str, &str)] = &[
    ("Finance summary", "Resumo financeiro"),
    ("On this phone. No bank login.", "Neste telefone. Sem acesso ao banco."),
    ("Balances are what you last said.", "Os saldos são o que você disse por último."),
    ("No exchange rate is applied.", "Nenhum câmbio é aplicado."),
    ("Net", "Líquido"),
    ("Net worth", "Patrimônio líquido"),
    ("Money", "Dinheiro"),
    ("Debts", "Dívidas"),
    ("Investments", "Investimentos"),
    ("Not set", "Não indicado"),
    ("Money, debts, and investments", "Dinheiro, dívidas e investimentos"),
    ("Every account", "Cada conta"),
    ("Income and expenses", "Receitas e despesas"),
    ("Income", "Receitas"),
    ("Expenses", "Despesas"),
    ("Payments", "Pagamentos"),
    ("By category", "Por categoria"),
    ("Expense", "Despesa"),
    ("Six months", "Seis meses"),
    ("Debts owed", "Dívidas em aberto"),
    ("Holdings", "Posições"),
    ("Worth", "Valor"),
    ("Activity", "Movimentos"),
];

const CA_PDF: &[(&str, &str)] = &[
    ("Finance summary", "Resum de finances"),
    ("On this phone. No bank login.", "En este telèfon. Sense accés al banc."),
    ("Balances are what you last said.", "Els saldos són l'últim que has dit."),
    ("No exchange rate is applied.", "No s'aplica cap tipus de canvi."),
    ("Net", "Saldo"),
    ("Net worth", "Patrimoni net"),
    ("Money", "Diners"),
    ("Debts", "Deutes"),
    ("Investments", "Inversions"),
    ("Not set", "Sense indicar"),
    ("Money, debts, and investments", "Diners, deutes i inversions"),
    ("Every account", "Cada compte"),
    ("Income and expenses", "Ingressos i despeses"),
    ("Income", "Ingressos"),
    ("Expenses", "Despeses"),
    ("Payments", "Pagaments"),
    ("By category", "Per categoria"),
    ("Expense", "Despesa"),
    ("Six months", "Sis mesos"),
    ("Debts owed", "Deutes pendents"),
    ("Holdings", "Posicions"),
    ("Worth", "Valor"),
    ("Activity", "Moviments"),
];

fn export_from_tool(app: &AppHandle, finance: &Finance, args: &Value) -> Result<Option<Value>, String> {
    if !args["confirmed"].as_bool().unwrap_or(false) {
        let period = args["period"].as_str().unwrap_or("this_month");
        return Ok(Some(json!({
            "ok": false,
            "needsConfirm": true,
            "detail": spoken("Export the Finance summary for {period} as a PDF and open the share sheet? Say yes. Nothing is uploaded.", &[("period", period)])
        })));
    }
    let request = PdfRequest {
        period: args["period"].as_str().unwrap_or("this_month").to_string(),
        account: args["account"].as_str().unwrap_or("").to_string(),
        category: args["category"].as_str().unwrap_or("").to_string(),
        currency: args["currency"].as_str().unwrap_or("").to_string(),
        direction: args["direction"].as_str().unwrap_or("").to_string(),
        from: args["from"].as_str().unwrap_or("").to_string(),
        to: args["to"].as_str().unwrap_or("").to_string(),
        locale: args["locale"].as_str().unwrap_or("").to_string(),
    };
    let exported = finance.export_pdf(app, &request)?;
    Ok(Some(json!({
        "ok": true,
        "detail": spoken("Saved {file}. The share sheet is open. Nothing was uploaded.", &[("file", &exported.file_name)]),
        "fileName": exported.file_name,
        "shared": exported.shared
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn clock(today: NaiveDate) -> Clock {
        Clock { ms: 5_000, today, currency: "de-DE".into(), default_currency: "EUR".into() }
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
            occurrence_date: String::new(),
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
        rule_from_tool(&mut store, &json!({"name": "Rent", "date": "1", "cadence": "monthly"}), &clock, false).unwrap();
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

    #[test]
    fn zkb_bank_does_not_overwrite_the_credit_card() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        account_from_tool(&mut store, &json!({"name": "ZKB credit card", "kind": "credit_card", "balance": "190", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "ZKB", "kind": "bank", "balance": "5.73", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Swiss card", "kind": "debt", "balance": "113.95 CHF debt"}), &clock).unwrap();
        assert_eq!(store.accounts.len(), 3);
        let card = store.accounts.iter().find(|account| account.name == "ZKB credit card").unwrap();
        assert_eq!(card.kind, "credit_card");
        assert_eq!(card.balance_minor, Some(19_000));
        let bank = store.accounts.iter().find(|account| account.name == "ZKB").unwrap();
        assert_eq!(bank.kind, "bank");
        assert_eq!(bank.balance_minor, Some(573));
        let swiss = store.accounts.iter().find(|account| account.name == "Swiss card").unwrap();
        assert_eq!(swiss.kind, "credit_card");
        assert_eq!(swiss.balance_minor, Some(11_395));
        assert_eq!(swiss.currency, "CHF");
        account_from_tool(&mut store, &json!({"name": "ZKB", "kind": "credit_card", "balance": "9"}), &clock).unwrap();
        assert_eq!(store.accounts.iter().find(|account| account.name == "ZKB").unwrap().kind, "bank");
        let err = locate_account(&store, "", "card", "").unwrap_err();
        assert!(err.starts_with("Which account? I found"));
    }

    #[test]
    fn default_currency_and_account_currency_win_over_usd() {
        let mut store = Store::default();
        store.locale_hint = "en-US".into();
        assert_eq!(effective_default(&store, "en-US"), "USD");
        store.default_currency = "CHF".into();
        assert_eq!(effective_default(&store, "en-US"), "CHF");
        let clock = Clock { ms: 5_000, today: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(), currency: "en-US".into(), default_currency: "CHF".into() };
        account_from_tool(&mut store, &json!({"name": "Mercury", "kind": "bank", "balance": "12.31"}), &clock).unwrap();
        assert_eq!(store.accounts[0].currency, "CHF");
        account_from_tool(&mut store, &json!({"name": "Quanloop", "kind": "investment", "balance": "3280.72", "currency": "EUR"}), &clock).unwrap();
        log_from_tool(&mut store, &json!({"direction": "expense", "amount": "20", "account": "Quanloop"}), &clock).unwrap();
        assert_eq!(store.transactions[0].currency, "EUR");
        account_from_tool(&mut store, &json!({"defaultCurrency": "EUR"}), &clock).unwrap();
        assert_eq!(store.default_currency, "EUR");
    }

    #[test]
    fn other_debt_units_and_tools() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        account_from_tool(&mut store, &json!({"name": "Family", "kind": "other debt", "balance": "40", "currency": "CHF"}), &clock).unwrap();
        assert_eq!(store.accounts[0].kind, "other_debt");
        account_from_tool(&mut store, &json!({"name": "Cash", "kind": "cash", "balance": "100", "currency": "CHF"}), &clock).unwrap();
        let mut net = 0i64;
        for account in &store.accounts {
            let shown = shown_balance(account, &[]).unwrap();
            net += if is_debt(&account.kind) { -shown } else { shown };
        }
        assert_eq!(net, 6_000);
        account_from_tool(&mut store, &json!({"name": "Nitroeggs", "kind": "investment", "investmentType": "custom", "units": "2", "unitPrice": "50", "currency": "CHF"}), &clock).unwrap();
        let eggs = store.accounts.iter().find(|account| account.name == "Nitroeggs").unwrap();
        assert_eq!(eggs.investment_type, "custom");
        assert_eq!(shown_balance(eggs, &[]), Some(10_000));
        account_from_tool(&mut store, &json!({"name": "Nitroeggs", "kind": "investment", "unitPrice": "60"}), &clock).unwrap();
        let eggs = store.accounts.iter().find(|account| account.name == "Nitroeggs").unwrap();
        assert_eq!(shown_balance(eggs, &[]), Some(12_000));
        account_from_tool(&mut store, &json!({"name": "Nitroeggs", "kind": "investment", "balance": "80", "currency": "CHF"}), &clock).unwrap();
        let eggs = store.accounts.iter().find(|account| account.name == "Nitroeggs").unwrap();
        assert!(eggs.units.is_empty());
        assert_eq!(eggs.balance_minor, Some(8_000));
        let names: Vec<_> = tool_definitions().as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"export_finance_pdf".into()));
        assert!(names.contains(&"mark_paid".into()));
        let tools = tool_definitions();
        let save = tools.as_array().unwrap().iter().find(|tool| tool["name"] == "save_account").unwrap();
        let description = save["description"].as_str().unwrap();
        assert!(description.contains("other_debt"));
        assert!(description.contains("defaultCurrency"));
        assert!(description.contains("unitPrice"));
        assert!(description.contains("changeKind"));
    }

    #[test]
    fn rent_reminder_rolls_forward_when_marked_paid() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        rule_from_tool(&mut store, &json!({"name": "Rent", "amount": "1200", "currency": "CHF", "cadence": "monthly", "date": "2026-12-01", "direction": "payment", "leadDays": 2}), &clock, false).unwrap();
        let rent = store.rules.iter().find(|rule| rule.name == "Rent").unwrap();
        assert_eq!(rent.lead_days, Some(2));
        assert_eq!(pay_title(rent), "Pay Rent");
        mark_paid(&mut store, &json!({"name": "Rent"}), &clock).unwrap();
        let rent = store.rules.iter().find(|rule| rule.name == "Rent").unwrap();
        assert_eq!(rent.next_date, "2027-01-01");
        assert_eq!(store.transactions.len(), 1);
        assert_eq!(store.transactions[0].direction, "payment");
        assert_eq!(store.transactions[0].amount_minor, 120_000);
        let slot = reminder_slot(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(), 2, morning(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).unwrap());
        assert!(slot.is_some());
    }

    #[test]
    fn pdf_keeps_currencies_apart_and_draws_charts() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        account_from_tool(&mut store, &json!({"name": "Mercury", "kind": "bank", "balance": "12.31", "currency": "USD"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "ZKB credit card", "kind": "credit_card", "balance": "190", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "ZKB", "kind": "bank", "balance": "5.73", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Swiss card", "kind": "credit_card", "balance": "113.95", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Quanloop", "kind": "investment", "balance": "3280.72", "currency": "EUR", "color": "#4C78D8"}), &clock).unwrap();
        let request = PdfRequest { period: "this_month".into(), account: String::new(), category: String::new(), currency: String::new(), direction: String::new(), from: String::new(), to: String::new(), locale: "en".into() };
        let pages = pdf_pages(&store, &request, clock.today, "en");
        let bytes = crate::finance_pdf::render(&pages);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("Mercury"));
        assert!(text.contains("Swiss card"));
        assert!(text.contains("113.95 CHF"));
        assert!(text.contains("3280.72 EUR") || text.contains("3,280.72") || text.contains("3280.72"));
        assert!(text.contains("5.73 CHF"));
        assert!(text.contains("re f") || text.contains(" re f") || text.contains("re\\n") || text.contains(" re\n") || text.contains("re f\n") || bytes.windows(4).any(|w| w == b"re f"));
        assert!(!text.contains("1 CHF ="));
        assert!(pages.len() > 2);
    }

    #[test]
    fn phrases_program_schedules_without_mixing_money() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        apply_phrase(&mut store, "I'm paying a course for 9 months at 270 euros", &clock).unwrap();
        let course = store.rules.iter().find(|rule| rule.name == "Course").unwrap();
        assert_eq!(course.amount_minor, Some(27_000));
        assert_eq!(course.currency, "EUR");
        assert_eq!(course.end_count, 9);
        assert_eq!(course.cadence, "monthly");
        assert_eq!(course.end_kind, "after_count");
        assert!(store.transactions.is_empty());
        apply_phrase(&mut store, "rent is 2000 francs a month for as long as I live here", &clock).unwrap();
        let rent = store.rules.iter().find(|rule| rule.name == "Rent").unwrap();
        assert_eq!(rent.amount_minor, Some(200_000));
        assert_eq!(rent.currency, "CHF");
        assert_eq!(rent.end_kind, "none");
        assert_eq!(rent.end_count, 0);
        apply_phrase(&mut store, "I have 700 dollars in Nitroeggs that pays 14 dollars every 10 days in XMR", &clock).unwrap();
        let eggs = store.accounts.iter().find(|account| account.name == "Nitroeggs").unwrap();
        assert_eq!(eggs.kind, "investment");
        assert_eq!(eggs.balance_minor, Some(70_000));
        assert_eq!(eggs.currency, "USD");
        assert!(eggs.units.is_empty());
        let payout = store.rules.iter().find(|rule| rule.purpose == "investment_payout").unwrap();
        assert_eq!(payout.amount_minor, Some(1_400));
        assert_eq!(payout.currency, "USD");
        assert_eq!(payout.amount_mode, "quote");
        assert_eq!(payout.payout_currency, "XMR");
        assert_eq!(payout.interval_days, 10);
        assert_eq!(payout.cadence, "every_n_days");
        assert_ne!(payout.currency, payout.payout_currency);
        apply_phrase(&mut store, "I got paid 3000 this month", &clock).unwrap();
        assert_eq!(store.transactions.len(), 1);
        assert_eq!(store.transactions[0].direction, "income");
        assert_eq!(store.transactions[0].amount_minor, 300_000);
        assert_eq!(store.transactions[0].currency, "EUR");
        apply_phrase(&mut store, "I spent 40 on groceries", &clock).unwrap();
        assert_eq!(store.transactions[1].direction, "expense");
        assert_eq!(store.transactions[1].category, "Groceries");
        assert_eq!(store.transactions[1].amount_minor, 4_000);
        apply_phrase(&mut store, "I will get 500 euros in June", &clock).unwrap();
        let june = store.rules.iter().find(|rule| rule.purpose == "one_off").unwrap();
        assert_eq!(june.amount_minor, Some(50_000));
        assert_eq!(june.currency, "EUR");
        assert_eq!(june.start_date, "2027-06-01");
        assert_eq!(store.transactions.iter().filter(|tx| tx.currency == "EUR" && tx.amount_minor == 50_000).count(), 0);
        apply_phrase(&mut store, "Netflix subscription is 15 euros a month", &clock).unwrap();
        apply_phrase(&mut store, "stop the Netflix subscription", &clock).unwrap();
        assert_eq!(store.rules.iter().find(|rule| rule.name == "Netflix").unwrap().status, "stopped");
        let old = r#"{"id":"fr1","name":"Rent","amountMinor":120000,"currency":"CHF","direction":"expense","cadence":"monthly","nextDate":"2026-12-01"}"#;
        let mut legacy: Rule = serde_json::from_str(old).unwrap();
        assert!(legacy.purpose.is_empty());
        assert!(hydrate_rule(&mut legacy));
        assert_eq!(legacy.purpose, "recurring");
        assert_eq!(legacy.status, "active");
        assert_eq!(legacy.end_kind, "none");
        assert_eq!(legacy.start_date, "2026-12-01");
        mark_paid(&mut store, &json!({"name": "Course", "date": "2026-11-12"}), &clock).unwrap();
        let again = mark_paid(&mut store, &json!({"name": "Course", "date": "2026-11-12"}), &clock).unwrap();
        assert!(again.contains("already marked"));
        assert_eq!(store.transactions.iter().filter(|tx| tx.rule_id == store.rules.iter().find(|rule| rule.name == "Course").unwrap().id && tx.occurrence_date == "2026-11-12").count(), 1);
    }

    #[test]
    fn pdf_labels_leave_english_only_for_english() {
        let keys = [
            "Finance summary",
            "On this phone. No bank login.",
            "Balances are what you last said.",
            "No exchange rate is applied.",
            "Net",
            "Net worth",
            "Money",
            "Debts",
            "Investments",
            "Not set",
            "Money, debts, and investments",
            "Every account",
            "Income and expenses",
            "Income",
            "Expenses",
            "Payments",
            "By category",
            "Expense",
            "Six months",
            "Debts owed",
            "Holdings",
            "Worth",
            "Activity",
        ];
        for locale in ["es", "de", "pt", "ca-ES-valencia"] {
            for key in keys {
                let line = tr(locale, key);
                assert_ne!(line, key, "{locale} left {key} in English");
            }
        }
        assert_eq!(tr("en", "Net worth"), "Net worth");
        assert!(tr("de", "Net worth").contains('ö'));
        assert!(tr("ca-ES-valencia", "Net worth").contains("Patrimoni"));
    }

    #[test]
    fn spoken_confirmation_follows_the_app_locale() {
        let guard = push_locale("es");
        let line = spoken("Delete {label}? Say yes and I will remove it.", &[("label", "Caja")]);
        assert!(line.contains("Caja"));
        assert!(!line.contains("Say yes"), "{line}");
        drop(guard);
        let english = spoken("Delete {label}? Say yes and I will remove it.", &[("label", "Cash")]);
        assert!(english.contains("Say yes"));
    }
}

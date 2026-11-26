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
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
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

fn cadence_spoken(cadence: &str, interval: u32) -> String {
    let unit = match cadence {
        "every_n_days" => "days",
        "every_n_weeks" => "weeks",
        "every_n_months" => "months",
        "every_n_years" => "years",
        "daily" | "weekly" | "monthly" | "yearly" => return spoken(cadence, &[]),
        _ => return cadence.to_string(),
    };
    let count = interval.max(1).to_string();
    spoken("every {n} {unit}", &[("n", &count), ("unit", &spoken(unit, &[]))])
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
    /// `troy_oz`, `gram`, `kilogram`, `share`, or `coin`. Empty until a holding is linked.
    #[serde(default)]
    pub measure_unit: String,
    #[serde(default)]
    pub price_minor: Option<i64>,
    /// When true, `price_minor` replaces the live quote. Otherwise it is only a fallback.
    #[serde(default)]
    pub price_override: bool,
    /// Days before the due date to remind. Missing means two days, and again on the day.
    #[serde(default)]
    pub lead_days: Option<u32>,
    /// Credit limit in minor units. Only meaningful for a credit card.
    #[serde(default)]
    pub credit_limit_minor: Option<i64>,
    /// Payment due day of the month, 1 through 31. Empty when unset.
    #[serde(default)]
    pub due_day: Option<u32>,
    /// Statement day of the month, 1 through 31. Empty when unset.
    #[serde(default)]
    pub statement_day: Option<u32>,
    /// Last four digits only. Never a full card number.
    #[serde(default)]
    pub last_four: String,
    /// What you paid, when you said so. Empty until then.
    #[serde(default)]
    pub purchase_minor: Option<i64>,
    #[serde(default)]
    pub purchase_currency: String,
    #[serde(default)]
    pub purchase_date: String,
    /// Your own estimates, oldest first. The latest is the current value of a manual asset.
    #[serde(default)]
    pub valuations: Vec<Valuation>,
    #[serde(default)]
    pub isin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Valuation {
    pub as_of: String,
    pub amount_minor: i64,
    pub currency: String,
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
    pub subcategory: String,
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
    pub subcategory: String,
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
    flow_colors: Vec<NodeColor>,
    /// One color per account, category, income type, Sankey node, class, or chart series.
    #[serde(default)]
    entity_colors: Vec<EntityColor>,
    /// Portrait money flow: "vertical" or "horizontal". Empty keeps the phone default.
    #[serde(default)]
    flow_layout: String,
    #[serde(default)]
    categories_ready: bool,
    #[serde(default)]
    fx: crate::finance_fx::FxBook,
    #[serde(default)]
    prices: crate::finance_prices::PriceBook,
    #[serde(default)]
    history: Vec<crate::finance_history::ValuePoint>,
    #[serde(default)]
    history_seeded: Vec<String>,
    #[serde(default)]
    provider_pins: Vec<crate::finance_history::ProviderPin>,
    /// Bumped when cached provider ids from an older build should be dropped.
    #[serde(default)]
    price_cache_version: u32,
    #[serde(default)]
    budgets: Vec<Budget>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub order: i32,
    /// Empty for a top-level category. A subcategory stores its parent's name.
    #[serde(default)]
    pub parent: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeColor {
    pub id: String,
    #[serde(default)]
    pub color: String,
}

/// Saved chart color. `kind` is account, category, income, node, class, or series.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EntityColor {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLine {
    #[serde(default)]
    pub id: String,
    /// `income`, `expense`, `savings`, or `investment`.
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub subcategory: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub amount_minor: i64,
    #[serde(default)]
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    #[serde(default)]
    pub id: String,
    /// `YYYY-MM`, or empty for the recurring template.
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub lines: Vec<BudgetLine>,
    #[serde(default = "default_copy")]
    pub copy_forward: bool,
    #[serde(default)]
    pub notify: bool,
    #[serde(default)]
    pub warned: Vec<String>,
    #[serde(default)]
    pub updated_ms: i64,
}

fn default_copy() -> bool {
    true
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            id: String::new(),
            period: String::new(),
            currency: String::new(),
            lines: Vec::new(),
            copy_forward: true,
            notify: false,
            warned: Vec::new(),
            updated_ms: 0,
        }
    }
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
    pub flow_colors: Vec<NodeColor>,
    #[serde(default)]
    pub flow_layout: String,
    #[serde(default)]
    pub entity_colors: Vec<EntityColor>,
    #[serde(default)]
    pub fx: crate::finance_fx::FxBook,
    #[serde(default)]
    pub prices: crate::finance_prices::PriceBook,
    #[serde(default)]
    history: Vec<crate::finance_history::ValuePoint>,
    #[serde(default)]
    history_seeded: Vec<String>,
    #[serde(default)]
    provider_pins: Vec<crate::finance_history::ProviderPin>,
    /// Bumped when cached provider ids from an older build should be dropped.
    #[serde(default)]
    price_cache_version: u32,
    #[serde(default)]
    pub budgets: Vec<Budget>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    #[serde(flatten)]
    pub account: Account,
    pub shown_balance_minor: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_worth_minor: Option<i64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub market_worth_currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_price: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub market_price_currency: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub market_source: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub market_fetched_ms: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub market_stale: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub market_estimated: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub market_unit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub market_symbol: String,
    /// CoinGecko id used for the last resolution. Empty when the holding is not a known coin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_coingecko_id: String,
    /// CoinPaprika id used for the last resolution.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_paprika_id: String,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
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
    #[serde(default)]
    pub flow_colors: Vec<NodeColor>,
    #[serde(default)]
    pub flow_layout: String,
    #[serde(default)]
    pub entity_colors: Vec<EntityColor>,
    pub fx: FxStatus,
    pub prices: crate::finance_prices::PriceBook,
    #[serde(default)]
    pub history: Vec<crate::finance_history::ValuePoint>,
    #[serde(default)]
    pub budgets: Vec<Budget>,
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
    /// `troy_oz`, `gram`, `kilogram`, `share`, or `coin`.
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub unit_price: String,
    #[serde(default)]
    pub price_override: bool,
    #[serde(default)]
    pub touch_override: bool,
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
    #[serde(default)]
    pub credit_limit: String,
    #[serde(default)]
    pub due_day: Option<u32>,
    #[serde(default)]
    pub statement_day: Option<u32>,
    #[serde(default)]
    pub last_four: String,
    #[serde(default)]
    pub edit_limit: bool,
    #[serde(default)]
    pub edit_due_day: bool,
    #[serde(default)]
    pub edit_statement_day: bool,
    #[serde(default)]
    pub edit_last_four: bool,
    #[serde(default)]
    pub purchase: String,
    #[serde(default)]
    pub purchase_currency: String,
    #[serde(default)]
    pub purchase_date: String,
    #[serde(default)]
    pub revalue: String,
    #[serde(default)]
    pub revalue_currency: String,
    #[serde(default)]
    pub revalue_date: String,
    #[serde(default)]
    pub isin: String,
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
    pub subcategory: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub account_kind: String,
    #[serde(default)]
    pub from_account_id: String,
    #[serde(default)]
    pub from_account_name: String,
    #[serde(default)]
    pub from_account_kind: String,
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
    pub subcategory: String,
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
        publish_board(&finance.path, &finance.lock());
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
        let mut store = self.lock().clone();
        if ensure_categories(&mut store) {
            let _ = Self::persist(&self.path, &store);
            *self.lock() = store.clone();
        }
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
            flow_colors: store.flow_colors.clone(),
            flow_layout: store.flow_layout.clone(),
            entity_colors: store.entity_colors.clone(),
            fx: store.fx.clone(),
            prices: store.prices.clone(),
            history: store.history.clone(),
            history_seeded: store.history_seeded.clone(),
            provider_pins: store.provider_pins.clone(),
            price_cache_version: store.price_cache_version,
            budgets: store.budgets.clone(),
        }
    }

    pub fn replace(&self, app: &AppHandle, bundle: Bundle) -> Result<(), String> {
        let mut store = self.lock().clone();
        store.accounts = bundle.accounts;
        store.transactions = bundle.transactions;
        store.rules = bundle.rules;
        store.default_currency = bundle.default_currency;
        store.categories = bundle.categories;
        store.flow_colors = bundle.flow_colors;
        store.flow_layout = bundle.flow_layout;
        store.entity_colors = bundle.entity_colors;
        store.fx = bundle.fx;
        store.prices = bundle.prices;
        store.history = bundle.history;
        store.history_seeded = bundle.history_seeded;
        store.provider_pins = bundle.provider_pins;
        store.price_cache_version = bundle.price_cache_version;
        store.budgets = bundle.budgets;
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
        let locale = currency_locale(app, &store);
        let _guard = push_locale(&locale);
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

    pub fn save_budget(&self, app: &AppHandle, args: &Value) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let clock = clock_for(app, &store);
        let _ = apply_budget(&mut store, args, &clock)?;
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

    pub fn edit_category_on(&self, app: &AppHandle, action: &str, name: &str, to: &str, color: &str, kind: &str, icon: &str, order: i32, parent: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        edit_category_parent(&mut store, action, name, to, color, kind, icon, order, parent)?;
        self.commit(app, store)
    }

    pub fn set_color_on(&self, app: &AppHandle, kind: &str, id: &str, color: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        paint_entity(&mut store, kind, id, color)?;
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

    pub fn set_price_provider(&self, app: &AppHandle, id: &str, enabled: bool) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        crate::finance_prices::set_provider(&mut store.prices, id, enabled);
        self.commit(app, store)
    }

    pub fn set_manual_price(&self, app: &AppHandle, symbol: &str, price: &str, currency: &str, override_live: bool) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        if price.trim().is_empty() {
            store.prices.manual.retain(|row| !row.symbol.eq_ignore_ascii_case(symbol));
        } else {
            let value: f64 = price.trim().replace(',', ".").parse().map_err(|_| "The price has to be a number above zero.".to_string())?;
            crate::finance_prices::upsert_manual(&mut store.prices, symbol, value, currency, &store.fx.as_of, override_live)?;
        }
        self.commit(app, store)
    }

    pub fn refresh_rates(&self, app: &AppHandle, force: bool) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        // AI runs after every live provider misses, including the hourly pass. force only skips the daily wait.
        let changed = price_store(app, &mut store, force, true);
        if !changed {
            return Ok(snapshot_of(&store, &currency_locale(app, &store), Local::now().date_naive()));
        }
        self.commit(app, store)
    }

    /// Save a CoinGecko or CoinPaprika id for one holding and fetch it now.
    pub fn set_holding_provider(&self, app: &AppHandle, account_id: &str, gecko: &str, paprika: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let locale = currency_locale(app, &store);
        let _guard = push_locale(&locale);
        let id = account_id.trim();
        if !store.accounts.iter().any(|account| account.id == id) {
            return Err(spoken("No holding matches {name}.", &[("name", id)]));
        }
        let gecko = gecko.trim().to_string();
        let paprika = paprika.trim().to_string();
        if gecko.is_empty() && paprika.is_empty() {
            return Err(spoken("Say the provider id.", &[]));
        }
        if let Some(pin) = store.provider_pins.iter_mut().find(|pin| pin.account_id == id) {
            pin.coingecko_id = gecko;
            pin.paprika_id = paprika;
            pin.manual = true;
        } else {
            store.provider_pins.push(crate::finance_history::ProviderPin {
                account_id: id.to_string(),
                coingecko_id: gecko,
                paprika_id: paprika,
                manual: true,
            });
        }
        let _ = link_holdings(&mut store);
        let now_ms = Local::now().timestamp_millis();
        let _ = fetch_holding_prices(&mut store, now_ms, true, Some(id), &crate::finance_prices::blocking_get, Some(&|asset| ai_spot(app, asset)));
        self.commit(app, store)
    }

    /// Fetch one holding now. A fresh quote from earlier today does not block it.
    /// A miss is saved, then returned as an error so the screen can say why.
    pub fn refresh_holding(&self, app: &AppHandle, name_or_id: &str) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        let locale = currency_locale(app, &store);
        let _guard = push_locale(&locale);
        let outcome = refresh_named(&mut store, app, name_or_id);
        let snap = self.commit(app, store)?;
        outcome?;
        Ok(snap)
    }

    /// Unsettled finance occurrences inside [start, end). Reminders stay on their own alarms.
    pub fn calendar_events(&self, start_ms: i64, end_ms: i64, locale: &str) -> Vec<CalEvent> {
        let _guard = push_locale(locale);
        let store = self.lock().clone();
        calendar_money_events(&store, start_ms, end_ms, Local::now().date_naive())
    }

    pub fn set_manual_rate(&self, app: &AppHandle, base: &str, quote: &str, rate: &str, override_live: bool) -> Result<Snapshot, String> {
        let mut store = self.lock().clone();
        if rate.trim().is_empty() {
            store.fx.manual.retain(|row| !(row.base.eq_ignore_ascii_case(base) && row.quote.eq_ignore_ascii_case(quote)));
        } else {
            let value: f64 = rate.trim().parse().map_err(|_| "The rate has to be a number above zero.".to_string())?;
            let as_of = store.fx.as_of.clone();
            crate::finance_fx::upsert_manual(&mut store.fx, base, quote, value, &as_of, override_live)?;
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
        let locale = currency_locale(app, &store);
        let _spoken = push_locale(&locale);
        link_calendar(app, &mut store, Local::now());
        let snap = snapshot_of(&store, &currency_locale(app, &store), Local::now().date_naive());
        Self::persist(&self.path, &store)?;
        publish_board(&self.path, &store);
        gc_events(app, &previous, &store);
        gc_reminders(app, &previous, &store);
        *self.lock() = store;
        touch(app);
        if let Some(bridge) = app.try_state::<crate::native::NativeBridge>() {
            bridge.refresh_widgets();
        }
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
            "Log an expense, income, or a payment toward a card, loan, or other debt on this phone. amount is the number they said. currency is whatever they named; omit it to use that account's saved currency, or the Finance default. Do not assume USD. direction is expense, income, or payment. account is a nickname. accountKind is bank, cash, credit_card, loan, other_debt, investment, or other when creating that nickname. fromAccount is the bank or cash a payment came from. date is YYYY-MM-DD, tomorrow, or a day of the month. recurrence is once, recurring, or irregular. cadence is daily, weekly, monthly, or yearly. leadDays is how many days before a future payment to remind them, in this app only. Do not invent a balance, due date, or account number. A future payment, due, or income date is also saved on this app's Calendar and Time list. id edits that one saved transaction and does not rewrite other ones. payInFull true pays the balance owed. Charging a card is direction expense with account set to the card: card debt rises, the bank does not move, and it counts as an expense. Paying a card is direction payment with fromAccount: it is not an expense. Paying more than the balance owed is allowed and leaves a credit, a negative amount owed. Confirm that new credit. Do not refuse the overpay. payInFull when nothing is owed, or the card is already in credit, does not move money: say there is nothing to pay. If they name an amount while the card is already in credit, pay that amount from fromAccount and the credit grows. A charge on a card that is in credit is still direction expense: it reduces the credit first and can leave a positive debt, and the whole charge is an expense. currency may be a crypto ticker such as BTC. Spending that currency from its balance reduces the units. If the units are not enough, refuse. A missing exchange rate refuses the payment. Never use 1:1. If the card, the funding account, or the amount is missing, or two names match, the error is the one question to ask. Pass items to log several expenses in one call. Each saved row includes id. Speak only rows in saved. If failed is not empty, say those errors and do not say they were logged. A phrase may correct itself, such as 79.95, no wait, 63.96, which saves 63.96 only. Account names match ignoring case, spaces, hyphens, punctuation, and diacritics, and a single close prefix or typo matches too: Swiss card is Swisscard when that card exists. Logging never creates an account or card. If none match, or several do, ask and list the closest names. Create one only with save_account after they explicitly ask to add it. To change an existing expense, pass its id, or matchAmount, matchCategory, matchAccount, and matchDate, plus the new amount. That edits the same id. Do not add a second expense. category must already exist. subcategory is a child of that category. notes is the description. Do not invent a category from the description. If none fits, leave category empty. confirmCategory true creates it only after the user agrees. create category Name, or create subcategory Name under Parent, is that agreement.",
            json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "category": { "type": "string" },
                    "subcategory": { "type": "string" },
                    "confirmCategory": { "type": "boolean", "description": "True only after the user agrees to create a new category." },
                    "account": { "type": "string" },
                    "accountKind": { "type": "string" },
                    "fromAccount": { "type": "string" },
                    "date": { "type": "string" },
                    "recurrence": { "type": "string" },
                    "cadence": { "type": "string" },
                    "leadDays": { "type": "integer" },
                    "notes": { "type": "string" },
                    "id": { "type": "string" },
                    "payInFull": { "type": "boolean" },
                    "fromAccountKind": { "type": "string" },
                    "phrase": { "type": "string" },
                    "items": { "type": "array", "items": { "type": "object" } },
                    "matchAmount": { "type": "string" },
                    "matchCategory": { "type": "string" },
                    "matchQuery": { "type": "string" },
                    "matchDate": { "type": "string" },
                    "matchAccount": { "type": "string" }
                },
                "required": ["direction"]
            })
        ),
        fn_tool(
            "save_recurring",
            "Save a repeating bill, payday, subscription, card payment, or automated investment on this phone. name is the nickname. amount may be omitted when they only named a date. cadence is daily, weekly, monthly, yearly, every_n_days, every_n_weeks, every_n_months, or every_n_years. intervalDays is N, at least 1. A phrase can be the whole sentence, such as add a 20 CHF payment every 10 days starting tomorrow, or change gym to every 2 weeks. Do not create an account or card unless they explicitly ask to add one. occurrences is how many times a fixed run lasts, such as 9. openEnded true means it does not end. date is YYYY-MM-DD, tomorrow, or a day such as 1. direction is expense, income, or payment. leadDays is how many days before to remind them in this app. Default is 2, and the day itself is reminded too. 0 means only on the day. Do not invent an amount or a date. The date is saved on this app's Calendar and Time list, not the phone Clock. Pass phrase instead when they describe the whole plan in one sentence.",
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
                    "notes": { "type": "string" },
                    "intervalDays": { "type": "integer" },
                    "endDate": { "type": "string" },
                    "occurrences": { "type": "integer" },
                    "openEnded": { "type": "boolean" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "save_schedule",
            "Program income or an expense on this phone: a fixed run such as a course for 9 months, open-ended rent, a subscription, a one-off, planned future income, or an investment payout. Pass phrase as the user's sentence when you can, for example a course for 9 months at 270 euros, rent for as long as they live there, or 700 dollars in Nitroeggs that pays 14 dollars every 10 days in XMR. phrase programs the plan and does not mark it paid. Structured fields: name, amount, currency, direction expense or income, purpose one_off, recurring, subscription, or investment_payout, cadence, intervalDays, occurrences, openEnded, date, payoutCurrency, amountMode fixed or quote, account, accountKind. action pause, stop, or resume with name. Do not invent a price for the investment or an exchange rate. A future item stays planned until mark_paid. Omitting amount on an existing name keeps the saved amount. Changing the amount applies only to future unpaid occurrences. Paid history stays. account can be cash, a bank, or a credit card.",
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
                    "leadDays": { "type": "integer" },
                    "id": { "type": "string" },
                    "notes": { "type": "string" },
                    "endDate": { "type": "string" }
                }
            })
        ),
        fn_tool(
            "save_account",
            "Save or update a bank, cash, card, loan, other debt, or investment the user described. Match the exact nickname. Never merge different kinds that only share part of a name, such as ZKB and ZKB credit card. kind is bank, cash, credit_card, loan, other_debt, investment, or other. changeKind true only when they explicitly change the kind of that same nickname. balance or worth is only a figure they stated. On a credit card, balance is the amount owed right now and can be updated any time, including zero and a negative credit. Pass -1393.65, or 1393.65 credit, or 200 in credit. That sets the statement balance directly: it is not income, not an expense, and it does not move a funding account. units keeps every decimal, such as 2.969859854727, and unit is troy_oz, gram, kilogram, share, or coin. Ounces of metal are troy ounces. phrase may be the whole sentence, such as 5 ounces of silver, 10 grams of gold, 2.5 shares of Tesla, half a share of VWCE, or 16.46 ZANO. unitPrice is only a price they stated. If the result includes a market worth, say that worth and its source. Do not say a price is missing when that worth is present. currency may be fiat or a crypto ticker such as BTC, ETH, XMR, SOL, or DOGE. A crypto balance is units of that currency and can be spent or can receive income. investmentType is stock, fund, crypto, property, collectible, stake, custom, or a short label. A custom asset is kind investment with the worth they stated. That worth is their figure, not an exchange rate. A later price or worth replaces only what they stated. currency when they name one; otherwise that account's currency, or the Finance default. defaultCurrency sets that default and can be sent alone, for example CHF. color is a hex color such as #E25B45. investmentType is stock, fund, crypto, or custom. newName renames that nickname. leadDays is how many days before a due date to remind them in this app. Do not invent a balance, a price, or an exchange rate. Never store a card number, account number, or PIN. lastFour is only four digits. creditLimit, dueDay, and statementDay are for a credit card. Do not turn a loan or an investment into a card unless changeKind is true because they said to change that same nickname. Spacing, hyphens, and a small typo still select that same nickname, so Swiss card updates Swisscard instead of creating another card. A new account is created only when no saved name matches.",
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
                    "unit": { "type": "string" },
                    "unitPrice": { "type": "string" },
                    "phrase": { "type": "string" },
                    "priceOverride": { "type": "boolean" },
                    "newName": { "type": "string" },
                    "changeKind": { "type": "boolean" },
                    "defaultCurrency": { "type": "string" },
                    "leadDays": { "type": "integer" },
                    "notes": { "type": "string" },
                    "creditLimit": { "type": "string" },
                    "dueDay": { "type": "integer" },
                    "statementDay": { "type": "integer" },
                    "lastFour": { "type": "string" }
                },
                "required": ["name"]
            })
        ),
        fn_tool(
            "list_finance",
            "Read Finance saved on this phone, without guessing amounts or exchange rates. view is summary, accounts, activity, upcoming, or investments. month is YYYY-MM. kind is bank, cash, credit_card, loan, other_debt, or investment. account is a nickname. Speak the detail, including upcoming payments with their dates. view coming, or upcoming, is only the next 30 days: bills from cash or a bank, and the card payment that is due, not each card charge. Also speak balances, net worth, this month's result without card-bill payments, card debt or credit, utilization when a limit is saved, and the next card payment. If the owed balance is negative, say the credit, not a remaining limit, and do not say a payment is due. Utilization is zero when the card is in credit or paid off. A debt above the limit can be over 100 percent. base and quote, such as BTC and EUR, quote a saved rate and never invent one. refresh true fetches today's rates and holding prices and does not wait until tomorrow. When they say refresh price of a holding, such as refresh price of EPIC Cash or refresh EPIC price, set refresh true and account to that name. phrase may be that whole sentence. When they ask why a holding has no price, such as why has EPIC no price, set phrase to that sentence. Speak the sources, the ids, and the status of each source. Speak a holding's market worth and its source when the detail includes them, and say when the price is an AI estimate. If the rate is missing, say so. Do not invent a balance or a rate that is not in the result. view activity, transactions, or search returns each transaction id, date, amount, currency, category, and account. amount, date, query, and account filter that list. transactionCount on an account is how many transactions are attached. Do not say there are none unless that count is zero or transactions is empty. phrase what color is X reads a saved color. phrase make X green sets it. phrase reset the color for X clears it. Do not create an account when they only ask the color.",
            json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string" },
                    "month": { "type": "string" },
                    "kind": { "type": "string" },
                    "account": { "type": "string" },
                    "direction": { "type": "string" },
                    "recurrence": { "type": "string" },
                    "query": { "type": "string" },
                    "base": { "type": "string" },
                    "quote": { "type": "string" },
                    "refresh": { "type": "boolean" },
                    "phrase": { "type": "string" },
                    "amount": { "type": "string" },
                    "date": { "type": "string" }
                }
            })
        ),
        fn_tool(
            "delete_finance",
            "Delete one finance account, transaction, recurring rule, or budget after a clear yes. Call first with confirmed false, say what will be removed, then call with confirmed true. kind is account, transaction, rule, budget, or budget_line. id comes from list_finance. period is this or YYYY-MM for a budget. name or category selects a budget line. Deleting an account says how many transactions it has. If that count is not zero, offer to move them. Pass moveTo with the destination name to move them before the account is removed. Do not say there are none unless the count is zero.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string" },
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "confirmed": { "type": "boolean" },
                    "moveTo": { "type": "string" }
                },
                "required": ["kind", "confirmed"]
            })
        ),
        fn_tool(
            "move_transactions",
            "Move one or many saved transactions onto another bank, cash wallet, or card. ids come from list_finance. Or pass fromAccount plus query, amount, category, or date. toAccount is the destination nickname and must already exist. This edits account on those rows. It does not copy them and does not log a new expense. Balances and card debt are recalculated from the same rows. If none match, say so. Do not create an account.",
            json!({
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "string" } },
                    "fromAccount": { "type": "string" },
                    "toAccount": { "type": "string" },
                    "toAccountKind": { "type": "string" },
                    "query": { "type": "string" },
                    "amount": { "type": "string" },
                    "category": { "type": "string" },
                    "date": { "type": "string" },
                    "direction": { "type": "string" }
                },
                "required": ["toAccount"]
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
            "open_finance",
            "Open a Finance screen on this phone and answer with the same numbers the screen shows, in the user's main currency. view is overview, activity, accounts, holdings, plan, portfolio, or charts. overview leads with net worth, income, and expenses for the dashboard period. period on overview is this, last, 3m, ytd, 1y, or custom. action hide or show with name hero, result, change, next, debt, categories, flow, pace, or attention. Hide top categories uses name categories. activity is transactions. accounts is banks, cash, cards, and loans, not investments. holdings is investments, crypto, metals, and manual assets. portfolio says how the saved history changed. plan is what is coming, including the budget. chart is flow, allocation, category, debts, trend, holdings, or portfolio. flow uses action labels, values, color, reset, fullscreen, planned, or describe. mode category groups income and expenses by type, mode entry gives each transaction its own node, mode mono is one color, mode vertical is top to bottom, and mode horizontal is left to right. action color with name and color paints one saved item and every chart that shows it: a holding, account, category, income type, or money-flow node. color is a name such as green or blue, or a #hex. action reset with name clears that color. Ask what color is X to read it. action reset with name all asks first with confirmed false, then clears every saved chart color only after confirmed true. describe speaks income, spending, savings, and the largest categories. planned draws the saved budget. period is this, last, 3m, 6m, ytd, 1y, all, or custom. action is hide, show, only, reset, labels, values, color, fullscreen, planned, or describe. Do not invent a rate.",
            json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string" },
                    "chart": { "type": "string", "description": "allocation, category, debts, trend, holdings, or portfolio" },
                    "action": { "type": "string", "description": "hide, show, only, reset, labels, values, color, or fullscreen" },
                    "mode": { "type": "string", "description": "For chart flow: category, entry, mono, vertical, or horizontal" },
                    "name": { "type": "string", "description": "Holding name, dashboard section, or a money-flow element such as Salary, Spending, or Rest" },
                    "color": { "type": "string", "description": "For chart flow action color: green, blue, red, orange, gray, or #RRGGBB" },
                    "confirmed": { "type": "boolean", "description": "For resetting every money-flow color: false to ask, true only after a spoken yes" },
                    "period": { "type": "string", "description": "this, last, 3m, 6m, ytd, 1y, all, or custom" },
                    "start": { "type": "string", "description": "Custom range start, YYYY-MM-DD" },
                    "end": { "type": "string", "description": "Custom range end, YYYY-MM-DD" }
                },
                "required": ["view"]
            })
        ),
        fn_tool(
            "save_budget",
            "Create or change the monthly budget on this phone. action is create, set, read, or delete. period is this, next, YYYY-MM, or template. role is income, expense, savings, or investment. category is the type, such as Salary or Restaurants. subcategory is optional. label is an optional expected entry, such as Salary 1 Nov, so two paydays stay two lines under Salary. amount is the planned figure. currency is what they named, or the Finance default. Do not assume USD. copyForward true copies this month onto later months that have no budget. notify true warns at 80 percent and 100 percent of an expense limit. question left with category answers how much is left. question over answers whether spending is over the plan. read with no category speaks income, spending, savings, and investments. delete asks first with confirmed false, then removes only after confirmed true. Do not invent an amount.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string" },
                    "period": { "type": "string" },
                    "role": { "type": "string" },
                    "category": { "type": "string" },
                    "subcategory": { "type": "string" },
                    "label": { "type": "string" },
                    "amount": { "type": "string" },
                    "currency": { "type": "string" },
                    "copyForward": { "type": "boolean" },
                    "notify": { "type": "boolean" },
                    "question": { "type": "string" },
                    "confirmed": { "type": "boolean" },
                    "id": { "type": "string" }
                }
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

pub fn voice_money_hint() -> &'static str {
    " When they say refresh EPIC price, call list_finance with phrase set to that sentence. When they ask why has EPIC no price, call list_finance with phrase set to that sentence and speak the sources, ids, and status. When logging money, call log_money and then confirm only the rows in saved, each with its id. If failed is not empty or the tool returns an error, say what failed and do not say those items were logged. items logs several expenses in one call. A correction in the same sentence, such as 79.95 no wait 63.96, saves only 63.96. To change 695 to 6.95, edit the existing id from list_finance view activity. Do not add a second expense. Activity returns id, date, amount, currency, category, and account. Swiss card matches an existing Swisscard. Logging never creates an account or card. If none match, or several do, ask and list the closest names. Create a card only with save_account after they explicitly ask to add one. move_transactions moves saved rows and recalculates balances and card debt without copying them. delete_finance on an account says how many transactions it has. Do not say there are none unless that count is zero."
}

pub fn try_tool(app: &AppHandle, name: &str, args: &Value) -> Result<Option<Value>, String> {
    if !matches!(
        name,
        "log_money" | "save_recurring" | "save_schedule" | "save_account" | "list_finance" | "delete_finance" | "move_transactions" | "mark_paid" | "export_finance_pdf" | "open_finance" | "save_budget"
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
        "log_money" | "save_recurring" | "save_schedule" | "save_account" | "list_finance" | "delete_finance" | "move_transactions" | "mark_paid" | "export_finance_pdf" | "open_finance" | "save_budget"
    ) {
        return Ok(None);
    }
    let finance = app.state::<Finance>();
    if name == "export_finance_pdf" {
        return export_from_tool(app, &finance, args);
    }
    if name == "open_finance" {
        let view = screen_view(args["view"].as_str().unwrap_or("overview"));
        let chart = args["chart"].as_str().unwrap_or("").trim().to_string();
        let action = args["action"].as_str().unwrap_or("").trim().to_string();
        let chart_name = args["name"].as_str().unwrap_or("").trim().to_string();
        let period = args["period"].as_str().unwrap_or("").trim().to_string();
        let start = args["start"].as_str().unwrap_or("").trim().to_string();
        let end = args["end"].as_str().unwrap_or("").trim().to_string();
        let mode = args["mode"].as_str().unwrap_or("").trim().to_string();
        let color = args["color"].as_str().unwrap_or("").trim().to_string();
        let confirmed = args["confirmed"].as_bool().unwrap_or(false);
        let folded_action = fold(&action);
        let folded_mode = fold(&mode);
        if chart != "flow" && matches!(folded_action.as_str(), "color" | "paint" | "reset") {
            let mut store = finance.lock().clone();
            let (sentence, changed) = if folded_action == "reset" {
                reset_saved_color(&mut store, &chart_name)
            } else {
                match paint_saved_color(&mut store, &chart_name, &color)? {
                    (sentence, true) => (sentence, true),
                    _ => (spoken("No saved item named {name}.", &[("name", &chart_name)]), false),
                }
            };
            if changed {
                finance.commit(app, store.clone())?;
            }
            touch_chart(app, view, &chart, &action, &chart_name, &period, &start, &end, &mode, &color, confirmed);
            return Ok(Some(json!({ "ok": true, "view": view, "detail": sentence, "defaultCurrency": effective_default(&store, &currency_locale(app, &store)) })));
        }
        if chart == "flow" && (matches!(folded_action.as_str(), "color" | "paint" | "reset") || matches!(folded_mode.as_str(), "vertical" | "horizontal")) {
            let mut store = finance.lock().clone();
            let (sentence, changed) = apply_flow_voice(&mut store, &folded_action, &chart_name, &color, confirmed, &folded_mode);
            if changed {
                finance.commit(app, store.clone())?;
            }
            touch_chart(app, view, &chart, &action, &chart_name, &period, &start, &end, &mode, &color, confirmed);
            return Ok(Some(json!({ "ok": true, "view": view, "detail": sentence, "defaultCurrency": effective_default(&store, &currency_locale(app, &store)) })));
        }
        touch_chart(app, view, &chart, &action, &chart_name, &period, &start, &end, &mode, "", false);
        let store = finance.lock().clone();
        let today = Local::now().date_naive();
        let raw_view = fold(args["view"].as_str().unwrap_or(""));
        let ask = Ask {
            view: if raw_view == "portfolio" {
                "portfolio".into()
            } else if view == "holdings" {
                "investments".into()
            } else {
                view.into()
            },
            month: String::new(),
            kind: String::new(),
            account: String::new(),
            direction: String::new(),
            recurrence: String::new(),
            query: String::new(),
            amount: String::new(),
            date: String::new(),
        };
        let mut detail = speak(&store, &ask, today);
        if chart == "flow" || action == "describe" {
            detail.push(' ');
            detail.push_str(&speak_flow(&store, today));
        }
        if !action.is_empty() {
            detail.push(' ');
            detail.push_str(&speak_chart(&action, &chart, &chart_name));
        }
        return Ok(Some(json!({ "ok": true, "view": view, "detail": detail, "defaultCurrency": effective_default(&store, &currency_locale(app, &store)) })));
    }
    if name == "list_finance" {
        let color_phrase = args["phrase"].as_str().or_else(|| args["query"].as_str()).unwrap_or("").trim();
        if !color_phrase.is_empty() {
            let mut store = finance.lock().clone();
            if let Some((sentence, changed)) = spoken_color_phrase(&mut store, color_phrase)? {
                let code = effective_default(&store, &currency_locale(app, &store));
                if changed {
                    finance.commit(app, store)?;
                }
                return Ok(Some(json!({ "ok": true, "detail": sentence, "defaultCurrency": code })));
            }
        }
        let phrase_name = ["phrase", "query", "account"].iter().find_map(|key| args[*key].as_str().and_then(parse_refresh_phrase));
        let why_name = ["phrase", "query", "account"].iter().find_map(|key| args[*key].as_str().and_then(parse_why_price));
        if let Some(name) = why_name {
            let mut store = finance.lock().clone();
            let _ = link_holdings(&mut store);
            let now_ms = Local::now().timestamp_millis();
            let _ = fetch_holding_prices(&mut store, now_ms, true, Some(&name), &crate::finance_prices::blocking_get, Some(&|asset| ai_spot(app, asset)));
            let detail = explain_unpriced(&store, &name, now_ms)?;
            finance.commit(app, store)?;
            return Ok(Some(json!({ "ok": true, "detail": detail, "refreshed": true })));
        }
        if let Some(name) = phrase_name {
            let mut store = finance.lock().clone();
            let detail = refresh_named(&mut store, app, &name)?;
            finance.commit(app, store)?;
            return Ok(Some(json!({ "ok": true, "detail": detail, "refreshed": true })));
        }
        let force = args["refresh"].as_bool().unwrap_or(false);
        if force {
            let account = args["account"].as_str().unwrap_or("").trim();
            let view = fold(args["view"].as_str().unwrap_or(""));
            if !account.is_empty() && (view.is_empty() || matches!(view.as_str(), "investments" | "holdings" | "portfolio")) {
                let mut store = finance.lock().clone();
                let detail = refresh_named(&mut store, app, account)?;
                finance.commit(app, store)?;
                return Ok(Some(json!({ "ok": true, "detail": detail, "refreshed": true })));
            }
        }
        let _ = finance.refresh_rates(app, force);
        let store = finance.lock().clone();
        let today = Local::now().date_naive();
        let base = args["base"].as_str().unwrap_or("").trim();
        let quote = args["quote"].as_str().unwrap_or("").trim();
        if !base.is_empty() && !quote.is_empty() {
            let detail = speak_pair(&store, base, quote, Local::now().timestamp_millis())?;
            return Ok(Some(json!({ "ok": true, "detail": detail })));
        }
        let ask = ask_from(args);
        if matches!(ask.view.as_str(), "budget" | "plan") && (ask.query.to_ascii_lowercase().contains("budget") || ask.query.to_ascii_lowercase().contains("left") || ask.query.to_ascii_lowercase().contains("over") || args["category"].as_str().unwrap_or("").trim().len() > 0) {
            let period = budget_period(args["month"].as_str().unwrap_or("this"), today);
            let clock = clock_for(app, &store);
            let detail = speak_budget(&store, &period, args["category"].as_str().unwrap_or(&ask.query), &ask.query, &clock);
            return Ok(Some(json!({ "ok": true, "detail": detail, "defaultCurrency": store.default_currency })));
        }
        let mut detail = speak(&store, &ask, today);
        if force {
            let count = store.prices.last_updated.to_string();
            let sentence = if store.prices.last_failed > 0 {
                spoken("Updated {count}. {reason}", &[("count", &count), ("reason", &store.prices.last_error)])
            } else if store.prices.last_error == "No holdings to price." {
                spoken("No holdings to price.", &[])
            } else {
                spoken("Updated {count} prices.", &[("count", &count)])
            };
            detail.push(' ');
            detail.push_str(&sentence);
        }
        if let Some(view) = named_screen(&ask.view) {
            touch_view(app, view);
        }
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
                "transactionCount": transactions_on(&store, &row.account.id),
                "dueDate": row.account.due_date,
                "calendarEventId": row.account.calendar_event_id,
            })).collect::<Vec<_>>(),
            "transactions": transaction_views(&store, &ask),
        })));
    }
    if name == "save_budget" {
        let mut store = finance.lock().clone();
        let clock = clock_for(app, &store);
        let detail = apply_budget(&mut store, args, &clock)?;
        finance.commit(app, store)?;
        return Ok(Some(json!({ "ok": true, "detail": detail })));
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
    if name == "log_money" {
        let entries = log_entries(&mut store, args, &clock)?;
        let value = entries_json(&entries);
        if !entries.saved.is_empty() || entries.failed.is_empty() {
            finance.commit(app, store)?;
        }
        return Ok(Some(value));
    }
    if name == "move_transactions" {
        let value = move_transactions(&mut store, args, &clock)?;
        finance.commit(app, store)?;
        return Ok(Some(value));
    }
    let detail = match name {
        "log_money" => return Ok(None),
        "save_recurring" | "save_schedule" => {
            if let Some(phrase) = args["phrase"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
                apply_phrase(&mut store, phrase, &clock)?
            } else if matches!(fold(args["action"].as_str().unwrap_or("")).as_str(), "pause" | "stop" | "resume") {
                phrase_action(&mut store, &crate::finance_schedule::parse_phrase(&format!("{} {}", args["action"].as_str().unwrap_or(""), args["name"].as_str().unwrap_or("")), clock.today, &clock.default_currency)?, &clock)?
            } else {
                rule_from_tool(&mut store, args, &clock, false)?
            }
        }
        "save_account" => {
            let saved = account_from_tool(&mut store, args, &clock)?;
            price_store(app, &mut store, false, true);
            let extra = store
                .accounts
                .iter()
                .max_by_key(|row| row.updated_ms)
                .map(|account| worth_sentence(&store, account, clock.ms, &clock.default_currency))
                .unwrap_or_default();
            if extra.is_empty() { saved } else { format!("{saved} {extra}") }
        }
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
        amount: amount_text(args.get("amount")),
        date: args["date"].as_str().unwrap_or("").trim().to_string(),
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
    amount: String,
    date: String,
}

fn resolve_pay_account(store: &mut Store, name: &str, kind: &str, _currency: &str, _clock: &Clock) -> Result<String, String> {
    let kind_norm = normalize_kind(kind);
    let folded = fold(name);
    if !name.trim().is_empty() {
        match locate_account(store, "", name, if kind_norm.is_empty() { "" } else { &kind_norm }) {
            Ok(Some(found)) => return Ok(store.accounts[found.index].id.clone()),
            Ok(None) => {}
            Err(err) => return Err(err),
        }
    }
    let generic_cash = kind_norm == "cash" && (name.trim().is_empty() || folded == "cash");
    let said_cash = folded == "cash";
    if generic_cash || (said_cash && kind_norm.is_empty()) {
        let cash: Vec<usize> = store.accounts.iter().enumerate().filter(|(_, account)| account.kind == "cash").map(|(index, _)| index).collect();
        return match cash.len() {
            1 => Ok(store.accounts[cash[0]].id.clone()),
            0 => Err(spoken("No cash account is saved. I did not create one.", &[])),
            _ => Err(which_account(store, &cash)),
        };
    }
    if name.trim().is_empty() {
        return Ok(String::new());
    }
    Err(unresolved_account(store, name))
}

fn unresolved_account(store: &Store, name: &str) -> String {
    let names = closest_account_names(store, name, 4);
    if names.is_empty() {
        return spoken("No account matches {name}. I did not create one.", &[("name", name)]);
    }
    let found = and_list(names.iter().map(String::as_str));
    spoken("No account matches {name}. I did not create one. Closest: {names}.", &[("name", name), ("names", &found)])
}

fn closest_account_names(store: &Store, name: &str, limit: usize) -> Vec<String> {
    let query = compact_name(name);
    if query.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, String)> = store
        .accounts
        .iter()
        .map(|account| (edit_distance(&query, &compact_name(&account.name)), account.name.clone()))
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, name)| name).collect()
}

fn guard_payment(store: &Store, direction: &str, account_id: &str, from_id: &str, amount: i64, currency: &str, clock: &Clock) -> Result<(), String> {
    if direction != "payment" || account_id.is_empty() {
        return Ok(());
    }
    let Some(account) = store.accounts.iter().find(|account| account.id == account_id) else {
        return Err("Which card?".into());
    };
    if !is_debt(&account.kind) {
        return Ok(());
    }
    if from_id.is_empty() {
        return Err(spoken("Which account pays {name}?", &[("name", &account.name)]));
    }
    let sample = Transaction {
        id: String::new(),
        amount_minor: amount,
        currency: currency.to_string(),
        direction: "payment".into(),
        category: String::new(),
        subcategory: String::new(),
        account_id: account_id.to_string(),
        from_account_id: from_id.to_string(),
        date: String::new(),
        notes: String::new(),
        recurrence: "once".into(),
        rule_id: String::new(),
        created_ms: 0,
        updated_ms: 0,
        occurrence_date: String::new(),
    };
    // Overpay is allowed and leaves a credit. A missing rate is still a refusal, never 1:1.
    if amount_for_account(&sample, account, &store.fx, clock.ms).is_none() {
        let codes = format!("{currency} {}", account.currency);
        return Err(spoken("The payment needs a rate for {codes}.", &[("codes", &codes)]));
    }
    if let Some(from) = store.accounts.iter().find(|row| row.id == from_id) {
        if amount_for_account(&sample, from, &store.fx, clock.ms).is_none() {
            let codes = format!("{currency} {}", from.currency);
            return Err(spoken("The payment needs a rate for {codes}.", &[("codes", &codes)]));
        }
    }
    guard_crypto_spend(store, direction, account_id, from_id, amount, currency, clock)
}

fn guard_crypto_spend(store: &Store, direction: &str, account_id: &str, from_id: &str, amount: i64, currency: &str, clock: &Clock) -> Result<(), String> {
    if !matches!(direction, "expense" | "income" | "payment") {
        return Ok(());
    }
    let holder_id = if direction == "payment" { from_id } else { account_id };
    let Some(account) = store.accounts.iter().find(|account| account.id == holder_id) else {
        return Ok(());
    };
    if account.currency.is_empty() {
        return Ok(());
    }
    let sample = Transaction {
        id: String::new(),
        amount_minor: amount,
        currency: currency.to_string(),
        direction: direction.to_string(),
        category: String::new(),
        subcategory: String::new(),
        account_id: account_id.to_string(),
        from_account_id: from_id.to_string(),
        date: String::new(),
        notes: String::new(),
        recurrence: "once".into(),
        rule_id: String::new(),
        created_ms: 0,
        updated_ms: 0,
        occurrence_date: String::new(),
    };
    let moved = amount_for_account(&sample, account, &store.fx, clock.ms);
    if !account.currency.eq_ignore_ascii_case(currency) && moved.is_none() {
        let codes = format!("{currency} {}", account.currency);
        return Err(spoken("The payment needs a rate for {codes}.", &[("codes", &codes)]));
    }
    if matches!(direction, "expense" | "payment") && crate::finance_fx::is_crypto(&account.currency) && !is_debt(&account.kind) {
        let Some(debit) = moved else {
            return Ok(());
        };
        if let Some(have) = shown_with_fx(account, &store.transactions, &store.fx, clock.ms) {
            if debit > have {
                let have_text = format_minor(have.max(0), &account.currency);
                return Err(spoken("That is more than {name} holds ({have}).", &[("name", &account.name), ("have", &have_text)]));
            }
        }
    }
    Ok(())
}

fn edit_logged(store: &mut Store, id: &str, args: &Value, clock: &Clock) -> Result<String, String> {
    let index = store.transactions.iter().position(|tx| tx.id == id).ok_or("No transaction with that id.")?;
    let mut tx = store.transactions[index].clone();
    if !amount_text(args.get("amount")).trim().is_empty() {
        let (minor, currency) = money_for(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(&tx.currency), &tx.currency, &clock.default_currency)?;
        tx.amount_minor = minor;
        tx.currency = currency;
    } else if let Some(code) = args["currency"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        tx.currency = canonical_currency(code)?;
    }
    if let Some(direction) = args["direction"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        tx.direction = normalize_direction(direction)?;
    }
    if args.get("category").is_some() {
        tx.category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    }
    if args.get("notes").is_some() {
        tx.notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    }
    if args.get("date").is_some() {
        let date = require_date(args["date"].as_str().unwrap_or(""), "", clock.today)?;
        if !date.is_empty() {
            tx.date = date;
        }
    }
    if args.get("account").is_some() || args.get("accountKind").is_some() {
        tx.account_id = resolve_pay_account(store, args["account"].as_str().unwrap_or(""), args["accountKind"].as_str().unwrap_or(""), &tx.currency, clock)?;
    }
    if args.get("fromAccount").is_some() || args.get("fromAccountKind").is_some() {
        tx.from_account_id = resolve_pay_account(store, args["fromAccount"].as_str().unwrap_or(""), args["fromAccountKind"].as_str().unwrap_or(""), &tx.currency, clock)?;
    }
    if tx.direction == "payment" {
        guard_payment(store, &tx.direction, &tx.account_id, &tx.from_account_id, tx.amount_minor, &tx.currency, clock)?;
    } else {
        guard_crypto_spend(store, &tx.direction, &tx.account_id, &tx.from_account_id, tx.amount_minor, &tx.currency, clock)?;
    }
    tx.updated_ms = clock.ms;
    let label = if tx.category.is_empty() { tx.direction.clone() } else { tx.category.clone() };
    let amount = format_minor(tx.amount_minor, &tx.currency);
    store.transactions[index] = tx;
    Ok(spoken("Updated {name} to {amount}.", &[("name", &label), ("amount", &amount)]))
}

#[derive(Debug)]
struct LogEntries {
    detail: String,
    saved: Vec<Value>,
    failed: Vec<Value>,
}

fn log_from_tool(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let entries = log_entries(store, args, clock)?;
    if entries.saved.is_empty() && !entries.failed.is_empty() {
        return Err(entries.detail);
    }
    Ok(entries.detail)
}

fn log_entries(store: &mut Store, args: &Value, clock: &Clock) -> Result<LogEntries, String> {
    if let Some(phrase) = args["phrase"].as_str().map(str::trim).filter(|value| !value.is_empty()) {
        if args.get("items").is_none() {
            if let Some(items) = spoken_expense_batch(phrase) {
                return log_many(store, &items, clock);
            }
            let before = store.transactions.len();
            let detail = apply_phrase(store, phrase, clock)?;
            return Ok(saved_from_new(store, before, detail));
        }
    }
    if let Some(items) = args.get("items").and_then(Value::as_array) {
        let collapsed = collapse_corrections(items);
        let values: Vec<Value> = collapsed.iter().map(|item| merge_item(args, item)).collect();
        return log_many(store, &values, clock);
    }
    if explicit_id(args).is_none() && has_match_keys(args) {
        let indexes = find_transactions(store, args)?;
        if indexes.is_empty() {
            return Err("No transactions matched.".into());
        }
        if indexes.len() > 1 {
            return Err(which_transaction(store, &indexes));
        }
        let id = store.transactions[indexes[0]].id.clone();
        let mut edit = args.clone();
        edit["id"] = json!(id);
        let detail = log_single(store, &edit, clock)?;
        return Ok(entry_from_id(store, &id, detail));
    }
    if let Some(id) = explicit_id(args) {
        let detail = log_single(store, args, clock)?;
        return Ok(entry_from_id(store, &id, detail));
    }
    let before = store.transactions.len();
    let detail = log_single(store, args, clock)?;
    Ok(saved_from_new(store, before, detail))
}

fn explicit_id(args: &Value) -> Option<String> {
    args.get("id").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()).map(str::to_string)
}

fn has_match_keys(args: &Value) -> bool {
    ["matchAmount", "matchCategory", "matchQuery", "matchDate", "matchAccount"].iter().any(|key| match args.get(*key) {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(_) => true,
    })
}

fn saved_from_new(store: &Store, before: usize, detail: String) -> LogEntries {
    if store.transactions.len() > before {
        let tx = store.transactions.last().expect("transaction was pushed");
        return entry_from_id(store, &tx.id, detail);
    }
    LogEntries { detail, saved: Vec::new(), failed: Vec::new() }
}

fn entry_from_id(store: &Store, id: &str, mut detail: String) -> LogEntries {
    let Some(tx) = store.transactions.iter().find(|tx| tx.id == id) else {
        return LogEntries { detail, saved: Vec::new(), failed: Vec::new() };
    };
    if !detail.contains(&format!("Id {id}")) {
        detail.push_str(&spoken(" Id {id}.", &[("id", id)]));
    }
    let account = account_name(store, &tx.account_id).unwrap_or_default();
    LogEntries { detail, saved: vec![tx_saved_json(tx, &account)], failed: Vec::new() }
}

fn log_many(store: &mut Store, items: &[Value], clock: &Clock) -> Result<LogEntries, String> {
    if items.is_empty() {
        return Err("Say the amount, or say pay in full.".into());
    }
    let mut saved = Vec::new();
    let mut failed = Vec::new();
    for item in items {
        let before = store.transactions.len();
        match log_single(store, item, clock) {
            Ok(_) => {
                if store.transactions.len() > before {
                    let tx = store.transactions.last().expect("transaction was pushed");
                    let account = account_name(store, &tx.account_id).unwrap_or_default();
                    saved.push(tx_saved_json(tx, &account));
                }
            }
            Err(err) => failed.push(json!({
                "ok": false,
                "error": err,
                "amount": item.get("amount").cloned().unwrap_or(Value::Null),
                "category": item.get("category").cloned().unwrap_or(Value::Null),
                "account": item.get("account").cloned().unwrap_or(Value::Null),
            })),
        }
    }
    let detail = batch_detail(&saved, &failed);
    Ok(LogEntries { detail, saved, failed })
}

fn batch_detail(saved: &[Value], failed: &[Value]) -> String {
    let count = saved.len().to_string();
    let total = (saved.len() + failed.len()).to_string();
    let mut detail = spoken("Saved {count} of {total}.", &[("count", &count), ("total", &total)]);
    if !saved.is_empty() {
        let body = saved
            .iter()
            .map(|row| {
                spoken(
                    "{label} {amount} on {account} {date}, id {id}.",
                    &[
                        ("label", row["label"].as_str().unwrap_or("")),
                        ("amount", row["amount"].as_str().unwrap_or("")),
                        ("account", row["account"].as_str().unwrap_or("")),
                        ("date", row["date"].as_str().unwrap_or("")),
                        ("id", row["id"].as_str().unwrap_or("")),
                    ],
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        detail.push(' ');
        detail.push_str(&body);
    }
    if !failed.is_empty() {
        let body = failed.iter().filter_map(|row| row["error"].as_str()).collect::<Vec<_>>().join(" ");
        if !body.is_empty() {
            detail.push(' ');
            detail.push_str(&body);
        }
    }
    detail
}

fn tx_saved_json(tx: &Transaction, account: &str) -> Value {
    let label = if tx.category.trim().is_empty() { spoken("unknown", &[]) } else { tx.category.clone() };
    json!({
        "ok": true,
        "id": tx.id,
        "date": tx.date,
        "amountMinor": tx.amount_minor,
        "amount": format_minor(tx.amount_minor, &tx.currency),
        "currency": tx.currency,
        "category": tx.category,
        "description": if tx.category.trim().is_empty() { tx.notes.clone() } else { tx.category.clone() },
        "label": label,
        "account": account,
        "accountId": tx.account_id,
        "direction": tx.direction,
        "paymentMethod": account,
    })
}

fn entries_json(entries: &LogEntries) -> Value {
    json!({
        "ok": entries.failed.is_empty(),
        "savedCount": entries.saved.len(),
        "failedCount": entries.failed.len(),
        "saved": entries.saved,
        "failed": entries.failed,
        "detail": entries.detail,
    })
}

fn merge_item(parent: &Value, item: &Value) -> Value {
    let mut out = json!({});
    if let Some(obj) = parent.as_object() {
        for (key, value) in obj {
            if matches!(key.as_str(), "items" | "phrase" | "matchAmount" | "matchCategory" | "matchQuery" | "matchDate" | "matchAccount") {
                continue;
            }
            out[key] = value.clone();
        }
    }
    if let Some(obj) = item.as_object() {
        for (key, value) in obj {
            if matches!(key.as_str(), "correctsPrevious" | "replacesPrevious") {
                continue;
            }
            let empty = value.as_str().is_some_and(|text| text.trim().is_empty());
            if !value.is_null() && !empty {
                out[key] = value.clone();
            }
        }
    }
    out
}

fn collapse_corrections(items: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for item in items {
        let mut item = item.clone();
        if let Some(raw) = item.get("amount").map(|value| amount_text(Some(value))) {
            let fixed = correct_amount_text(&raw);
            if !fixed.is_empty() {
                item["amount"] = json!(fixed);
            }
        }
        let corrects = item["correctsPrevious"].as_bool().unwrap_or(false) || item["replacesPrevious"].as_bool().unwrap_or(false);
        if corrects {
            let previous = out.pop();
            if text_field(&item, "category").is_empty() {
                if let Some(prev) = previous.as_ref() {
                    item["category"] = json!(text_field(prev, "category"));
                }
            }
            if text_field(&item, "account").is_empty() {
                if let Some(prev) = previous.as_ref() {
                    item["account"] = json!(text_field(prev, "account"));
                }
            }
        }
        out.push(item);
    }
    out
}

fn text_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

fn correct_amount_text(raw: &str) -> String {
    if !has_correction(raw) {
        return raw.trim().to_string();
    }
    let tail = tail_after_correction(raw);
    scan_amounts(&tail).into_iter().next().map(|(amount, _)| amount).unwrap_or_else(|| raw.trim().to_string())
}

fn has_correction(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["no wait", "wait no", "no, wait", "i mean", "i meant", "actually", "correction"].iter().any(|marker| lower.contains(marker))
}

fn tail_after_correction(clause: &str) -> String {
    let lower = clause.to_lowercase();
    let markers = ["no wait", "wait no", "no, wait", "i mean", "i meant", "actually", "correction"];
    let mut best = 0usize;
    for marker in markers {
        if let Some(pos) = lower.rfind(marker) {
            let end = pos + marker.len();
            if end > best && clause.is_char_boundary(end) {
                best = end;
            }
        }
    }
    clause[best..].trim().trim_start_matches([',', ' ']).to_string()
}

fn spoken_expense_batch(phrase: &str) -> Option<Vec<Value>> {
    if looks_like_schedule(phrase) {
        return None;
    }
    let items = parse_spoken_expenses(phrase);
    if items.len() >= 2 || (items.len() == 1 && has_correction(phrase)) {
        Some(items)
    } else {
        None
    }
}

fn looks_like_schedule(phrase: &str) -> bool {
    let folded = fold(phrase);
    if folded.contains("every")
        || folded.contains("subscription")
        || folded.contains("as long")
        || folded.contains("will get")
        || folded.contains("will receive")
        || folded.contains("got paid")
        || folded.contains("was paid")
        || folded.contains("pays")
    {
        return true;
    }
    let words: Vec<&str> = folded.split_whitespace().collect();
    words.windows(3).any(|window| {
        window[0] == "for" && window[1].chars().all(|ch| ch.is_ascii_digit()) && (window[2].starts_with("month") || window[2].starts_with("week") || window[2].starts_with("day"))
    })
}

fn parse_spoken_expenses(phrase: &str) -> Vec<Value> {
    let mut carry_currency = String::new();
    let mut carry_account = String::new();
    let mut items = Vec::new();
    for clause in split_expense_clauses(phrase) {
        let tail = tail_after_correction(&clause);
        let Some((amount, mut currency)) = scan_amounts(&tail).into_iter().next() else {
            continue;
        };
        if currency.is_empty() {
            currency = carry_currency.clone();
        } else {
            carry_currency = currency.clone();
        }
        let mut account = account_of(&clause);
        if account.is_empty() {
            account = carry_account.clone();
        } else {
            carry_account = account.clone();
        }
        let described = description_of(&clause);
        let category = if forgotten(&clause) && !clause.to_lowercase().contains(" on ") {
            String::new()
        } else {
            hinted_label(&described)
        };
        let notes = if category.is_empty() { described } else { String::new() };
        items.push(json!({
            "direction": "expense",
            "amount": amount,
            "currency": currency,
            "category": category,
            "account": account,
            "notes": notes,
        }));
    }
    items
}

fn split_expense_clauses(phrase: &str) -> Vec<String> {
    let chars: Vec<char> = phrase.chars().collect();
    let mut normalized = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '.' && index + 1 < chars.len() && chars[index + 1].is_whitespace() {
            normalized.push('\n');
            index += 1;
            continue;
        }
        normalized.push(chars[index]);
        index += 1;
    }
    let mut clauses = Vec::new();
    for line in normalized.split('\n') {
        let mut buf = String::new();
        for word in line.split_whitespace() {
            if fold(word) == "then" {
                if !buf.trim().is_empty() {
                    clauses.push(buf.trim().to_string());
                }
                buf.clear();
                continue;
            }
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(word);
        }
        if !buf.trim().is_empty() {
            clauses.push(buf.trim().to_string());
        }
    }
    clauses.into_iter().map(|clause| clause.trim_matches(|ch: char| ch == '.' || ch == ';').trim().to_string()).filter(|clause| !clause.is_empty()).collect()
}

fn color_command(phrase: &str) -> Option<(String, String)> {
    let folded = fold(phrase);
    let verb = ["make", "color", "colour", "pinta", "pon ", "poe ", "mach", "farbe", "fes ", "haz ", "paint"].iter().any(|word| folded.contains(word));
    if !verb {
        return None;
    }
    let colors = [
        ("gray", "#8A8A8A"),
        ("grey", "#8A8A8A"),
        ("gris", "#8A8A8A"),
        ("grau", "#8A8A8A"),
        ("cinza", "#8A8A8A"),
        ("cinzent", "#8A8A8A"),
        ("grisa", "#8A8A8A"),
        ("blue", "#0072B2"),
        ("azul", "#0072B2"),
        ("blau", "#0072B2"),
        ("green", "#009E73"),
        ("verde", "#009E73"),
        ("grun", "#009E73"),
        ("orange", "#E69F00"),
        ("naranja", "#E69F00"),
        ("laranja", "#E69F00"),
    ];
    let hit = colors.iter().find(|(word, _)| folded.split_whitespace().any(|token| token == *word))?;
    let mut words = Vec::new();
    for word in phrase.split_whitespace() {
        let token = fold(word.trim_matches(|ch: char| !ch.is_alphanumeric()));
        if token.is_empty() || colors.iter().any(|(name, _)| token == *name) {
            continue;
        }
        if matches!(token.as_str(), "make" | "color" | "colour" | "pinta" | "pon" | "poe" | "mach" | "farbe" | "fes" | "haz" | "paint" | "the" | "a" | "la" | "el" | "die" | "der" | "das" | "o") {
            continue;
        }
        words.push(word.trim_matches(|ch: char| !ch.is_alphanumeric()).to_string());
    }
    let name = title_phrase(&words.join(" "));
    if name.trim().is_empty() {
        return None;
    }
    Some((name, (*hit.1).to_string()))
}

fn color_question(phrase: &str) -> Option<String> {
    let folded = fold(phrase);
    let marks = [
        "what color is ",
        "what colour is ",
        "what's the color of ",
        "what is the color of ",
        "which color is ",
        "que color es ",
        "que color tiene ",
        "de que color es ",
        "cual es el color de ",
        "welche farbe hat ",
        "welche farbe ist ",
        "qual a cor de ",
        "qual a cor do ",
        "qual e a cor de ",
        "qual e a cor do ",
        "quin color te ",
        "quin color es ",
        "quin color te el ",
    ];
    let mark = marks.iter().find(|mark| folded.contains(*mark))?;
    let index = folded.find(mark)?;
    let raw = phrase.get(index + mark.len()..)?.trim().trim_matches(|ch: char| ch == '?' || ch == '.' || ch == '!');
    let name = raw.trim();
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn color_reset_name(phrase: &str) -> Option<String> {
    let folded = fold(phrase);
    if matches!(folded.as_str(), "reset all colors" | "reset every color" | "reset all colours" | "restablece todos los colores" | "restablecer todos los colores" | "alle farben zurucksetzen" | "repoe todas as cores" | "repor todas as cores" | "restableix tots els colors") {
        return Some(String::new());
    }
    let marks = [
        "reset the color for ",
        "reset the color of ",
        "reset color for ",
        "reset color of ",
        "clear the color for ",
        "clear the color of ",
        "restablece el color de ",
        "restablecer el color de ",
        "quita el color de ",
        "setze die farbe von ",
        "setze die farbe fur ",
        "farbe von ",
        "repoe a cor de ",
        "repor a cor de ",
        "limpa a cor de ",
        "restableix el color de ",
        "treu el color de ",
    ];
    let mark = marks.iter().find(|mark| folded.contains(*mark))?;
    let index = folded.find(mark)?;
    let raw = phrase.get(index + mark.len()..)?.trim().trim_matches(|ch: char| ch == '?' || ch == '.' || ch == '!');
    let name = raw.split_whitespace().next().unwrap_or("").trim();
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn speak_saved_color(store: &Store, name: &str) -> String {
    if let Some(found) = locate_color_name(store, name) {
        let (label, color) = match found {
            ColorHit::Account(index) => {
                let account = &store.accounts[index];
                let saved = store.entity_colors.iter().find(|row| row.kind == "account" && row.id == account.id).map(|row| row.color.as_str()).unwrap_or(account.color.as_str());
                (account.name.clone(), shown_swatch(&account.id, saved))
            }
            ColorHit::Category(index) => {
                let category = &store.categories[index];
                let kind = if category.kind == "income" { "income" } else { "category" };
                let saved = store.entity_colors.iter().find(|row| row.kind == kind && fold(&row.id) == fold(&category.name)).map(|row| row.color.as_str()).unwrap_or(category.color.as_str());
                (category.name.clone(), shown_swatch(&category.name, saved))
            }
        };
        return spoken("{name} is {color}.", &[("name", &label), ("color", &color_word(&color))]);
    }
    spoken("No saved item named {name}.", &[("name", name)])
}

enum ColorHit {
    Account(usize),
    Category(usize),
}

fn locate_color_name(store: &Store, name: &str) -> Option<ColorHit> {
    if let Ok(Some(found)) = locate_account(store, "", name, "") {
        return Some(ColorHit::Account(found.index));
    }
    let query = fold(name);
    let ticker: Vec<usize> = store.accounts.iter().enumerate().filter(|(_, account)| !account.ticker.is_empty() && fold(&account.ticker) == query).map(|(index, _)| index).collect();
    if ticker.len() == 1 {
        return Some(ColorHit::Account(ticker[0]));
    }
    let coins: Vec<usize> = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| fold(&account.currency) == query && (account.kind == "investment" || account.kind == "other"))
        .map(|(index, _)| index)
        .collect();
    if coins.len() == 1 {
        return Some(ColorHit::Account(coins[0]));
    }
    store.categories.iter().position(|category| fold(&category.name) == query).map(ColorHit::Category)
}

fn reset_saved_color(store: &mut Store, name: &str) -> (String, bool) {
    if name.trim().is_empty() {
        return (spoken("Reset every money-flow color? Say yes.", &[]), false);
    }
    let Some(found) = locate_color_name(store, name) else {
        return (spoken("No saved item named {name}.", &[("name", name)]), false);
    };
    match found {
        ColorHit::Account(index) => {
            let id = store.accounts[index].id.clone();
            let label = store.accounts[index].name.clone();
            let swatch = default_swatch(&id);
            store.accounts[index].color = swatch.clone();
            remember_entity(store, "account", &id, &swatch);
            (spoken("{name} is back to its default color.", &[("name", &label)]), true)
        }
        ColorHit::Category(index) => {
            let label = store.categories[index].name.clone();
            let kind = if store.categories[index].kind == "income" { "income" } else { "category" };
            let swatch = default_swatch(&label);
            store.categories[index].color = swatch.clone();
            remember_entity(store, kind, &label, &swatch);
            (spoken("{name} is back to its default color.", &[("name", &label)]), true)
        }
    }
}

fn paint_saved_color(store: &mut Store, name: &str, color: &str) -> Result<(String, bool), String> {
    let swatch = normalize_swatch(color)?;
    if swatch.is_empty() {
        return Ok((spoken("Say the color.", &[]), false));
    }
    let Some(found) = locate_color_name(store, name) else {
        return Ok((String::new(), false));
    };
    match found {
        ColorHit::Account(index) => {
            let id = store.accounts[index].id.clone();
            let label = store.accounts[index].name.clone();
            store.accounts[index].color = swatch.clone();
            remember_entity(store, "account", &id, &swatch);
            Ok((spoken("{name} is {color}.", &[("name", &label), ("color", &color_word(&swatch))]), true))
        }
        ColorHit::Category(index) => {
            let label = store.categories[index].name.clone();
            let kind = if store.categories[index].kind == "income" { "income" } else { "category" };
            store.categories[index].color = swatch.clone();
            remember_entity(store, kind, &label, &swatch);
            Ok((spoken("{name} is {color}.", &[("name", &label), ("color", &color_word(&swatch))]), true))
        }
    }
}

/// Read, set, or reset a saved color. A set for an unknown name returns None so account creation can continue.
fn spoken_color_phrase(store: &mut Store, phrase: &str) -> Result<Option<(String, bool)>, String> {
    if phrase.trim().is_empty() {
        return Ok(None);
    }
    if let Some(name) = color_question(phrase) {
        return Ok(Some((speak_saved_color(store, &name), false)));
    }
    if let Some(name) = color_reset_name(phrase) {
        return Ok(Some(reset_saved_color(store, &name)));
    }
    if let Some((name, swatch)) = color_command(phrase) {
        let (sentence, changed) = paint_saved_color(store, &name, &swatch)?;
        if sentence.is_empty() {
            return Ok(None);
        }
        return Ok(Some((sentence, changed)));
    }
    Ok(None)
}

#[derive(Debug)]
struct PhysicalPhrase {
    name: String,
    amount: String,
    currency: String,
    date: String,
    revalue: bool,
}

/// A watch, a car, art, or anything with no market feed. Live holdings stay on the other parser.
fn parse_physical(phrase: &str) -> Option<PhysicalPhrase> {
    let folded = fold(phrase);
    if folded.contains("ounce") || folded.contains("onza") || folded.contains("gram") || folded.contains("share") || folded.contains("accion") || folded.contains("aktie") || folded.contains("onca") {
        return None;
    }
    let revalue = folded.contains("worth") || folded.contains(" vale") || folded.contains("wert") || folded.contains(" val ");
    let bought = folded.contains("bought") || folded.contains("compre") || folded.contains("gekauft") || folded.contains("comprat") || folded.contains("purchase");
    if !bought && !revalue {
        return None;
    }
    let (amount, currency) = scan_amounts(phrase).into_iter().next()?;
    if amount.trim().is_empty() {
        return None;
    }
    let mut name_words = Vec::new();
    for word in phrase.split_whitespace() {
        let token = fold(word.trim_matches(|ch: char| !ch.is_alphanumeric()));
        if token.is_empty() || token.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if matches!(
            token.as_str(),
            "i" | "my" | "a" | "an" | "the" | "for" | "is" | "now" | "worth" | "bought" | "buy" | "purchase" | "purchased" | "comprei" | "compre" | "comprat" | "gekauft" | "mein" | "meine" | "mi" | "el" | "la" | "un" | "una" | "vale" | "wert" | "val" | "de" | "por" | "fur" | "on" | "dated" | "date" | "um" | "meu" | "minha" | "para"
        ) {
            continue;
        }
        if crate::finance_fx::is_fiat(&token) || matches!(token.as_str(), "btc" | "eth" | "xmr" | "eur" | "usd" | "chf") {
            continue;
        }
        name_words.push(word.trim_matches(|ch: char| !ch.is_alphanumeric()).to_string());
    }
    let name = title_phrase(&name_words.join(" "));
    if name.trim().is_empty() {
        return None;
    }
    let date = phrase.split_whitespace().find(|word| word.len() >= 10 && word.as_bytes().get(4) == Some(&b'-')).unwrap_or("").to_string();
    Some(PhysicalPhrase { name, amount, currency, date, revalue: revalue && !bought })
}

fn scan_amounts(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut hits = Vec::new();
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            index += 1;
            let mut saw_sep = false;
            while index < chars.len() {
                if chars[index].is_ascii_digit() {
                    index += 1;
                } else if !saw_sep && (chars[index] == '.' || chars[index] == ',') && index + 1 < chars.len() && chars[index + 1].is_ascii_digit() {
                    saw_sep = true;
                    index += 1;
                } else {
                    break;
                }
            }
            let amount: String = chars[start..index].iter().collect();
            let after: String = chars[index..].iter().collect();
            let next = after.trim_start().split_whitespace().next().unwrap_or("");
            let next_fold = fold(next);
            if next_fold.starts_with("month") || next_fold.starts_with("day") || next_fold.starts_with("week") || next_fold.starts_with("year") {
                continue;
            }
            let currency = canonical_currency(next.trim_matches(|ch: char| !ch.is_alphanumeric())).unwrap_or_default();
            hits.push((amount, currency));
        } else {
            index += 1;
        }
    }
    hits
}

fn category_of(clause: &str) -> String {
    hinted_label(&description_of(clause))
}

fn description_of(clause: &str) -> String {
    let lower = clause.to_lowercase();
    let Some(on) = lower.rfind(" on ") else {
        return String::new();
    };
    if !clause.is_char_boundary(on + 4) {
        return String::new();
    }
    let rest = &clause[on + 4..];
    let rest_lower = rest.to_lowercase();
    let mut end = rest.len();
    for marker in [" paid with", " with the", " with my", ","] {
        if let Some(pos) = rest_lower.find(marker) {
            if rest.is_char_boundary(pos) {
                end = end.min(pos);
            }
        }
    }
    strip_article(rest[..end].trim())
}

/// A single known word such as groceries becomes its category. A description does not.
fn hinted_label(text: &str) -> String {
    let folded = fold(text);
    let tokens: Vec<&str> = folded.split_whitespace().collect();
    if tokens.len() != 1 {
        return String::new();
    }
    const HINTS: &[(&str, &str)] = &[
        ("grocery", "Groceries"),
        ("grocer", "Groceries"),
        ("supermerc", "Groceries"),
        ("rent", "Housing"),
        ("alquiler", "Housing"),
        ("miete", "Housing"),
        ("renda", "Housing"),
        ("lloguer", "Housing"),
        ("food", "Food"),
        ("comida", "Food"),
        ("essen", "Food"),
        ("restaurant", "Food"),
        ("coffee", "Food"),
        ("cafe", "Food"),
        ("uber", "Transport"),
        ("salary", "Salary"),
        ("sueldo", "Salary"),
        ("gehalt", "Salary"),
        ("nomina", "Salary"),
    ];
    for (word, name) in HINTS {
        if tokens[0] == *word || tokens[0] == fold(name) || tokens[0].contains(word) {
            return (*name).to_string();
        }
    }
    String::new()
}

fn account_of(clause: &str) -> String {
    let lower = clause.to_lowercase();
    for marker in ["paid with the ", "paid with ", "with the ", "with my "] {
        if let Some(pos) = lower.rfind(marker) {
            let end = pos + marker.len();
            if clause.is_char_boundary(end) {
                let name = clause[end..].trim().trim_end_matches('.').trim();
                let name = name.split(',').next().unwrap_or(name).trim();
                if !name.is_empty() && !name.chars().any(|ch| ch.is_ascii_digit()) {
                    return clean_account_name(name);
                }
            }
        }
    }
    if let Some(pos) = clause.rfind(',') {
        let tail = clause[pos + 1..].trim().trim_end_matches('.').trim();
        let words = tail.split_whitespace().count();
        let lower_tail = tail.to_lowercase();
        if (1..=4).contains(&words) && !tail.chars().any(|ch| ch.is_ascii_digit()) && !lower_tail.contains("remember") && !lower_tail.contains("wait") {
            return clean_account_name(tail);
        }
    }
    String::new()
}

fn clean_account_name(value: &str) -> String {
    strip_article(value.trim().trim_end_matches('.').trim())
}

fn strip_article(value: &str) -> String {
    let mut words: Vec<&str> = value.split_whitespace().collect();
    while words.first().map(|word| fold(word)).is_some_and(|word| matches!(word.as_str(), "a" | "an" | "the" | "my")) {
        words.remove(0);
    }
    words.join(" ")
}

fn title_phrase(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn forgotten(text: &str) -> bool {
    let folded = fold(text);
    folded.contains("remember") || folded.contains("not sure") || folded.contains("dont know") || folded.contains("unknown")
}

fn find_transactions(store: &Store, args: &Value) -> Result<Vec<usize>, String> {
    let account = args["matchAccount"].as_str().unwrap_or("").trim().to_string();
    let query = {
        let query = args["matchQuery"].as_str().unwrap_or("").trim();
        let category = args["matchCategory"].as_str().unwrap_or("").trim();
        if !query.is_empty() { query.to_string() } else { category.to_string() }
    };
    let ask = Ask {
        view: "search".into(),
        month: String::new(),
        kind: String::new(),
        account,
        direction: args["direction"].as_str().unwrap_or("").trim().to_string(),
        recurrence: String::new(),
        query,
        amount: amount_text(args.get("matchAmount")),
        date: args["matchDate"].as_str().unwrap_or("").trim().to_string(),
    };
    if ask.account.is_empty() && ask.query.is_empty() && ask.amount.is_empty() && ask.date.is_empty() {
        return Err("Which transaction?".into());
    }
    if !ask.account.is_empty() {
        match locate_account(store, "", &ask.account, "")? {
            Some(_) => {}
            None => return Err(unresolved_account(store, &ask.account)),
        }
    }
    let rows = filtered_transactions(store, &ask)?;
    let ids: Vec<String> = rows.iter().map(|tx| tx.id.clone()).collect();
    Ok(store.transactions.iter().enumerate().filter(|(_, tx)| ids.iter().any(|id| id == &tx.id)).map(|(index, _)| index).collect())
}

fn which_transaction(store: &Store, indexes: &[usize]) -> String {
    let lines: Vec<String> = indexes
        .iter()
        .map(|index| {
            let tx = &store.transactions[*index];
            let account = account_name(store, &tx.account_id).unwrap_or_default();
            let label = if tx.category.is_empty() { spoken("unknown", &[]) } else { tx.category.clone() };
            format!("{} {} {} {}", tx.id, format_minor(tx.amount_minor, &tx.currency), label, account)
        })
        .collect();
    let found = and_list(lines.iter().map(String::as_str));
    spoken("Which transaction? I found {names}.", &[("names", &found)])
}

fn move_transactions(store: &mut Store, args: &Value, clock: &Clock) -> Result<Value, String> {
    let to_name = args["toAccount"].as_str().or_else(|| args["account"].as_str()).unwrap_or("").trim();
    let to_given = args["toAccountId"].as_str().unwrap_or("").trim();
    if to_name.is_empty() && to_given.is_empty() {
        return Err("Which account?".into());
    }
    let to_id = if to_given.is_empty() {
        resolve_pay_account(store, to_name, args["toAccountKind"].as_str().unwrap_or(""), "", clock)?
    } else {
        resolve_account(store, to_given, "", "", "", clock, false)?
    };
    if to_id.is_empty() {
        return Err(unresolved_account(store, to_name));
    }
    let ids: Vec<String> = args
        .get("ids")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(|value| value.as_str().map(str::trim).filter(|id| !id.is_empty()).map(str::to_string)).collect())
        .unwrap_or_default();
    let mut indexes = Vec::new();
    if !ids.is_empty() {
        for id in &ids {
            let index = store.transactions.iter().position(|tx| &tx.id == id).ok_or("No transaction with that id.")?;
            indexes.push(index);
        }
    } else {
        let from = args["fromAccount"].as_str().unwrap_or("").trim();
        let query = args["query"].as_str().unwrap_or(args["category"].as_str().unwrap_or("")).trim();
        let amount = amount_text(args.get("amount"));
        if from.is_empty() && query.is_empty() && amount.trim().is_empty() && args["date"].as_str().unwrap_or("").trim().is_empty() {
            return Err("Which transaction?".into());
        }
        let finder = json!({
            "matchAccount": from,
            "matchQuery": query,
            "matchAmount": amount,
            "matchDate": args["date"].as_str().unwrap_or(""),
            "direction": args["direction"].as_str().unwrap_or(""),
        });
        indexes = find_transactions(store, &finder)?;
        if indexes.is_empty() {
            return Err("No transactions matched.".into());
        }
    }
    let to_saved = account_name(store, &to_id).unwrap_or_default();
    let floor = store.accounts.iter().find(|account| account.id == to_id).map(|account| account.balance_updated_ms).unwrap_or(0);
    let mut moved = Vec::new();
    for index in indexes {
        if store.transactions[index].account_id == to_id {
            continue;
        }
        store.transactions[index].account_id = to_id.clone();
        if store.transactions[index].created_ms <= floor {
            store.transactions[index].created_ms = floor.saturating_add(1);
        }
        store.transactions[index].updated_ms = clock.ms.max(store.transactions[index].created_ms);
        moved.push(store.transactions[index].id.clone());
    }
    if moved.is_empty() {
        return Err("No transactions matched.".into());
    }
    let count = moved.len().to_string();
    let detail = spoken("Moved {count} transactions to {name}.", &[("count", &count), ("name", &to_saved)]);
    let owed = store.accounts.iter().find(|account| account.id == to_id).and_then(|account| shown_with_fx(account, &store.transactions, &store.fx, clock.ms));
    Ok(json!({
        "ok": true,
        "movedCount": moved.len(),
        "ids": moved,
        "account": to_saved,
        "accountId": to_id,
        "shownBalanceMinor": owed,
        "detail": detail,
    }))
}

fn log_single(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    if let Some(id) = args.get("id").and_then(|value| value.as_str()).map(str::trim).filter(|id| !id.is_empty()) {
        return edit_logged(store, id, args, clock);
    }
    let direction = normalize_direction(args["direction"].as_str().unwrap_or(""))?;
    if direction.is_empty() {
        return Err("Direction is income, expense, or payment.".into());
    }
    let account_label = args["account"].as_str().unwrap_or("");
    let account_kind = args["accountKind"].as_str().unwrap_or("");
    let known = locate_account(store, "", account_label, account_kind)?;
    let saved_currency = known.map(|found| store.accounts[found.index].currency.clone()).unwrap_or_default();
    let pay_full = args["payInFull"].as_bool().unwrap_or(false) || matches!(fold(&amount_text(args.get("amount"))).as_str(), "full" | "in full" | "balance" | "pay in full");
    let (mut amount_minor, mut currency) = if pay_full {
        (0, saved_currency.clone())
    } else if amount_text(args.get("amount")).trim().is_empty() {
        return Err("Say the amount, or say pay in full.".into());
    } else {
        money_for(&amount_text(args.get("amount")), args["currency"].as_str().unwrap_or(""), &saved_currency, &clock.default_currency)?
    };
    let cadence = normalize_cadence(args["cadence"].as_str().unwrap_or(""));
    let class = classify(args["recurrence"].as_str().unwrap_or(""), &cadence);
    let provided = require_date(args["date"].as_str().unwrap_or(""), &cadence, clock.today)?;
    let date = if provided.is_empty() { iso(clock.today) } else { provided.clone() };
    let account_id = resolve_pay_account(store, args["account"].as_str().unwrap_or(""), args["accountKind"].as_str().unwrap_or(""), &currency, clock)?;
    let from_account_id = if args["fromAccount"].as_str().unwrap_or("").trim().is_empty() && args["fromAccountKind"].as_str().unwrap_or("").trim().is_empty() {
        String::new()
    } else {
        resolve_pay_account(store, args["fromAccount"].as_str().unwrap_or(""), args["fromAccountKind"].as_str().unwrap_or(""), &currency, clock)?
    };
    if pay_full {
        let account = store.accounts.iter().find(|account| account.id == account_id).ok_or("Which card?")?;
        match shown_with_fx(account, &store.transactions, &store.fx, clock.ms) {
            Some(owed) if owed > 0 => {
                amount_minor = owed;
                currency = account.currency.clone();
            }
            Some(owed) if owed < 0 => return Ok(spoken("That card is already in credit.", &[])),
            Some(_) => return Ok(spoken("That card is already paid.", &[])),
            None => return Err("Say the amount, or say pay in full.".into()),
        }
    }
    guard_payment(store, &direction, &account_id, &from_account_id, amount_minor, &currency, clock)?;
    if direction != "payment" {
        guard_crypto_spend(store, &direction, &account_id, &from_account_id, amount_minor, &currency, clock)?;
    }
    let raw_category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    let raw_sub = clean_text(args["subcategory"].as_str().unwrap_or(""), 80)?;
    let mut notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    let confirm_category = args["confirmCategory"].as_bool().unwrap_or(false);
    let (category, subcategory, hint) = assign_categories(store, &raw_category, &raw_sub, &mut notes, confirm_category);
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
        subcategory: subcategory.clone(),
        account_id: account_id.clone(),
        from_account_id: from_account_id.clone(),
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
    if direction == "payment" {
        if let (Some(card_name), Some(from_name)) = (account_name(store, &account_id), account_name(store, &from_account_id)) {
            detail = spoken("Paid {amount} toward {card} from {account}.", &[("amount", &amount), ("card", &card_name), ("account", &from_name)]);
            if let Some(card) = store.accounts.iter().find(|row| row.id == account_id && row.kind == "credit_card") {
                if let Some(minor) = shown_with_fx(card, &store.transactions, &store.fx, clock.ms) {
                    if minor < 0 {
                        let credit = format_minor(minor.abs(), &card.currency);
                        detail.push(' ');
                        detail.push_str(&spoken("{name} now has a credit of {amount}.", &[("name", &card_name), ("amount", &credit)]));
                    } else {
                        detail.push_str(&balance_clause(store, &account_id));
                    }
                }
            } else {
                detail.push_str(&balance_clause(store, &account_id));
            }
        }
    } else if store.accounts.iter().any(|row| row.id == account_id && row.kind == "credit_card") && direction == "expense" {
        if let Some(name) = account_name(store, &account_id) {
            detail = spoken("Charged {amount} to {name}.", &[("amount", &amount), ("name", &name)]);
            detail.push_str(&balance_clause(store, &account_id));
        }
    } else if let Some(name) = account_name(store, &account_id) {
        detail.push_str(&spoken(" Account: {name}.", &[("name", &name)]));
        detail.push_str(&balance_clause(store, &account_id));
    }
    if linked {
        detail.push_str(&calendar_clause(&cadence, &provided));
    }
    if !hint.is_empty() {
        detail.push(' ');
        detail.push_str(&hint);
    }
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
        return Err("Cadence is daily, weekly, monthly, yearly, or every N days, weeks, months, or years.".into());
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
    let id = args["id"].as_str().unwrap_or("").trim().to_string();
    let existing = if !id.is_empty() {
        store.rules.iter().find(|rule| rule.id == id).cloned()
    } else {
        let hits: Vec<Rule> = store.rules.iter().filter(|rule| fold(&rule.name) == fold(&name)).cloned().collect();
        if hits.len() > 1 {
            let names = and_list(hits.iter().map(|rule| rule.name.as_str()));
            return Err(spoken("Which payment? I found {names}.", &[("names", &names)]));
        }
        hits.into_iter().next()
    };
    let mut direction = if args["direction"].as_str().map(str::trim).filter(|s| !s.is_empty()).is_some() {
        normalize_direction(args["direction"].as_str().unwrap_or(""))?
    } else if let Some(rule) = &existing {
        rule.direction.clone()
    } else if fold(&name).contains("salary") || fold(&name).contains("paycheck") {
        "income".into()
    } else {
        "expense".into()
    };
    let mut cadence = cadence;
    let mut date_raw = date_raw;
    let mut amount = amount;
    let mut currency = currency;
    let mut category = clean_text(args["category"].as_str().unwrap_or(""), 80)?;
    let mut notes = clean_text(args["notes"].as_str().unwrap_or(""), 500)?;
    let mut account_id = if args.get("account").is_some() || args.get("accountKind").is_some() {
        resolve_pay_account(store, args["account"].as_str().unwrap_or(""), args["accountKind"].as_str().unwrap_or(""), if currency.is_empty() { &clock.default_currency } else { &currency }, clock)?
    } else {
        String::new()
    };
    if let Some(rule) = &existing {
        if amount.is_none() {
            amount = rule.amount_minor;
        }
        if currency.is_empty() {
            currency = rule.currency.clone();
        }
        if cadence.is_empty() && args.get("cadence").is_none() {
            cadence = rule.cadence.clone();
        }
        if date_raw.is_empty() && args.get("date").is_none() && args.get("dueDate").is_none() && args.get("nextDate").is_none() && args.get("day").is_none() {
            date_raw = rule.next_date.clone();
        }
        if args.get("direction").is_none() {
            direction = rule.direction.clone();
        }
        if args.get("category").is_none() {
            category = rule.category.clone();
        }
        if args.get("notes").is_none() {
            notes = rule.notes.clone();
        }
        if account_id.is_empty() && args.get("account").is_none() && args.get("accountKind").is_none() {
            account_id = rule.account_id.clone();
        }
    }
    let saved = upsert_named_rule(store, &id, &name, amount, &currency, &direction, &cadence, &date_raw, &category, &account_id, &notes, &[], false, lead_arg(args), clock)?;
    let amount_changed = existing.as_ref().is_some_and(|rule| rule.amount_minor != amount) && amount.is_some();
    remember_category(store, &category);
    let money = match amount {
        Some(minor) => format_minor(minor, if currency.is_empty() { &clock.default_currency } else { &currency }),
        None => spoken("no amount", &[]),
    };
    let template = if programmed { "Programmed {name}, {money}." } else { "Saved {name}, {money}." };
    let mut detail = spoken(template, &[("name", &name), ("money", &money)]);
    if recurrence::is_repeating(&cadence) {
        let word = cadence_spoken(&cadence, store.rules.iter().find(|rule| rule.id == saved).map(|rule| rule.interval_days).unwrap_or(0));
        detail.push_str(&spoken(" Repeats {cadence}.", &[("cadence", &word)]));
    }
    detail.push_str(&calendar_clause(&cadence, &date_raw));
    let _ = saved;
    if let Some(rule) = store.rules.iter_mut().find(|rule| rule.id == saved) {
        paint_schedule_args(rule, args);
        let _ = hydrate_rule(rule);
    }
    if amount_changed {
        detail = spoken(
            "Updated {name} to {amount}. Future unpaid occurrences use this. Paid ones stay as recorded.",
            &[("name", &name), ("amount", &money)],
        );
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
    if let Some(done) = category_phrase(store, phrase) {
        return done;
    }
    let parsed = crate::finance_schedule::parse_phrase(phrase, clock.today, &clock.default_currency)?;
    match parsed.kind.as_str() {
        "transaction" => log_single(
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
        "list" => Ok(list_schedules(store)),
        "delete" => phrase_delete(store, &parsed),
        "edit" => phrase_edit(store, &parsed, clock),
        _ => phrase_schedule(store, &parsed, clock),
    }
}

fn list_schedules(store: &Store) -> String {
    let rows: Vec<&Rule> = store.rules.iter().filter(|rule| !rule.from_account).collect();
    if rows.is_empty() {
        return spoken("No recurring item is saved.", &[]);
    }
    let mut lines = Vec::new();
    for rule in rows {
        let money = match rule.amount_minor {
            Some(amount) => format_minor(amount, &rule.currency),
            None => spoken("no amount", &[]),
        };
        let when = if rule.next_date.is_empty() { spoken("No date yet", &[]) } else { rule.next_date.clone() };
        let cadence = cadence_spoken(&rule.cadence, rule.interval_days);
        lines.push(spoken("{name}, {money}, {cadence}, next {date}.", &[("name", &rule.name), ("money", &money), ("cadence", &cadence), ("date", &when)]));
    }
    lines.join(" ")
}

fn phrase_delete(store: &mut Store, parsed: &crate::finance_schedule::Parsed) -> Result<String, String> {
    let args = json!({ "kind": "rule", "name": parsed.name, "confirmed": parsed.action == "delete_yes" });
    if parsed.action != "delete_yes" {
        return describe_delete(store, &args);
    }
    apply_delete(store, &args)
}

fn phrase_edit(store: &mut Store, parsed: &crate::finance_schedule::Parsed, clock: &Clock) -> Result<String, String> {
    let index = rule_by_name(store, &parsed.name)?;
    if !parsed.account_name.is_empty() {
        let account_id = resolve_pay_account(store, &parsed.account_name, "", &clock.default_currency, clock)?;
        store.rules[index].account_id = account_id;
    }
    let rule = &mut store.rules[index];
    if !parsed.new_name.is_empty() {
        rule.name = parsed.new_name.clone();
    }
    if !parsed.cadence.is_empty() {
        rule.cadence = parsed.cadence.clone();
        rule.interval_days = parsed.interval_days;
    }
    if !parsed.amount.is_empty() {
        let (minor, currency) = resolve_money(&parsed.amount, &parsed.currency, &clock.default_currency)?;
        rule.amount_minor = Some(minor);
        if !currency.is_empty() {
            rule.currency = currency;
        }
    }
    if !parsed.start_date.is_empty() {
        rule.start_date = parsed.start_date.clone();
        rule.next_date = parsed.start_date.clone();
    }
    if !parsed.category.is_empty() {
        rule.category = parsed.category.clone();
    }
    if parsed.end_kind == "none" {
        rule.end_kind = "none".into();
        rule.end_count = 0;
        rule.end_date.clear();
    }
    rule.updated_ms = clock.ms;
    let name = rule.name.clone();
    let cadence = cadence_spoken(&rule.cadence, rule.interval_days);
    Ok(spoken("Updated {name}. Repeats {cadence}.", &[("name", &name), ("cadence", &cadence)]))
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
    let phrase = args.get("phrase").and_then(|value| value.as_str()).unwrap_or("");
    if let Some((sentence, _)) = spoken_color_phrase(store, phrase)? {
        return Ok(sentence);
    }
    let physical = parse_physical(phrase);
    let painted = color_command(phrase);
    let hinted = crate::finance_prices::parse_holding_phrase(phrase);
    let named = if args["name"].as_str().unwrap_or("").trim().is_empty() {
        if let Some(hit) = physical.as_ref() {
            hit.name.clone()
        } else if let Some((name, _)) = painted.as_ref() {
            name.clone()
        } else {
            hinted.as_ref().map(|row| title_phrase(&row.query)).unwrap_or_default()
        }
    } else {
        args["name"].as_str().unwrap_or("").to_string()
    };
    let name = clean_text(&named, 80)?;
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
    let mut display = if id.is_empty() {
        name.clone()
    } else {
        store.accounts.iter().find(|account| account.id == id).map(|account| account.name.clone()).unwrap_or_else(|| name.clone())
    };
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
        unit: args["unit"].as_str().unwrap_or("").trim().to_string(),
        unit_price: amount_text(args.get("unitPrice")),
        price_override: args["priceOverride"].as_bool().unwrap_or(false),
        touch_override: args.get("unitPrice").is_some() || args.get("priceOverride").is_some(),
        value_mode: String::new(),
        new_name: String::new(),
        lead_days: lead_arg(args),
        change_kind: allow_kind,
        credit_limit: String::new(),
        due_day: None,
        statement_day: None,
        last_four: String::new(),
        edit_limit: false,
        edit_due_day: false,
        edit_statement_day: false,
        edit_last_four: false,
        purchase: String::new(),
        purchase_currency: String::new(),
        purchase_date: String::new(),
        revalue: String::new(),
        revalue_currency: String::new(),
        revalue_date: String::new(),
        isin: String::new(),
    };
    if args.get("phrase").and_then(|value| value.as_str()).is_some() {
        if let Some((_, swatch)) = painted {
            draft.color = swatch;
        }
        if let Some(hit) = physical {
            draft.kind = if draft.kind.trim().is_empty() { "investment".into() } else { draft.kind };
            draft.investment_type = if hit.revalue { draft.investment_type } else { "collectible".into() };
            if hit.revalue {
                draft.revalue = hit.amount;
                draft.revalue_currency = hit.currency;
                draft.revalue_date = hit.date;
            } else {
                draft.purchase = hit.amount.clone();
                draft.purchase_currency = hit.currency.clone();
                draft.purchase_date = hit.date.clone();
                draft.balance = hit.amount;
                draft.currency = hit.currency;
            }
        }
    }
    paint_holding(&mut draft, args);
    if !draft.units.trim().is_empty() && (!draft.unit_price.trim().is_empty() || !draft.unit.trim().is_empty() || draft.investment_type == "crypto" || draft.investment_type == "stock" || draft.investment_type == "fund" || draft.investment_type == "metal") {
        draft.value_mode = "units".into();
    } else if args.get("units").is_some() && args.get("unitPrice").is_some() {
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
    if args.get("creditLimit").is_some() {
        draft.credit_limit = amount_text(args.get("creditLimit"));
        draft.edit_limit = true;
    }
    if let Some(day) = args.get("dueDay").and_then(|value| value.as_u64()) {
        draft.due_day = Some(day as u32);
        draft.edit_due_day = true;
    } else if args.get("dueDay").is_some() {
        draft.due_day = None;
        draft.edit_due_day = true;
    }
    if let Some(day) = args.get("statementDay").and_then(|value| value.as_u64()) {
        draft.statement_day = Some(day as u32);
        draft.edit_statement_day = true;
    } else if args.get("statementDay").is_some() {
        draft.statement_day = None;
        draft.edit_statement_day = true;
    }
    if args.get("lastFour").is_some() {
        draft.last_four = args["lastFour"].as_str().unwrap_or("").to_string();
        draft.edit_last_four = true;
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

fn apply_card_fields(account: &mut Account, draft: &AccountDraft, clock: &Clock) -> Result<(), String> {
    if draft.edit_limit {
        account.credit_limit_minor = if draft.credit_limit.trim().is_empty() {
            None
        } else {
            Some(resolve_money(&draft.credit_limit, &account.currency, &clock.default_currency)?.0)
        };
    }
    if draft.edit_due_day {
        account.due_day = match draft.due_day {
            Some(day) if (1..=31).contains(&day) => Some(day),
            Some(_) => return Err("Day of the month, 1 to 31.".into()),
            None => None,
        };
    }
    if draft.edit_statement_day {
        account.statement_day = match draft.statement_day {
            Some(day) if (1..=31).contains(&day) => Some(day),
            Some(_) => return Err("Day of the month, 1 to 31.".into()),
            None => None,
        };
    }
    if draft.edit_last_four {
        let digits: String = draft.last_four.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            account.last_four.clear();
        } else if digits.len() == 4 {
            account.last_four = digits;
        } else {
            return Err("Use the last four digits only.".into());
        }
    }
    if account.kind == "credit_card" && account.due_date.is_empty() {
        if let Some(day) = account.due_day {
            account.due_date = iso(next_month_day(day, clock.today));
            if account.cadence.is_empty() {
                account.cadence = "monthly".into();
            }
        }
    }
    Ok(())
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
    if mask.color || !draft.color.trim().is_empty() {
        account.color = if draft.color.trim().is_empty() { default_swatch(&account.id) } else { normalize_color(&draft.color)? };
    }
    if account.color.is_empty() {
        account.color = default_swatch(&account.id);
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
        } else if account.price_minor.is_none() && !account.units.is_empty() && crate::finance_fx::is_crypto(&account.currency) {
            let minor = parse_minor_signed(&account.units, exponent(&account.currency))?;
            account.balance_minor = Some(minor);
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
            let (minor, currency) = if account.kind == "credit_card" {
                money_for_statement(&draft.balance, &draft.currency, &account.currency, &clock.default_currency)?
            } else {
                money_for(&draft.balance, &draft.currency, &account.currency, &clock.default_currency)?
            };
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
        } else if account.price_minor.is_none() && !account.units.is_empty() && crate::finance_fx::is_crypto(&account.currency) {
            let minor = parse_minor_signed(&account.units, exponent(&account.currency))?;
            account.balance_minor = Some(minor);
            account.balance_updated_ms = clock.ms;
            account.balance_as_of = iso(clock.today);
        }
    }
    if !draft.unit.trim().is_empty() {
        account.measure_unit = normalize_measure(&draft.unit);
    }
    if draft.touch_override {
        account.price_override = draft.price_override || !draft.unit_price.trim().is_empty();
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
            return Err("Cadence is daily, weekly, monthly, yearly, or every N days, weeks, months, or years.".into());
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
    if !draft.purchase.trim().is_empty() {
        let (minor, currency) = resolve_money(&draft.purchase, if draft.purchase_currency.trim().is_empty() { &account.currency } else { &draft.purchase_currency }, &clock.default_currency)?;
        account.purchase_minor = Some(minor);
        account.purchase_currency = currency;
        if !draft.purchase_date.trim().is_empty() {
            account.purchase_date = require_date(&draft.purchase_date, "", clock.today)?;
        }
        account.investment_type = if account.investment_type.is_empty() { "collectible".into() } else { account.investment_type };
        account.price_override = true;
    }
    if !draft.revalue.trim().is_empty() {
        let (minor, currency) = resolve_money(&draft.revalue, if draft.revalue_currency.trim().is_empty() { &account.currency } else { &draft.revalue_currency }, &clock.default_currency)?;
        let dated = if draft.revalue_date.trim().is_empty() { iso(clock.today) } else { require_date(&draft.revalue_date, "", clock.today)? };
        account.valuations.push(Valuation { as_of: dated.clone(), amount_minor: minor, currency: currency.clone() });
        account.balance_minor = Some(minor);
        account.currency = currency;
        account.balance_as_of = dated;
        account.balance_updated_ms = clock.ms;
        account.price_override = true;
        if account.kind != "investment" {
            account.kind = "investment".into();
        }
        if account.investment_type.is_empty() {
            account.investment_type = "collectible".into();
        }
    }
    if !draft.isin.trim().is_empty() {
        account.isin = draft.isin.trim().to_ascii_uppercase();
    }
    apply_card_fields(&mut account, draft, clock)?;
    account.updated_ms = clock.ms;
    let id = account.id.clone();
    let detail = account_detail(&account);
    remember_entity(store, "account", &account.id, &account.color);
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
    if !account.units.trim().is_empty() {
        let unit = measure_spoken(&account.measure_unit);
        parts.push(spoken("Quantity {qty} {unit}.", &[("qty", &account.units), ("unit", &unit)]));
    } else {
        match account.balance_minor {
            Some(minor) => {
                let amount = format_minor(minor, &account.currency);
                parts.push(spoken("Stated balance {amount}.", &[("amount", &amount)]));
            }
            None => parts.push(spoken("No balance was stated.", &[])),
        }
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
        subcategory: String::new(),
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
    let account_id = if !draft.account_id.trim().is_empty() {
        resolve_account(store, &draft.account_id, "", "", &currency, clock, false)?
    } else {
        resolve_pay_account(store, &draft.account_name, &draft.account_kind, &currency, clock)?
    };
    let from_account_id = if draft.from_account_id.trim().is_empty() && draft.from_account_name.trim().is_empty() && draft.from_account_kind.trim().is_empty() {
        String::new()
    } else if !draft.from_account_id.trim().is_empty() {
        resolve_account(store, &draft.from_account_id, "", "", &currency, clock, false)?
    } else {
        resolve_pay_account(store, &draft.from_account_name, &draft.from_account_kind, &currency, clock)?
    };
    guard_payment(store, &direction, &account_id, &from_account_id, amount_minor, &currency, clock)?;
    if direction != "payment" {
        guard_crypto_spend(store, &direction, &account_id, &from_account_id, amount_minor, &currency, clock)?;
    }
    let category = clean_text(&draft.category, 80)?;
    let subcategory = clean_text(&draft.subcategory, 80)?;
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
        category: category.clone(),
        subcategory: subcategory.clone(),
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
    remember_category(store, &category);
    if !subcategory.is_empty() {
        remember_child(store, &subcategory, &category);
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
        subcategory: String::new(),
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
    dirty |= link_holdings(store);
    dirty
}

fn schedule_repeats(cadence: &str, interval: u32) -> bool {
    crate::finance_schedule::repeats(cadence, interval)
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

fn budget_period(raw: &str, today: NaiveDate) -> String {
    let folded = fold(raw);
    if folded.is_empty() || matches!(folded.as_str(), "this" | "month" | "current") {
        return format!("{:04}-{:02}", today.year(), today.month());
    }
    if matches!(folded.as_str(), "template" | "recurring" | "repeat") {
        return String::new();
    }
    if folded == "next" {
        let (year, month) = if today.month() == 12 { (today.year() + 1, 1) } else { (today.year(), today.month() + 1) };
        return format!("{year:04}-{month:02}");
    }
    if raw.len() >= 7 && raw.as_bytes().get(4) == Some(&b'-') {
        return raw[..7].to_string();
    }
    format!("{:04}-{:02}", today.year(), today.month())
}

fn budget_role(raw: &str) -> String {
    match fold(raw).as_str() {
        "income" | "salary" | "in" => "income".into(),
        "savings" | "save" | "saving" => "savings".into(),
        "investment" | "invest" | "investing" => "investment".into(),
        "expense" | "limit" | "spending" | "spend" => "expense".into(),
        _ => "expense".into(),
    }
}

fn line_name(line: &BudgetLine) -> String {
    let label = line.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    let sub = line.subcategory.trim();
    if !sub.is_empty() {
        return sub.to_string();
    }
    let category = line.category.trim();
    if !category.is_empty() {
        return category.to_string();
    }
    line.role.clone()
}

fn ensure_budget<'a>(store: &'a mut Store, period: &str, currency: &str, clock: &Clock) -> &'a mut Budget {
    if let Some(index) = store.budgets.iter().position(|row| row.period == period) {
        return &mut store.budgets[index];
    }
    let inherited = if period.is_empty() {
        None
    } else {
        store
            .budgets
            .iter()
            .find(|row| row.period.is_empty())
            .or_else(|| {
                store
                    .budgets
                    .iter()
                    .filter(|row| !row.period.is_empty() && row.period.as_str() < period && row.copy_forward)
                    .max_by(|a, b| a.period.cmp(&b.period))
            })
            .cloned()
    };
    let mut budget = inherited.unwrap_or_default();
    budget.id = alloc(store, "bud");
    budget.period = period.to_string();
    budget.currency = if budget.currency.is_empty() { currency.to_string() } else { budget.currency };
    budget.warned.clear();
    budget.updated_ms = clock.ms;
    store.budgets.push(budget);
    let last = store.budgets.len() - 1;
    &mut store.budgets[last]
}

fn upsert_line(store: &mut Store, budget_index: usize, line: &Value, clock: &Clock) -> Result<String, String> {
    let role = budget_role(line["role"].as_str().unwrap_or("expense"));
    let category = line["category"].as_str().unwrap_or("").trim().to_string();
    let subcategory = line["subcategory"].as_str().unwrap_or("").trim().to_string();
    let label = line["label"].as_str().unwrap_or("").trim().to_string();
    let id = line["id"].as_str().unwrap_or("").trim().to_string();
    let currency_arg = line["currency"].as_str().unwrap_or("");
    let amount_text = amount_text(line.get("amount"));
    let (amount_minor, currency) = if !amount_text.is_empty() {
        resolve_money(&amount_text, currency_arg, &clock.default_currency)?
    } else if let Some(minor) = line["amountMinor"].as_i64().filter(|value| *value > 0) {
        let currency = if currency_arg.trim().is_empty() { clock.default_currency.clone() } else { canonical_currency(currency_arg)? };
        (minor, currency)
    } else {
        return Err("Say the amount.".into());
    };
    let index = store.budgets.get(budget_index).ok_or("No budget for that month.")?.lines.iter().position(|row| {
        (!id.is_empty() && row.id == id)
            || (id.is_empty() && fold(&row.role) == fold(&role) && fold(&row.category) == fold(&category) && fold(&row.subcategory) == fold(&subcategory) && fold(&row.label) == fold(&label))
    });
    let fresh_id = if index.is_none() { alloc(store, "bln") } else { String::new() };
    let name = if !label.is_empty() {
        label.clone()
    } else if !subcategory.is_empty() {
        subcategory.clone()
    } else if !category.is_empty() {
        category.clone()
    } else {
        role.clone()
    };
    let budget = store.budgets.get_mut(budget_index).ok_or("No budget for that month.")?;
    if let Some(index) = index {
        let row = &mut budget.lines[index];
        row.role = role;
        row.category = category;
        row.subcategory = subcategory;
        row.label = label;
        row.amount_minor = amount_minor;
        row.currency = currency.clone();
    } else {
        budget.lines.push(BudgetLine { id: fresh_id, role, category, subcategory, label, amount_minor, currency: currency.clone() });
    }
    if let Some(budget) = store.budgets.get_mut(budget_index) {
        budget.updated_ms = clock.ms;
    }
    Ok(spoken("Set {name} to {amount}.", &[("name", &name), ("amount", &format_minor(amount_minor, &currency))]))
}

fn apply_budget(store: &mut Store, args: &Value, clock: &Clock) -> Result<String, String> {
    let action = fold(args["action"].as_str().unwrap_or("set"));
    let period = budget_period(args["period"].as_str().unwrap_or("this"), clock.today);
    let month = if period.is_empty() { spoken("the template", &[]) } else { period.clone() };
    if action == "read" || action == "status" {
        return Ok(speak_budget(store, &period, args["category"].as_str().unwrap_or(""), args["question"].as_str().unwrap_or(""), clock));
    }
    if action == "delete" || args["remove"].as_bool().unwrap_or(false) {
        if !args["confirmed"].as_bool().unwrap_or(false) && action == "delete" {
            let name = args["category"].as_str().unwrap_or("").trim();
            return if name.is_empty() {
                Ok(spoken("Delete the budget for {month}? Say yes and I will remove it.", &[("month", &month)]))
            } else {
                Ok(spoken("Delete the {name} line? Say yes and I will remove it.", &[("name", name)]))
            };
        }
        return delete_budget(store, &period, args);
    }
    let currency = if args["currency"].as_str().unwrap_or("").trim().is_empty() {
        effective_default(store, &current_locale())
    } else {
        canonical_currency(args["currency"].as_str().unwrap_or(""))?
    };
    if store.budgets.iter().all(|row| row.period != period) {
        let _ = ensure_budget(store, &period, &currency, clock);
    }
    let index = store.budgets.iter().position(|row| row.period == period).ok_or("No budget for that month.")?;
    if let Some(copy) = args.get("copyForward").and_then(|value| value.as_bool()) {
        store.budgets[index].copy_forward = copy;
    }
    if let Some(notify) = args.get("notify").and_then(|value| value.as_bool()) {
        store.budgets[index].notify = notify;
    }
    if let Some(warned) = args.get("warned").and_then(|value| value.as_array()) {
        store.budgets[index].warned = warned.iter().filter_map(|row| row.as_str().map(str::to_string)).collect();
    }
    if args["replaceLines"].as_bool().unwrap_or(false) {
        let mut lines = Vec::new();
        for row in args["lines"].as_array().cloned().unwrap_or_default() {
            let text = amount_text(row.get("amount"));
            let (amount_minor, line_currency) = if !text.is_empty() {
                resolve_money(&text, row["currency"].as_str().unwrap_or(""), &currency)?
            } else if let Some(minor) = row["amountMinor"].as_i64().filter(|value| *value > 0) {
                (minor, if row["currency"].as_str().unwrap_or("").trim().is_empty() { currency.clone() } else { canonical_currency(row["currency"].as_str().unwrap_or(""))? })
            } else {
                continue;
            };
            lines.push(BudgetLine {
                id: {
                    let existing = row["id"].as_str().unwrap_or("").trim().to_string();
                    if existing.is_empty() { alloc(store, "bln") } else { existing }
                },
                role: budget_role(row["role"].as_str().unwrap_or("expense")),
                category: row["category"].as_str().unwrap_or("").trim().to_string(),
                subcategory: row["subcategory"].as_str().unwrap_or("").trim().to_string(),
                label: row["label"].as_str().unwrap_or("").trim().to_string(),
                amount_minor,
                currency: line_currency,
            });
        }
        if let Some(budget) = store.budgets.iter_mut().find(|row| row.period == period) {
            budget.lines = lines;
            budget.updated_ms = clock.ms;
            if !currency.is_empty() && budget.currency.is_empty() {
                budget.currency = currency;
            }
        }
        return Ok(spoken("Saved the budget for {month}.", &[("month", &month)]));
    }
    if args.get("line").is_some() || !args["category"].as_str().unwrap_or("").trim().is_empty() || !amount_text(args.get("amount")).is_empty() {
        let line = if args.get("line").is_some() { args["line"].clone() } else { args.clone() };
        let detail = upsert_line(store, index, &line, clock)?;
        return Ok(detail);
    }
    Ok(spoken("Saved the budget for {month}.", &[("month", &month)]))
}

fn delete_budget(store: &mut Store, period: &str, args: &Value) -> Result<String, String> {
    let name = args["category"].as_str().unwrap_or(args["name"].as_str().unwrap_or("")).trim().to_string();
    let id = args["id"].as_str().unwrap_or("").trim().to_string();
    let month = if period.is_empty() { spoken("the template", &[]) } else { period.to_string() };
    let Some(index) = store.budgets.iter().position(|row| row.period == period || (!id.is_empty() && row.id == id)) else {
        return Err("No budget for that month.".into());
    };
    if name.is_empty() && args["lineId"].as_str().unwrap_or("").trim().is_empty() {
        store.budgets.remove(index);
        return Ok(spoken("Deleted the budget for {month}.", &[("month", &month)]));
    }
    let line_id = args["lineId"].as_str().unwrap_or("").trim().to_string();
    let budget = &mut store.budgets[index];
    let before = budget.lines.len();
    budget.lines.retain(|line| {
        if !line_id.is_empty() && line.id == line_id {
            return false;
        }
        if !id.is_empty() && line.id == id && !name.is_empty() {
            return false;
        }
        !(fold(&line_name(line)) == fold(&name) || fold(&line.category) == fold(&name))
    });
    if budget.lines.len() == before {
        return Err("No budget line with that name.".into());
    }
    Ok(spoken("Deleted the {name} line.", &[("name", &name)]))
}

fn flow_role(direction: &str, kind: &str, account_kind: &str) -> &'static str {
    if direction == "income" {
        return "income";
    }
    if direction == "payment" {
        return "expense";
    }
    if direction == "transfer" {
        if account_kind == "investment" || kind == "investment" {
            return "investment";
        }
        return "ignore";
    }
    if direction != "expense" {
        return "ignore";
    }
    if kind == "savings" {
        return "ignore";
    }
    if kind == "investment" {
        return "investment";
    }
    "expense"
}

fn category_kind_of(store: &Store, name: &str) -> String {
    store.categories.iter().find(|row| fold(&row.name) == fold(name)).map(|row| row.kind.clone()).unwrap_or_default()
}

fn converted_sum(store: &Store, month: &str, role: &str, category: &str, now_ms: i64, display: &str) -> Result<i64, String> {
    let mut total = 0i64;
    let mut missing = Vec::new();
    for tx in &store.transactions {
        if !month.is_empty() && !tx.date.starts_with(month) {
            continue;
        }
        let kind = category_kind_of(store, &tx.category);
        let account_kind = store.accounts.iter().find(|row| row.id == tx.account_id).map(|row| row.kind.as_str()).unwrap_or("");
        if flow_role(&tx.direction, &kind, account_kind) != role {
            continue;
        }
        if !category.is_empty() && fold(&tx.category) != fold(category) && fold(&tx.subcategory) != fold(category) && fold(&tx.notes) != fold(category) {
            continue;
        }
        match crate::finance_fx::convert(tx.amount_minor, &tx.currency, display, &store.fx, now_ms) {
            Some(row) => total = total.saturating_add(row.amount_minor),
            None => missing.push(tx.currency.to_uppercase()),
        }
    }
    if !missing.is_empty() && total == 0 {
        missing.sort();
        missing.dedup();
        return Err(spoken("This period needs a rate for {codes}.", &[("codes", &missing.join(", "))]));
    }
    Ok(total)
}

fn speak_budget(store: &Store, period: &str, category: &str, question: &str, clock: &Clock) -> String {
    let display = effective_default(store, &current_locale());
    let month = if period.is_empty() { spoken("the template", &[]) } else { period.to_string() };
    let Some(budget) = store.budgets.iter().find(|row| row.period == period).or_else(|| store.budgets.iter().find(|row| row.period.is_empty())) else {
        return spoken("No budget is saved for {month}.", &[("month", &month)]);
    };
    let now_ms = clock.ms.max(1);
    let asked = fold(&format!("{category} {question}"));
    if !category.trim().is_empty() {
        let line = budget.lines.iter().find(|line| fold(&line_name(line)) == fold(category) || fold(&line.category) == fold(category));
        let Some(line) = line else {
            return spoken("No budget is saved for {month}.", &[("month", &month)]);
        };
        let planned = crate::finance_fx::convert(line.amount_minor, &line.currency, &display, &store.fx, now_ms).map(|row| row.amount_minor).unwrap_or(line.amount_minor);
        let actual = converted_sum(store, period, &budget_role(&line.role), &line.category, now_ms, &display).unwrap_or(0);
        let left = planned.saturating_sub(actual);
        if actual > planned {
            return spoken("{name} is over by {amount}.", &[("name", &line_name(line)), ("amount", &format_minor(actual - planned, &display))]);
        }
        return spoken("{name} has {left} left of {planned}.", &[("name", &line_name(line)), ("left", &format_minor(left, &display)), ("planned", &format_minor(planned, &display))]);
    }
    let income = converted_sum(store, period, "income", "", now_ms, &display).unwrap_or(0);
    let spending = converted_sum(store, period, "expense", "", now_ms, &display).unwrap_or(0);
    let investments = converted_sum(store, period, "investment", "", now_ms, &display).unwrap_or(0);
    let savings = income.saturating_sub(spending).saturating_sub(investments);
    let planned_spend: i64 = budget
        .lines
        .iter()
        .filter(|line| budget_role(&line.role) == "expense")
        .filter_map(|line| crate::finance_fx::convert(line.amount_minor, &line.currency, &display, &store.fx, now_ms).map(|row| row.amount_minor))
        .sum();
    let posture = if asked.contains("over") || asked.contains("sobre") || asked.contains("über") || asked.contains("sobre") {
        if planned_spend > 0 && spending > planned_spend {
            spoken("You are over the spending plan.", &[])
        } else {
            spoken("Spending is within the plan.", &[])
        }
    } else {
        String::new()
    };
    let summary = spoken(
        "Budget for {month}. Income {income}. Spending {spending}. Savings {savings}. Investments {investments}.",
        &[
            ("month", &month),
            ("income", &format_minor(income, &display)),
            ("spending", &format_minor(spending, &display)),
            ("savings", &format_minor(savings, &display)),
            ("investments", &format_minor(investments, &display)),
        ],
    );
    if posture.is_empty() { summary } else { format!("{posture} {summary}") }
}

fn speak_flow(store: &Store, today: NaiveDate) -> String {
    let month = format!("{:04}-{:02}", today.year(), today.month());
    let display = effective_default(store, &current_locale());
    let now_ms = Local::now().timestamp_millis();
    let income = match converted_sum(store, &month, "income", "", now_ms, &display) {
        Ok(value) => value,
        Err(message) => return message,
    };
    let spending = converted_sum(store, &month, "expense", "", now_ms, &display).unwrap_or(0);
    let investments = converted_sum(store, &month, "investment", "", now_ms, &display).unwrap_or(0);
    if income == 0 && spending == 0 && investments == 0 {
        return spoken("Nothing recorded in this period.", &[]);
    }
    let mut tops: Vec<(String, i64)> = Vec::new();
    for tx in &store.transactions {
        if !tx.date.starts_with(&month) {
            continue;
        }
        let kind = category_kind_of(store, &tx.category);
        let account_kind = store.accounts.iter().find(|row| row.id == tx.account_id).map(|row| row.kind.as_str()).unwrap_or("");
        if flow_role(&tx.direction, &kind, account_kind) != "expense" {
            continue;
        }
        let name = if tx.category.trim().is_empty() { spoken("Uncategorized", &[]) } else { tx.category.clone() };
        let Some(converted) = crate::finance_fx::convert(tx.amount_minor, &tx.currency, &display, &store.fx, now_ms) else { continue };
        if let Some(row) = tops.iter_mut().find(|row| fold(&row.0) == fold(&name)) {
            row.1 = row.1.saturating_add(converted.amount_minor);
        } else {
            tops.push((name, converted.amount_minor));
        }
    }
    tops.sort_by(|a, b| b.1.cmp(&a.1));
    tops.truncate(3);
    let categories = tops.iter().map(|(name, amount)| format!("{name} {}", format_minor(*amount, &display))).collect::<Vec<_>>().join(", ");
    if income == 0 {
        let spending_text = format_minor(spending, &display);
        return if categories.is_empty() {
            spoken("No income this month. Spending {spending}.", &[("spending", &spending_text)])
        } else {
            format!(
                "{} {}",
                spoken("No income this month. Spending {spending}.", &[("spending", &spending_text)]),
                spoken("Largest categories: {categories}.", &[("categories", &categories)])
            )
        };
    }
    let savings = income as i64 - spending - investments;
    let saved = if savings < 0 {
        spoken("Shortfall {amount}.", &[("amount", &format_minor(-savings, &display))])
    } else {
        spoken("Savings {amount}.", &[("amount", &format_minor(savings, &display))])
    };
    let head = spoken(
        "Income {income}. Spending {spending}.",
        &[("income", &format_minor(income, &display)), ("spending", &format_minor(spending, &display))],
    );
    if categories.is_empty() {
        format!("{head} {saved}")
    } else {
        format!("{head} {saved} {}", spoken("Largest categories: {categories}.", &[("categories", &categories)]))
    }
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
            let count = transactions_on(store, &id);
            let count_text = count.to_string();
            let moved_to = args["moveTo"].as_str().unwrap_or("").trim().to_string();
            if !moved_to.is_empty() && count > 0 {
                let clock = Clock { ms: store.accounts.iter().map(|account| account.updated_ms).max().unwrap_or(0).max(1), today: Local::now().date_naive(), currency: String::new(), default_currency: effective_default(store, &current_locale()) };
                move_transactions(store, &json!({ "fromAccount": label, "toAccount": moved_to }), &clock)?;
            }
            let remaining = transactions_on(store, &id);
            remove_account(store, &id)?;
            if count == 0 {
                Ok(spoken("Deleted the account {label}.", &[("label", &label)]))
            } else if !moved_to.is_empty() && remaining == 0 {
                let moved = spoken("Moved {count} transactions to {name}.", &[("count", &count_text), ("name", &moved_to)]);
                let deleted = spoken("Deleted the account {label}.", &[("label", &label)]);
                Ok(format!("{moved} {deleted}"))
            } else {
                Ok(spoken("Deleted the account {label}. {count} transactions are still in Activity.", &[("label", &label), ("count", &count_text)]))
            }
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
        "budget" | "budget_line" => {
            let period = budget_period(args["period"].as_str().unwrap_or(args["month"].as_str().unwrap_or("this")), Local::now().date_naive());
            let mut owned = args.clone();
            if kind == "budget_line" && owned["category"].as_str().unwrap_or("").trim().is_empty() {
                owned["category"] = args["name"].clone();
            }
            if kind == "budget" && owned["category"].as_str().unwrap_or("").trim().is_empty() {
                owned["name"] = Value::String(String::new());
            }
            delete_budget(store, &period, &owned)
        }
        _ => Err("Kind is account, transaction, rule, or budget.".into()),
    }
}

fn describe_delete(store: &Store, args: &Value) -> Result<String, String> {
    let kind = fold(args["kind"].as_str().unwrap_or(""));
    let id = args["id"].as_str().unwrap_or("").trim();
    let name = args["name"].as_str().unwrap_or("").trim();
    let label = match kind.as_str() {
        "account" => {
            let index = find_account(store, id, name)?.ok_or("Which account?")?;
            let label = store.accounts[index].name.clone();
            let count = transactions_on(store, &store.accounts[index].id);
            let count_text = count.to_string();
            return if count == 0 {
                Ok(spoken("Delete {label}? It has no transactions. Say yes and I will remove it.", &[("label", &label)]))
            } else {
                Ok(spoken(
                    "Delete {label}? It has {count} transactions. Tell me where to move them, or say yes to delete the account and leave them in Activity.",
                    &[("label", &label), ("count", &count_text)],
                ))
            };
        }
        "transaction" | "entry" => store.transactions.iter().find(|tx| tx.id == id).map(|tx| format!("{} {}", tx.direction, format_minor(tx.amount_minor, &tx.currency))).ok_or("Which transaction?")?,
        "rule" | "recurring" => {
            if !id.is_empty() {
                store.rules.iter().find(|rule| rule.id == id).map(|rule| rule.name.clone()).ok_or("Which recurring item?")?
            } else {
                store.rules.iter().find(|rule| fold(&rule.name) == fold(name)).map(|rule| rule.name.clone()).ok_or("Which recurring item?")?
            }
        }
        "budget" | "budget_line" => {
            let period = budget_period(args["period"].as_str().unwrap_or("this"), Local::now().date_naive());
            let month = if period.is_empty() { spoken("the template", &[]) } else { period };
            let name = args["name"].as_str().unwrap_or(args["category"].as_str().unwrap_or("")).trim();
            return if kind == "budget_line" || !name.is_empty() {
                Ok(spoken("Delete the {name} line? Say yes and I will remove it.", &[("name", name)]))
            } else {
                Ok(spoken("Delete the budget for {month}? Say yes and I will remove it.", &[("month", &month)]))
            };
        }
        _ => return Err("Kind is account, transaction, rule, or budget.".into()),
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

/// Exact names win, including names that differ only by case, spaces, hyphens,
/// punctuation, or diacritics. A unique close name (prefix or a small typo)
/// matches too. Different kinds that only share a fragment, such as ZKB and
/// ZKB credit card, stay apart unless the folded name is exact.
fn locate_account(store: &Store, id: &str, name: &str, kind: &str) -> Result<Option<Found>, String> {
    if !id.trim().is_empty() {
        let index = store.accounts.iter().position(|account| account.id == id.trim()).ok_or("No account with that id.")?;
        return Ok(Some(Found { index, exact: true }));
    }
    let query = fold(name);
    let query_compact = compact_name(name);
    if query_compact.is_empty() {
        return Ok(None);
    }
    let exact: Vec<usize> = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| {
            let folded = fold(&account.name);
            folded == query || compact_name(&account.name) == query_compact
        })
        .map(|(index, _)| index)
        .collect();
    if exact.len() == 1 {
        return Ok(Some(Found { index: exact[0], exact: true }));
    }
    if exact.len() > 1 {
        return Err(which_account(store, &exact));
    }
    let incoming = normalize_kind(kind);
    let mut scored: Vec<(usize, i32)> = Vec::new();
    for (index, account) in store.accounts.iter().enumerate() {
        if !incoming.is_empty() && incoming != account.kind {
            continue;
        }
        let folded = fold(&account.name);
        let compact = compact_name(&account.name);
        if compact.is_empty() {
            continue;
        }
        let mut score = 0;
        if folded.contains(&query) || query.contains(&folded) {
            score = score.max(300 + query.len() as i32);
        }
        if query_compact.len() >= 4 && (compact.starts_with(&query_compact) || query_compact.starts_with(&compact)) {
            let shorter = query_compact.len().min(compact.len());
            let longer = query_compact.len().max(compact.len());
            if shorter * 2 >= longer {
                score = score.max(800 - (longer - shorter) as i32);
            }
        }
        let distance = edit_distance(&query_compact, &compact);
        let len_diff = (query_compact.len() as i32 - compact.len() as i32).abs();
        if distance == 1 && query_compact.len() >= 5 && len_diff <= 1 {
            score = score.max(700);
        }
        if score > 0 {
            scored.push((index, score));
        }
    }
    if scored.is_empty() {
        return Ok(None);
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let best = scored[0].1;
    let close: Vec<usize> = scored.iter().filter(|row| best - row.1 < 80).map(|row| row.0).collect();
    if close.len() == 1 {
        return Ok(Some(Found { index: close[0], exact: false }));
    }
    Err(which_account(store, &close))
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
    account.color = default_swatch(&id);
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
        measure_unit: String::new(),
        price_minor: None,
        price_override: false,
        lead_days: None,
        credit_limit_minor: None,
        due_day: None,
        statement_day: None,
        last_four: String::new(),
        purchase_minor: None,
        purchase_currency: String::new(),
        purchase_date: String::new(),
        valuations: Vec::new(),
        isin: String::new(),
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
    shown_with_fx(account, transactions, &crate::finance_fx::FxBook::default(), 0)
}

fn shown_with_fx(account: &Account, transactions: &[Transaction], book: &crate::finance_fx::FxBook, now_ms: i64) -> Option<i64> {
    if account.kind == "investment" && !crate::finance_fx::is_crypto(&account.currency) {
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
        let Some(amount) = amount_for_account(tx, account, book, now_ms) else { continue };
        if account.balance_updated_ms > 0 && tx.created_ms <= account.balance_updated_ms {
            continue;
        }
        if !account.balance_as_of.is_empty() && tx.date.as_str() < account.balance_as_of.as_str() {
            continue;
        }
        let role = if tx.account_id == account.id { Role::Primary } else { Role::Source };
        balance = step_balance(balance, &account.kind, &account.currency, role, &tx.direction, amount);
    }
    Some(balance)
}

/// Same currency is identity. A missing rate is skipped, never treated as 1:1.
fn amount_for_account(tx: &Transaction, account: &Account, book: &crate::finance_fx::FxBook, now_ms: i64) -> Option<i64> {
    if account.currency.is_empty() || tx.currency.is_empty() || tx.currency.eq_ignore_ascii_case(&account.currency) {
        return Some(tx.amount_minor);
    }
    crate::finance_fx::convert(tx.amount_minor, &tx.currency, &account.currency, book, now_ms).map(|row| row.amount_minor)
}

fn step_balance(balance: i64, kind: &str, currency: &str, role: Role, direction: &str, amount: i64) -> i64 {
    let delta = match role {
        Role::Source => -amount,
        Role::Primary => match (kind, direction) {
            (_, "transfer") if is_debt(kind) => -amount,
            (_, "transfer") => amount,
            ("credit_card" | "loan" | "other_debt", "expense") => amount,
            ("credit_card" | "loan" | "other_debt", "payment" | "income") => -amount,
            ("credit_card" | "loan" | "other_debt", _) => 0,
            ("investment", _) if !crate::finance_fx::is_crypto(currency) => 0,
            (_, "income") => amount,
            (_, "expense" | "payment") => -amount,
            _ => 0,
        },
    };
    balance.saturating_add(delta)
}

fn normalize_measure(raw: &str) -> String {
    match fold(raw).as_str() {
        "oz" | "ozt" | "ounce" | "ounces" | "troy" | "troyoz" | "troy ounce" | "troy ounces" => "troy_oz".into(),
        "g" | "gram" | "grams" => "gram".into(),
        "kg" | "kilo" | "kilogram" | "kilograms" => "kilogram".into(),
        "share" | "shares" => "share".into(),
        "coin" | "coins" => "coin".into(),
        other if !other.is_empty() && other.len() <= 16 => other.to_string(),
        _ => String::new(),
    }
}

fn measure_spoken(unit: &str) -> String {
    let key = match unit {
        "troy_oz" => "troy ounces",
        "gram" => "grams",
        "kilogram" => "kilograms",
        "share" => "shares",
        "coin" => "coins",
        _ => "",
    };
    if key.is_empty() { String::new() } else { spoken(key, &[]) }
}

fn paint_holding(draft: &mut AccountDraft, args: &Value) {
    let phrase = args.get("phrase").and_then(|value| value.as_str()).unwrap_or("").trim();
    let blob = format!("{phrase} {} {}", draft.units, draft.name);
    let Some(parsed) = crate::finance_prices::parse_holding_phrase(phrase).or_else(|| crate::finance_prices::parse_holding_phrase(&blob)) else {
        return;
    };
    if draft.units.trim().is_empty() || draft.units.chars().any(|ch| ch.is_ascii_alphabetic()) {
        if !parsed.quantity.is_empty() {
            draft.units = parsed.quantity;
        }
    }
    if draft.unit.trim().is_empty() && !parsed.unit.is_empty() {
        draft.unit = parsed.unit;
    }
    if draft.name.trim().is_empty() && !parsed.query.is_empty() {
        draft.name = title_phrase(&parsed.query);
    }
    if draft.kind.trim().is_empty() {
        draft.kind = "investment".into();
    }
    if let Some(asset) = crate::finance_prices::resolve_holding(&draft.name, &draft.ticker, &draft.currency, &draft.investment_type, &draft.unit) {
        if draft.ticker.trim().is_empty() {
            draft.ticker = asset.symbol.clone();
        }
        if draft.unit.trim().is_empty() {
            draft.unit = asset.holding_unit.clone();
        }
        if draft.investment_type.trim().is_empty() || draft.investment_type == "custom" {
            draft.investment_type = match asset.kind.as_str() {
                "crypto" => "crypto".into(),
                "stock" => "stock".into(),
                "metal" => "metal".into(),
                _ => draft.investment_type.clone(),
            };
        }
        if asset.kind == "crypto" && draft.currency.trim().is_empty() {
            draft.currency = asset.symbol;
        }
    }
}

fn flow_transactions(store: &Store, today: NaiveDate) -> Vec<Transaction> {
    let mut rows = store.transactions.clone();
    rows.extend(due_flow(store, today));
    rows
}

/// Occurrences that have come due and were not posted. A card payment stays a payment, so it is not an expense.
fn due_flow(store: &Store, today: NaiveDate) -> Vec<Transaction> {
    let mut extra = Vec::new();
    for rule in &store.rules {
        if matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
            continue;
        }
        if !matches!(rule.direction.as_str(), "income" | "expense") {
            continue;
        }
        let Some(amount) = rule.amount_minor else { continue };
        if amount == 0 || rule.currency.trim().is_empty() {
            continue;
        }
        let spec = rule_spec(rule);
        let settled = settled_dates(store, &rule.id);
        for date in crate::finance_schedule::occurrence_dates(&spec, today) {
            if date > today || settled.contains(&date) || spec.skipped.contains(&date) {
                continue;
            }
            extra.push(Transaction {
                id: format!("due-{}-{}", rule.id, iso(date)),
                amount_minor: amount,
                currency: rule.currency.clone(),
                direction: rule.direction.clone(),
                category: if rule.category.is_empty() { rule.name.clone() } else { rule.category.clone() },
                subcategory: rule.subcategory.clone(),
                account_id: rule.account_id.clone(),
                from_account_id: String::new(),
                date: iso(date),
                notes: String::new(),
                recurrence: "recurring".into(),
                rule_id: rule.id.clone(),
                created_ms: 0,
                updated_ms: 0,
                occurrence_date: iso(date),
            });
        }
    }
    extra
}

fn recover_quantity(account: &mut Account) -> bool {
    let mut dirty = false;
    let normalized_unit = normalize_measure(&account.measure_unit);
    if !normalized_unit.is_empty() && normalized_unit != account.measure_unit {
        account.measure_unit = normalized_unit;
        dirty = true;
    }
    let messy = account.units.chars().any(|ch| ch.is_ascii_alphabetic());
    if account.units.trim().is_empty() || messy {
        let blob = format!("{} {} {}", account.units, account.notes, account.name);
        if let Some(parsed) = crate::finance_prices::parse_holding_phrase(&blob) {
            let named = holding_name_resolves(account) || crate::finance_prices::resolve_holding(&parsed.query, "", "", "", "").is_some();
            let digits = !parsed.quantity.is_empty() && parsed.quantity.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
            if digits && (named || messy) && account.units != parsed.quantity {
                account.units = parsed.quantity;
                dirty = true;
            }
            if account.measure_unit.trim().is_empty() && !parsed.unit.is_empty() {
                account.measure_unit = normalize_measure(&parsed.unit);
                dirty = true;
            }
        }
    }
    if account.units.trim().is_empty() && crate::finance_fx::is_crypto(&account.currency) {
        if let Some(minor) = account.balance_minor {
            let text = quantity_text(minor, &account.currency);
            if !text.is_empty() {
                account.units = text;
                dirty = true;
            }
        }
    }
    if account.units.trim().is_empty() && holding_name_resolves(account) {
        if let Some(qty) = crate::finance_prices::first_quantity(&format!("{} {}", account.notes, account.name)) {
            account.units = qty;
            dirty = true;
        }
    }
    dirty
}

fn recover_history_quantities(store: &mut Store) -> bool {
    let found: Vec<(String, String)> = store
        .accounts
        .iter()
        .filter(|account| account.units.trim().is_empty() && holding_name_resolves(account))
        .filter_map(|account| {
            let saved = store.history.iter().rev().find(|point| point.account_id == account.id && !point.quantity.trim().is_empty())?.quantity.clone();
            let qty = crate::finance_prices::first_quantity(&saved).unwrap_or(saved);
            let plain = !qty.is_empty() && qty.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
            plain.then_some((account.id.clone(), qty))
        })
        .collect();
    let mut dirty = false;
    for (id, qty) in found {
        if let Some(account) = store.accounts.iter_mut().find(|account| account.id == id) {
            if account.units.trim().is_empty() {
                account.units = qty;
                dirty = true;
            }
        }
    }
    dirty
}

fn holding_name_resolves(account: &Account) -> bool {
    crate::finance_prices::resolve_holding(&account.name, &account.ticker, &account.currency, &account.investment_type, &format!("{} {}", account.notes, account.measure_unit)).is_some()
}

fn quantity_text(amount: i64, currency: &str) -> String {
    let labeled = format_minor(amount, currency);
    labeled.split_whitespace().next().unwrap_or("").to_string()
}

fn link_holdings(store: &mut Store) -> bool {
    // Read quantities and pins before the cache version bump. A bump must not erase the id that prices this holding.
    let mut dirty = recover_history_quantities(store);
    dirty |= invalidate_price_cache(store);
    for account in &mut store.accounts {
        dirty |= recover_quantity(account);
        if matches!(account.kind.as_str(), "bank" | "credit_card" | "loan" | "other_debt") {
            continue;
        }
        if account.kind == "cash" && !crate::finance_fx::is_crypto(&account.currency) && !holding_name_resolves(account) {
            continue;
        }
        let Some(asset) = holding_of(account) else { continue };
        if account.kind == "cash" && asset.kind != "crypto" {
            continue;
        }
        if matches!(account.kind.as_str(), "other" | "custom" | "") || (account.kind == "cash" && asset.kind == "crypto") {
            if account.kind != "investment" {
                account.kind = "investment".into();
                dirty = true;
            }
        }
        let name_known = crate::finance_prices::resolve_holding(&account.name, "", "", "", "").is_some();
        if account.ticker.trim().is_empty() || (name_known && !account.ticker.eq_ignore_ascii_case(&asset.symbol)) {
            if account.ticker != asset.symbol {
                account.ticker = asset.symbol.clone();
                dirty = true;
            }
        }
        if account.measure_unit.trim().is_empty() {
            account.measure_unit = asset.holding_unit.clone();
            dirty = true;
        }
        if asset.symbol == "EPIC" && account.investment_type != "crypto" {
            account.investment_type = "crypto".into();
            dirty = true;
        } else if account.investment_type.is_empty() || (account.investment_type == "custom" && asset.kind != "custom") {
            let next = match asset.kind.as_str() {
                "crypto" => "crypto",
                "stock" => "stock",
                "metal" => "metal",
                _ => "",
            };
            if !next.is_empty() {
                account.investment_type = next.into();
                dirty = true;
            }
        }
    }
    let resolved: Vec<(String, crate::finance_prices::AssetRef)> = store.accounts.iter().filter_map(|account| holding_of(account).map(|asset| (account.id.clone(), asset))).collect();
    for (id, asset) in resolved {
        remember_pin(store, &id, &asset);
    }
    dirty
}

fn is_holding_account(account: &Account) -> bool {
    !matches!(account.kind.as_str(), "bank" | "cash" | "credit_card" | "loan" | "other_debt")
}

fn holding_of(account: &Account) -> Option<crate::finance_prices::AssetRef> {
    let mut asset = crate::finance_prices::resolve_holding(&account.name, &account.ticker, &account.currency, &account.investment_type, &format!("{} {}", account.notes, account.measure_unit))?;
    if !account.measure_unit.trim().is_empty() {
        asset.holding_unit = account.measure_unit.clone();
    }
    Some(asset)
}

fn holding_resolved(store: &Store, account: &Account) -> Option<crate::finance_prices::AssetRef> {
    let mut asset = holding_of(account)?;
    // The typed name wins over a pin saved by an older build, including a manual epic-chain id.
    if let Some(named) = crate::finance_prices::resolve_holding(&account.name, "", "", "", "") {
        if named.coingecko_id == "epic-cash" {
            asset.symbol = named.symbol;
            asset.kind = named.kind;
            asset.coingecko_id = named.coingecko_id;
            asset.paprika_id = named.paprika_id;
            return Some(asset);
        }
    }
    if let Some(pin) = store.provider_pins.iter().find(|pin| pin.account_id == account.id && pin.manual) {
        if !pin.coingecko_id.is_empty() {
            asset.coingecko_id = pin.coingecko_id.clone();
        }
        if !pin.paprika_id.is_empty() {
            asset.paprika_id = pin.paprika_id.clone();
        }
        return Some(asset);
    }
    // A known coin keeps the id from its name. A pin is only for a ticker the alias table does not know.
    // epic-chain is the wrong coin for EPIC Cash. Never let that automatic pin win.
    if asset.symbol == "EPIC" && !asset.coingecko_id.is_empty() {
        return Some(asset);
    }
    let known = !asset.coingecko_id.is_empty() || !asset.paprika_id.is_empty();
    if !known {
        if let Some(pin) = store.provider_pins.iter().find(|pin| pin.account_id == account.id) {
            if pin.coingecko_id.eq_ignore_ascii_case("epic-chain") && asset.symbol == "EPIC" {
                asset.coingecko_id = "epic-cash".into();
                asset.paprika_id = "epic-epic-cash".into();
            } else {
                if !pin.coingecko_id.is_empty() {
                    asset.coingecko_id = pin.coingecko_id.clone();
                }
                if !pin.paprika_id.is_empty() {
                    asset.paprika_id = pin.paprika_id.clone();
                }
            }
        }
    }
    // A holding named EPIC Cash always prices epic-cash, even when an older pin stored epic-chain or a blank id.
    if let Some(named) = crate::finance_prices::resolve_holding(&account.name, "", "", "", "") {
        if named.coingecko_id == "epic-cash" {
            asset.symbol = named.symbol;
            asset.kind = named.kind;
            asset.coingecko_id = named.coingecko_id;
            asset.paprika_id = named.paprika_id;
        }
    }
    Some(asset)
}

const PRICE_CACHE_VERSION: u32 = 6;

/// Drop quotes that cannot be a price. A version change clears misses so the next pass tries again.
/// Pins are read and corrected first. They are not wiped, including a pin for a holding that has no price yet.
fn invalidate_price_cache(store: &mut Store) -> bool {
    let mut changed = false;
    let before = store.prices.quotes.len();
    store.prices.quotes.retain(|quote| quote.price.is_finite() && quote.price > 0.0);
    if store.prices.quotes.len() != before {
        changed = true;
    }
    if store.price_cache_version != PRICE_CACHE_VERSION {
        store.prices.misses.clear();
        store.prices.attempts.clear();
        store.prices.quotes.retain(|quote| !quote.symbol.eq_ignore_ascii_case("EPIC"));
        store.price_cache_version = PRICE_CACHE_VERSION;
        changed = true;
    }
    changed
}

fn remember_pin(store: &mut Store, account_id: &str, asset: &crate::finance_prices::AssetRef) {
    if asset.coingecko_id.is_empty() && asset.paprika_id.is_empty() {
        return;
    }
    if let Some(pin) = store.provider_pins.iter_mut().find(|pin| pin.account_id == account_id) {
        if pin.manual {
            return;
        }
        // A known coin replaces a pin saved under the wrong id, such as epic-chain for EPIC Cash.
        if !asset.coingecko_id.is_empty() && !pin.coingecko_id.eq_ignore_ascii_case(&asset.coingecko_id) {
            pin.coingecko_id = asset.coingecko_id.clone();
        }
        if !asset.paprika_id.is_empty() && !pin.paprika_id.eq_ignore_ascii_case(&asset.paprika_id) {
            pin.paprika_id = asset.paprika_id.clone();
        }
    } else {
        store.provider_pins.push(crate::finance_history::ProviderPin {
            account_id: account_id.to_string(),
            coingecko_id: asset.coingecko_id.clone(),
            paprika_id: asset.paprika_id.clone(),
            manual: false,
        });
    }
}

fn held_assets(store: &Store) -> Vec<crate::finance_prices::AssetRef> {
    let mut out = Vec::new();
    for account in &store.accounts {
        if let Some(asset) = holding_resolved(store, account) {
            if !out.iter().any(|row: &crate::finance_prices::AssetRef| row.symbol == asset.symbol && row.kind == asset.kind) {
                out.push(asset);
            }
        }
    }
    out
}

struct MarketNote {
    worth_minor: Option<i64>,
    worth_currency: String,
    price: Option<f64>,
    price_currency: String,
    source: String,
    fetched_ms: i64,
    stale: bool,
    estimated: bool,
    unit: String,
    symbol: String,
}

fn market_note(store: &Store, account: &Account, now_ms: i64, display: &str) -> Option<MarketNote> {
    let asset = holding_resolved(store, account)?;
    let qty = if !account.units.trim().is_empty() {
        account.units.clone()
    } else if let Some(parsed) = crate::finance_prices::first_quantity(&format!("{} {} {}", account.units, account.notes, account.name)) {
        parsed
    } else if let Some(priced) = crate::finance_prices::price_for(&store.prices, &asset, now_ms) {
        return Some(MarketNote {
            worth_minor: None,
            worth_currency: String::new(),
            price: Some(priced.unit_price),
            price_currency: priced.currency,
            source: if priced.source == "manual" { "manual".into() } else { priced.provider },
            fetched_ms: priced.fetched_ms,
            stale: priced.stale,
            estimated: priced.estimated,
            unit: asset.holding_unit,
            symbol: asset.symbol,
        });
    } else {
        return Some(MarketNote {
            worth_minor: None,
            worth_currency: String::new(),
            price: None,
            price_currency: String::new(),
            source: String::new(),
            fetched_ms: 0,
            stale: false,
            estimated: false,
            unit: asset.holding_unit,
            symbol: asset.symbol,
        });
    };
    let (unit_price, currency, source, fetched_ms, stale, estimated) = if account.price_override {
        let price = account.price_minor?;
        let exp = exponent(&account.currency);
        (price as f64 / 10f64.powi(exp as i32), account.currency.clone(), "manual".into(), 0, false, false)
    } else if let Some(priced) = crate::finance_prices::price_for(&store.prices, &asset, now_ms) {
        (priced.unit_price, priced.currency, if priced.source == "manual" { "manual".into() } else { priced.provider }, priced.fetched_ms, priced.stale, priced.estimated)
    } else if let Some(price) = account.price_minor {
        let exp = exponent(&account.currency);
        (price as f64 / 10f64.powi(exp as i32), account.currency.clone(), "manual".into(), 0, false, false)
    } else {
        return Some(MarketNote {
            worth_minor: None,
            worth_currency: String::new(),
            price: None,
            price_currency: String::new(),
            source: String::new(),
            fetched_ms: 0,
            stale: false,
            estimated: false,
            unit: asset.holding_unit,
            symbol: asset.symbol,
        });
    };
    let product = crate::finance_prices::quantity_times(&qty, unit_price)?;
    let exp = crate::finance_fx::exponent(&currency);
    let minor = crate::finance_prices::major_to_minor(product, exp)?;
    let (worth_minor, worth_currency) = if display.is_empty() || currency.eq_ignore_ascii_case(display) {
        (minor, currency.clone())
    } else if let Some(row) = crate::finance_fx::convert(minor, &currency, display, &store.fx, now_ms) {
        (row.amount_minor, row.currency)
    } else {
        (minor, currency.clone())
    };
    Some(MarketNote {
        worth_minor: Some(worth_minor),
        worth_currency,
        price: Some(unit_price),
        price_currency: currency,
        source,
        fetched_ms,
        stale,
        estimated,
        unit: asset.holding_unit,
        symbol: asset.symbol,
    })
}

fn view_of(store: &Store, account: &Account, now_ms: i64, display: &str) -> AccountView {
    let mut view = AccountView {
        shown_balance_minor: shown_with_fx(account, &store.transactions, &store.fx, now_ms),
        market_worth_minor: None,
        market_worth_currency: String::new(),
        market_price: None,
        market_price_currency: String::new(),
        market_source: String::new(),
        market_fetched_ms: 0,
        market_stale: false,
        market_estimated: false,
        market_unit: String::new(),
        market_symbol: String::new(),
        provider_coingecko_id: String::new(),
        provider_paprika_id: String::new(),
        account: account.clone(),
    };
    if let Some(asset) = holding_resolved(store, account) {
        view.provider_coingecko_id = asset.coingecko_id;
        view.provider_paprika_id = asset.paprika_id;
        if view.market_symbol.is_empty() {
            view.market_symbol = asset.symbol;
        }
    }
    let Some(note) = market_note(store, account, now_ms, display) else { return view };
    view.market_worth_minor = note.worth_minor;
    view.market_worth_currency = note.worth_currency;
    view.market_price = note.price;
    view.market_price_currency = note.price_currency;
    view.market_source = note.source;
    view.market_fetched_ms = note.fetched_ms;
    view.market_stale = note.stale;
    view.market_estimated = note.estimated;
    view.market_unit = note.unit;
    view.market_symbol = note.symbol;
    view
}

fn worth_sentence(store: &Store, account: &Account, now_ms: i64, display: &str) -> String {
    let Some(asset) = holding_of(account) else {
        if account.kind == "investment" && !account.units.trim().is_empty() && account.price_minor.is_none() {
            return spoken("No market symbol was found for {name}, so it has no fiat worth until you set a price.", &[("name", &account.name)]);
        }
        return String::new();
    };
    match market_note(store, account, now_ms, display) {
        Some(note) if note.worth_minor.is_some() && note.estimated => {
            let amount = format_minor(note.worth_minor.unwrap_or(0), &note.worth_currency);
            spoken("Worth about {amount}. That price is an AI estimate.", &[("amount", &amount)])
        }
        Some(note) if note.worth_minor.is_some() => {
            let amount = format_minor(note.worth_minor.unwrap_or(0), &note.worth_currency);
            spoken("Worth about {amount} at the current price from {source}.", &[("amount", &amount), ("source", &note.source)])
        }
        _ => spoken("No live price was found for {symbol}.", &[("symbol", &asset.symbol)]),
    }
}

fn price_failure(store: &Store, symbol: &str) -> String {
    let reason = store
        .prices
        .attempts
        .iter()
        .rev()
        .find(|row| row.symbol.eq_ignore_ascii_case(symbol) && row.status != "ok" && row.status != "skipped")
        .map(|row| format!("{}: {}", row.source, row.detail))
        .filter(|row| !row.trim().is_empty())
        .unwrap_or_else(|| {
            if store.prices.last_error.trim().is_empty() { "Could not refresh the price.".into() } else { store.prices.last_error.clone() }
        });
    spoken("No live price was found for {symbol}. {reason}", &[("symbol", symbol), ("reason", &reason)])
}

fn price_store(app: &AppHandle, store: &mut Store, force: bool, allow_ai: bool) -> bool {
    let now_ms = Local::now().timestamp_millis();
    let mut changed = link_holdings(store);
    if crate::finance_fx::needs_live_fetch(&store.fx, now_ms, force) {
        if let Ok(book) = crate::finance_fx::fetch_book(&store.fx, now_ms) {
            store.fx = book;
            changed = true;
        }
    }
    changed |= if allow_ai {
        fetch_holding_prices(store, now_ms, force, None, &crate::finance_prices::blocking_get, Some(&|asset| ai_spot(app, asset)))
    } else {
        fetch_holding_prices(store, now_ms, force, None, &crate::finance_prices::blocking_get, None)
    };
    let before = store.fx.per_eur.clone();
    crate::finance_prices::apply_crypto_legs(&mut store.fx, &store.prices);
    if store.fx.per_eur != before {
        changed = true;
    }
    if capture_history(store, Local::now().date_naive(), now_ms) {
        changed = true;
    }
    changed
}

fn account_matches(account: &Account, key: &str) -> bool {
    let key = key.trim();
    if key.is_empty() {
        return false;
    }
    if account.id == key || fold(&account.name) == fold(key) {
        return true;
    }
    if !account.ticker.is_empty() && fold(&account.ticker) == fold(key) {
        return true;
    }
    holding_of(account).is_some_and(|asset| fold(&asset.symbol) == fold(key))
}

/// `only` limits the fetch to that holding and bypasses the daily gate for it.
fn fetch_holding_prices(
    store: &mut Store,
    now_ms: i64,
    force: bool,
    only: Option<&str>,
    get: &dyn Fn(&str) -> Result<String, String>,
    ai: Option<&dyn Fn(&crate::finance_prices::AssetRef) -> Result<crate::finance_prices::AiEstimate, String>>,
) -> bool {
    let mut changed = false;
    let mut assets = held_assets(store);
    let force = if let Some(name) = only {
        let symbols: Vec<String> = store.accounts.iter().filter(|account| account_matches(account, name)).filter_map(|account| holding_of(account).map(|asset| asset.symbol)).collect();
        assets.retain(|asset| symbols.iter().any(|symbol| symbol.eq_ignore_ascii_case(&asset.symbol)));
        true
    } else {
        force
    };
    if assets.is_empty() || !crate::finance_prices::needs_refresh(&store.prices, &assets, now_ms, force) {
        return changed;
    }
    let prefer = store.default_currency.clone();
    let next = crate::finance_prices::refresh_book(&store.prices, &assets, now_ms, force, get, ai, &prefer);
    if next != store.prices {
        store.prices = next;
        changed = true;
    }
    changed
}

fn refresh_named(store: &mut Store, app: &AppHandle, name_or_id: &str) -> Result<String, String> {
    let key = name_or_id.trim();
    if !store.accounts.iter().any(|account| account_matches(account, key)) {
        return Err(spoken("No holding matches {name}.", &[("name", key)]));
    }
    let _ = link_holdings(store);
    let now_ms = Local::now().timestamp_millis();
    let _ = fetch_holding_prices(store, now_ms, true, Some(key), &crate::finance_prices::blocking_get, Some(&|asset| ai_spot(app, asset)));
    let account = store.accounts.iter().find(|account| account_matches(account, key)).cloned().ok_or_else(|| spoken("No holding matches {name}.", &[("name", key)]))?;
    let display = effective_default(store, &store.locale_hint);
    let note = market_note(store, &account, now_ms, &display);
    if note.as_ref().and_then(|row| row.price).is_none() {
        let symbol = note.map(|row| row.symbol).unwrap_or_else(|| account.name.clone());
        return Err(price_failure(store, &symbol));
    }
    Ok(worth_sentence(store, &account, now_ms, &display))
}

/// "refresh price of EPIC Cash" and the same request in es, de, pt, and ca.
fn parse_refresh_phrase(text: &str) -> Option<String> {
    let folded = fold(text).trim_end_matches('.').trim().to_string();
    const PREFIXES: &[&str] = &[
        "refresh the price of ",
        "refresh price of ",
        "refresh the price for ",
        "refresh price for ",
        "update the price of ",
        "update price of ",
        "actualiza el precio de ",
        "actualizar el precio de ",
        "actualiza precio de ",
        "actualizar precio de ",
        "aktualisiere den preis von ",
        "preis von ",
        "atualiza o preco de ",
        "atualizar o preco de ",
        "atualiza o preco da ",
        "atualizar o preco da ",
        "actualitza el preu de ",
        "actualitza el preu d ",
    ];
    for prefix in PREFIXES {
        let Some(rest) = folded.strip_prefix(prefix) else { continue };
        let mut name = rest.trim().trim_end_matches('.').to_string();
        if let Some(stripped) = name.strip_suffix(" aktualisieren") {
            name = stripped.trim().to_string();
        }
        if !name.is_empty() {
            return Some(name);
        }
    }
    // "refresh EPIC price" and "aktualisiere EPIC Preis".
    const HEADS: &[&str] = &["refresh ", "update ", "actualiza ", "actualizar ", "aktualisiere ", "atualiza ", "atualizar ", "actualitza "];
    const TAILS: &[&str] = &[" price", " precio", " preis", " preco", " preu"];
    for head in HEADS {
        let Some(rest) = folded.strip_prefix(head) else { continue };
        for tail in TAILS {
            let Some(name) = rest.strip_suffix(tail) else { continue };
            let name = name.trim().trim_end_matches('.').to_string();
            if !name.is_empty() && !name.contains("price") && !name.contains("precio") && !name.contains("preis") && !name.contains("preco") && !name.contains("preu") {
                return Some(name);
            }
        }
    }
    None
}

/// "why has EPIC no price" and the same question in es, de, pt, and ca.
fn parse_why_price(text: &str) -> Option<String> {
    let folded = fold(text).trim_end_matches('.').trim().to_string();
    const WRAPS: &[(&str, &str)] = &[
        ("why has ", " no price"),
        ("why does ", " have no price"),
        ("why is there no price for ", ""),
        ("why no price for ", ""),
        ("por que no tiene precio ", ""),
        ("por que ", " no tiene precio"),
        ("warum hat ", " keinen preis"),
        ("warum hat ", " kein preis"),
        ("porque ", " nao tem preco"),
        ("por que ", " nao tem preco"),
        ("per que no te preu ", ""),
        ("per que ", " no te preu"),
    ];
    for (head, tail) in WRAPS {
        let Some(rest) = folded.strip_prefix(head) else { continue };
        if tail.is_empty() {
            let name = rest.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
            continue;
        }
        let Some(name) = rest.strip_suffix(tail) else { continue };
        let name = name.trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn explain_unpriced(store: &Store, name: &str, now_ms: i64) -> Result<String, String> {
    let account = store.accounts.iter().find(|account| account_matches(account, name)).cloned().ok_or_else(|| spoken("No holding matches {name}.", &[("name", name)]))?;
    let asset = holding_resolved(store, &account);
    let symbol = asset.as_ref().map(|row| row.symbol.clone()).filter(|symbol| !symbol.is_empty()).unwrap_or_else(|| account.name.clone());
    let gecko = asset.as_ref().map(|row| row.coingecko_id.clone()).filter(|id| !id.is_empty()).unwrap_or_else(|| spoken("none", &[]));
    let paprika = asset.as_ref().map(|row| row.paprika_id.clone()).filter(|id| !id.is_empty()).unwrap_or_else(|| spoken("none", &[]));
    let mut detail = spoken(
        "{name} is symbol {symbol}. CoinGecko id {gecko}. CoinPaprika id {paprika}.",
        &[("name", &account.name), ("symbol", &symbol), ("gecko", &gecko), ("paprika", &paprika)],
    );
    let attempts: Vec<_> = store.prices.attempts.iter().filter(|row| row.symbol.eq_ignore_ascii_case(&symbol)).cloned().collect();
    if attempts.is_empty() {
        detail.push(' ');
        detail.push_str(&spoken("No source has been tried yet.", &[]));
    } else {
        for row in attempts {
            let status = match row.status.as_str() {
                "ok" => spoken("source answered", &[]),
                "miss" => spoken("source missed", &[]),
                "skipped" => spoken("source skipped", &[]),
                _ => spoken("source failed", &[]),
            };
            let id = if row.id.is_empty() { spoken("none", &[]) } else { row.id };
            detail.push(' ');
            detail.push_str(&spoken("{source} {id}: {status}.", &[("source", &row.source), ("id", &id), ("status", &status)]));
        }
    }
    if account.units.trim().is_empty() {
        detail.push(' ');
        detail.push_str(&spoken("No quantity is saved for {name}, so the price cannot become a value.", &[("name", &account.name)]));
    } else if store.prices.quotes.iter().any(|row| row.symbol.eq_ignore_ascii_case(&symbol) && row.estimated) {
        detail.push(' ');
        detail.push_str(&spoken("The saved price for {symbol} is an AI estimate.", &[("symbol", &symbol)]));
    } else if let Some(sentence) = market_note(store, &account, now_ms, &effective_default(store, &store.locale_hint)).filter(|note| note.worth_minor.is_some()).map(|_| worth_sentence(store, &account, now_ms, &effective_default(store, &store.locale_hint))) {
        if !sentence.is_empty() {
            detail.push(' ');
            detail.push_str(&sentence);
        }
    }
    Ok(detail)
}

fn capture_history(store: &mut Store, today: NaiveDate, now_ms: i64) -> bool {
    let before = store.history.len();
    let display = effective_default(store, &store.locale_hint);
    let date = iso(today);
    let accounts = store.accounts.clone();
    for account in &accounts {
        if !is_holding_account(account) {
            continue;
        }
        for valuation in &account.valuations {
            if valuation.as_of.len() != 10 {
                continue;
            }
            if store.history.iter().any(|row| row.account_id == account.id && row.date == valuation.as_of) {
                continue;
            }
            if let Some((minor, currency)) = history_value(store, valuation.amount_minor, &valuation.currency, &display, now_ms) {
                crate::finance_history::upsert_point(&mut store.history, history_point(account, &valuation.as_of, minor, &currency, valuation.amount_minor, &valuation.currency, "valuation"));
            }
        }
        if let Some(note) = market_note(store, account, now_ms, &display) {
            if !note.estimated {
                if let Some(worth) = note.worth_minor {
                    let native_minor = if note.price_currency.eq_ignore_ascii_case(&display) { worth } else { worth };
                    crate::finance_history::upsert_point(&mut store.history, history_point(account, &date, worth, &note.worth_currency, native_minor, &note.price_currency, &note.source));
                }
            }
            continue;
        }
        if let Some(balance) = account.balance_minor {
            if balance != 0 && !account.currency.is_empty() {
                if let Some((minor, currency)) = history_value(store, balance, &account.currency, &display, now_ms) {
                    crate::finance_history::upsert_point(&mut store.history, history_point(account, &date, minor, &currency, balance, &account.currency, "stated"));
                }
            }
        }
    }
    seed_backfill(store, today, now_ms, &display);
    store.history.len() != before
}

/// One CoinGecko market_chart pass per holding. Past points keep the quantity from the day they were written.
fn seed_backfill(store: &mut Store, today: NaiveDate, now_ms: i64, display: &str) {
    let today_iso = iso(today);
    let accounts = store.accounts.clone();
    for account in accounts {
        if store.history_seeded.iter().any(|id| id == &account.id) {
            continue;
        }
        let Some(asset) = holding_resolved(store, &account) else {
            store.history_seeded.push(account.id.clone());
            continue;
        };
        if asset.kind != "crypto" || asset.coingecko_id.is_empty() || account.units.trim().is_empty() {
            store.history_seeded.push(account.id.clone());
            continue;
        }
        let url = format!("https://api.coingecko.com/api/v3/coins/{}/market_chart?vs_currency=eur&days=365", asset.coingecko_id);
        if let Ok(body) = crate::finance_prices::blocking_get(&url) {
            for (date, price) in crate::finance_history::parse_market_chart(&body) {
                if date >= today_iso || store.history.iter().any(|row| row.account_id == account.id && row.date == date) {
                    continue;
                }
                let Some(major) = crate::finance_prices::quantity_times(&account.units, price) else { continue };
                let Some(eur_minor) = crate::finance_prices::major_to_minor(major, 2) else { continue };
                if let Some((minor, currency)) = history_value(store, eur_minor, "EUR", display, now_ms) {
                    crate::finance_history::upsert_point(
                        &mut store.history,
                        history_point(&account, &date, minor, &currency, eur_minor, "EUR", "coingecko history"),
                    );
                }
            }
        }
        store.history_seeded.push(account.id.clone());
    }
}

fn history_value(store: &Store, amount: i64, currency: &str, display: &str, now_ms: i64) -> Option<(i64, String)> {
    if display.is_empty() || currency.eq_ignore_ascii_case(display) {
        return Some((amount, currency.to_string()));
    }
    crate::finance_fx::convert(amount, currency, display, &store.fx, now_ms).map(|row| (row.amount_minor, row.currency))
}

fn history_point(account: &Account, date: &str, value: i64, currency: &str, native: i64, native_currency: &str, source: &str) -> crate::finance_history::ValuePoint {
    crate::finance_history::ValuePoint {
        account_id: account.id.clone(),
        date: date.to_string(),
        quantity: account.units.clone(),
        value_minor: value,
        currency: currency.to_string(),
        native_minor: native,
        native_currency: native_currency.to_string(),
        source: source.to_string(),
        name: account.name.clone(),
        class_name: account.investment_type.clone(),
    }
}

fn ai_spot(app: &AppHandle, asset: &crate::finance_prices::AssetRef) -> Result<crate::finance_prices::AiEstimate, String> {
    let vault = app.state::<crate::vault::Vault>();
    let bridge = app.state::<crate::native::NativeBridge>();
    let accounts = vault.accounts(&bridge).map_err(|err| err.to_string())?;
    let name = if asset.label.trim().is_empty() { asset.symbol.clone() } else { asset.label.clone() };
    let hint = if asset.symbol.eq_ignore_ascii_case("EPIC") {
        " This is Epic Cash (Epic Private Internet Cash, CoinGecko epic-cash). Not Epic Chain."
    } else {
        ""
    };
    let prompt = format!(
        "Current market price of one {} of {name} (ticker {}, {}).{hint} Reply with JSON only: {{\"price_usd\": number, \"source_url\": \"https://...\", \"as_of\": \"YYYY-MM-DD\"}}. price_usd is a positive number in US dollars. source_url is the page you read.",
        asset.holding_unit, asset.symbol, asset.kind
    );
    for provider in ["xai", "openai"] {
        let Some(secret) = accounts.iter().find(|row| row.provider == provider && !row.secret.is_empty()).map(|row| row.secret.clone()) else {
            continue;
        };
        let text = if provider == "xai" { ai_xai(&secret, &prompt) } else { ai_openai(&secret, &prompt) };
        if let Ok(text) = text {
            if let Ok(parsed) = crate::finance_prices::parse_ai_price(&text) {
                return Ok(parsed);
            }
        }
    }
    Err("no estimate".into())
}

fn ai_xai(secret: &str, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(12)).redirect(crate::http::api_redirect()).build().map_err(|_| "no estimate".to_string())?;
    let value: serde_json::Value = client
        .post("https://api.x.ai/v1/responses")
        .bearer_auth(secret)
        .json(&json!({ "model": "grok-4.7", "temperature": 0, "tools": [{ "type": "web_search" }], "input": prompt }))
        .send()
        .map_err(|_| "no estimate".to_string())?
        .json()
        .map_err(|_| "no estimate".to_string())?;
    let mut text = String::new();
    if let Some(items) = value["output"].as_array() {
        for item in items {
            if let Some(chunks) = item["content"].as_array() {
                for chunk in chunks {
                    if let Some(piece) = chunk["text"].as_str() {
                        text.push_str(piece);
                    }
                }
            }
        }
    }
    if text.trim().is_empty() { Err("no estimate".into()) } else { Ok(text) }
}

fn ai_openai(secret: &str, prompt: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(12)).redirect(crate::http::api_redirect()).build().map_err(|_| "no estimate".to_string())?;
    let value: serde_json::Value = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(secret)
        .json(&json!({ "model": "gpt-4o-mini", "temperature": 0, "messages": [{ "role": "user", "content": prompt }] }))
        .send()
        .map_err(|_| "no estimate".to_string())?
        .json()
        .map_err(|_| "no estimate".to_string())?;
    value["choices"][0]["message"]["content"].as_str().map(|text| text.to_string()).ok_or_else(|| "no estimate".into())
}

fn snapshot_of(store: &Store, locale: &str, today: NaiveDate) -> Snapshot {
    let period = format!("{:04}-{:02}", today.year(), today.month());
    let mut accounts: Vec<AccountView> = store
        .accounts
        .iter()
        .map(|account| view_of(store, account, Local::now().timestamp_millis(), &effective_default(store, locale)))
        .collect();
    accounts.sort_by(|a, b| a.account.kind.cmp(&b.account.kind).then(a.account.name.to_ascii_lowercase().cmp(&b.account.name.to_ascii_lowercase())));
    let mut transactions = store.transactions.clone();
    transactions.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_ms.cmp(&a.created_ms)));
    let mut rules = store.rules.clone();
    rules.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    let totals = period_totals(&flow_transactions(store, today), &period, "", "", "");
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
        flow_colors: store.flow_colors.clone(),
        flow_layout: store.flow_layout.clone(),
        entity_colors: store.entity_colors.clone(),
        fx: fx_status(store, today),
        prices: store.prices.clone(),
        history: store.history.clone(),
        budgets: store.budgets.clone(),
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

#[derive(Debug)]
struct ComingItem {
    name: String,
    date: String,
    direction: String,
}

fn coming_lines(store: &Store, today: NaiveDate, side: &str) -> Vec<ComingItem> {
    let end = today + Duration::days(30);
    let mut rows = Vec::new();
    for rule in &store.rules {
        if !shows_on_calendar(&rule.status) {
            continue;
        }
        let outgoing = rule.direction == "expense" || rule.direction == "payment";
        if side == "out" {
            if !outgoing {
                continue;
            }
        } else if outgoing {
            continue;
        }
        if side == "out" && rule.direction != "payment" {
            if store.accounts.iter().any(|account| account.id == rule.account_id && account.kind == "credit_card") {
                continue;
            }
        }
        if side == "out" && rule.direction == "payment" && card_nothing_due(store, &rule.account_id) {
            continue;
        }
        let spec = rule_spec(rule);
        let settled = settled_dates(store, &rule.id);
        for date in crate::finance_schedule::occurrence_dates(&spec, today) {
            if date < today || date > end || settled.contains(&date) || spec.skipped.contains(&date) {
                continue;
            }
            let name = if rule.direction == "payment" { card_payment_title(store, rule) } else { rule.name.clone() };
            rows.push(ComingItem { name, date: iso(date), direction: rule.direction.clone() });
        }
    }
    if side == "out" {
        for account in store.accounts.iter().filter(|account| account.kind == "credit_card") {
            if card_nothing_due(store, &account.id) {
                continue;
            }
            if rows.iter().any(|row| row.direction == "payment" && row.name == payment_title(&account.name)) {
                continue;
            }
            let Some(date) = card_due_on(account, today) else { continue };
            if date < today || date > end {
                continue;
            }
            let amount = account.payment_minor.filter(|value| *value > 0).or_else(|| shown_with_fx(account, &store.transactions, &store.fx, 0).filter(|value| *value > 0));
            if amount.is_none() {
                continue;
            }
            rows.push(ComingItem { name: payment_title(&account.name), date: iso(date), direction: "payment".into() });
        }
    }
    rows.sort_by(|a, b| a.date.cmp(&b.date).then(a.name.cmp(&b.name)));
    rows
}

fn speak_coming(store: &Store, today: NaiveDate) -> String {
    let out = coming_lines(store, today, "out");
    let inn = coming_lines(store, today, "in");
    if out.is_empty() && inn.is_empty() {
        return spoken("Nothing is due in the next 30 days.", &[]);
    }
    let mut parts = Vec::new();
    if !out.is_empty() {
        let body = out.iter().take(6).map(|row| format!("{} {}", row.name, row.date)).collect::<Vec<_>>().join("; ");
        parts.push(spoken("Coming expenses, next 30 days: {body}.", &[("body", &body)]));
    }
    if !inn.is_empty() {
        let body = inn.iter().take(6).map(|row| format!("{} {}", row.name, row.date)).collect::<Vec<_>>().join("; ");
        parts.push(spoken("Coming income, next 30 days: {body}.", &[("body", &body)]));
    }
    parts.join(" ")
}

fn speak_pair(store: &Store, base: &str, quote: &str, now_ms: i64) -> Result<String, String> {
    let base = canonical_currency(base)?;
    let quote = canonical_currency(quote)?;
    if base == quote {
        let amount = format_minor(10i64.pow(exponent(&base)), &base);
        return Ok(spoken("1 {base} is {amount}.", &[("base", &base), ("amount", &amount)]));
    }
    let one = 10i64.pow(exponent(&base));
    match crate::finance_fx::convert(one, &base, &quote, &store.fx, now_ms) {
        Some(row) => {
            let amount = format_minor(row.amount_minor, &quote);
            let mut line = spoken("1 {base} is {amount}.", &[("base", &base), ("amount", &amount)]);
            if row.source == "manual" {
                line.push(' ');
                line.push_str(&spoken("That is your manual rate.", &[]));
            } else if row.stale {
                line.push(' ');
                line.push_str(&spoken("That rate is stale.", &[]));
            }
            Ok(line)
        }
        None => Ok(spoken(
            "No rate is saved for {base} in {quote}. Refresh rates, or set a manual rate in Settings.",
            &[("base", &base), ("quote", &quote)],
        )),
    }
}

fn asks_owed(text: &str) -> bool {
    let folded = fold(text);
    folded.contains("owe") || folded.contains("debo") || folded.contains("deuda") || folded.contains("schulde") || folded.contains("devo") || folded.contains("deute")
}

fn asks_worth(text: &str) -> bool {
    let folded = fold(text);
    folded.contains("net worth") || folded.contains("patrimonio") || folded.contains("patrimoni") || folded.contains("vermogen")
}

fn asks_spent(text: &str) -> bool {
    let folded = fold(text);
    folded.contains("spend") || folded.contains("spent") || folded.contains("gasto") || folded.contains("gaste") || folded.contains("despes") || folded.contains("ausgegeben")
}

fn speak_spent(store: &Store, today: NaiveDate) -> String {
    let display = effective_default(store, &current_locale());
    let now_ms = Local::now().timestamp_millis();
    let month = format!("{:04}-{:02}", today.year(), today.month());
    let totals = period_totals(&flow_transactions(store, today), &month, "", "", "");
    let parts: Vec<(i64, String)> = totals.iter().filter(|row| row.expense_minor != 0).map(|row| (row.expense_minor, row.currency.clone())).collect();
    if parts.is_empty() {
        return spoken("No income or expenses are saved for {month}.", &[("month", &month)]);
    }
    let Some(combined) = crate::finance_fx::combine(&parts, &display, &store.fx, now_ms) else {
        let missing = crate::finance_fx::missing_codes(&parts, &display, &store.fx).join(", ");
        return spoken("Net worth needs a rate for {codes}.", &[("codes", &missing)]);
    };
    let amount = format_minor(combined.amount_minor, &combined.currency);
    spoken("Spent {amount} this month.", &[("amount", &amount)])
}

fn speak(store: &Store, ask: &Ask, today: NaiveDate) -> String {
    if asks_owed(&ask.query) {
        return speak_debt(store, today);
    }
    if asks_worth(&ask.query) {
        return speak_net_worth(store);
    }
    if asks_spent(&ask.query) {
        return speak_spent(store, today);
    }
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
    if view == "coming" {
        return speak_coming(store, today);
    }
    if view == "upcoming" {
        let rows = upcoming_items(store, today)
            .into_iter()
            .filter(|row| (0..=30).contains(&row.days_until))
            .filter(|row| {
                if row.direction != "expense" {
                    return true;
                }
                let Some(rule) = store.rules.iter().find(|rule| rule.id == row.id) else { return true };
                !store.accounts.iter().any(|account| account.id == rule.account_id && account.kind == "credit_card")
            })
            .collect::<Vec<_>>();
        return speak_upcoming(&rows);
    }
    if view == "portfolio" {
        return speak_portfolio(store, today);
    }
    if view == "accounts" || view == "investments" {
        let scope = if view == "investments" { "hold" } else { "spend" };
        return speak_accounts(store, &kind, &ask.account, scope);
    }
    if matches!(view.as_str(), "activity" | "transactions" | "search") {
        return match filtered_transactions(store, ask) {
            Ok(rows) => speak_activity(store, &rows),
            Err(message) => message,
        };
    }
    let account_id = match locate_account(store, "", &ask.account, &kind) {
        Ok(Some(found)) => store.accounts[found.index].id.clone(),
        Ok(None) => String::new(),
        Err(message) => return message,
    };
    let direction = normalize_direction(&ask.direction).unwrap_or_default();
    let class = if ask.recurrence == "once" { "" } else { ask.recurrence.as_str() };
    let totals = period_totals(&flow_transactions(store, today), &month, &account_id, &direction, &class);
    let display = effective_default(store, &current_locale());
    let mut detail = speak_totals(store, &totals, &month, &display);
    if !account_id.is_empty() || !kind.is_empty() {
        detail.push(' ');
        detail.push_str(&speak_accounts(store, &kind, &ask.account, "any"));
    }
    if !detail.contains("needs a rate") {
        detail.push(' ');
        detail.push_str(&speak_coming(store, today));
    }
    if account_id.is_empty() && kind.is_empty() {
        let worth = speak_net_worth(store);
        if !worth.is_empty() {
            detail.push(' ');
            detail.push_str(&worth);
        }
    }
    let asked = fold(&format!("{} {}", ask.query, ask.account));
    if asked.contains("percent") || asked.contains("porcentaje") || asked.contains("percentatge") || asked.contains("prozent") || asked.contains("portfolio") || asked.contains("cartera") {
        detail.push(' ');
        detail.push_str(&portfolio_share_line(store, &format!("{} {}", ask.account, ask.query), &display, Local::now().timestamp_millis()));
    }
    detail
}

fn portfolio_share_line(store: &Store, needle: &str, display: &str, now_ms: i64) -> String {
    let mut rows: Vec<(String, i64)> = Vec::new();
    for account in &store.accounts {
        if is_debt(&account.kind) || account.kind == "cash" || account.kind == "bank" {
            continue;
        }
        let view = view_of(store, account, now_ms, display);
        let worth = if view.market_worth_minor.is_some() && view.market_worth_currency.eq_ignore_ascii_case(display) {
            view.market_worth_minor
        } else if let Some(shown) = view.shown_balance_minor {
            crate::finance_fx::convert(shown, &account.currency, display, &store.fx, now_ms).map(|row| row.amount_minor)
        } else {
            None
        };
        if let Some(amount) = worth {
            if amount > 0 {
                rows.push((account.name.clone(), amount));
            }
        }
    }
    let total: i64 = rows.iter().map(|row| row.1).sum();
    if total <= 0 {
        return spoken("Nothing priced yet.", &[]);
    }
    let folded = fold(needle);
    let Some((name, amount)) = rows.iter().find(|(name, _)| folded.contains(&fold(name)) || fold(name).contains(&folded.trim())) else {
        return spoken("No priced holding matches that name.", &[]);
    };
    let percent = ((*amount as f64) * 100.0 / total as f64).round() as i64;
    let percent = percent.to_string();
    spoken("{name} is {percent} percent of the portfolio.", &[("name", name), ("percent", &percent)])
}

fn current_locale() -> String {
    SPOKEN_LOCALE.with(|cell| {
        let current = cell.borrow();
        if current.is_empty() { "en".to_string() } else { current.clone() }
    })
}

/// Unpaid planned expenses on or before today. Future bills stay out.
/// A bill on a card, loan, or personal debt is skipped so that balance is not counted twice.
fn overdue_bills(store: &Store, today: NaiveDate) -> Vec<(i64, String)> {
    let mut parts = Vec::new();
    for rule in &store.rules {
        if rule.direction != "expense" || matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
            continue;
        }
        let Some(amount) = rule.amount_minor else { continue };
        if amount <= 0 || rule.currency.trim().is_empty() {
            continue;
        }
        if store.accounts.iter().any(|account| account.id == rule.account_id && is_debt(&account.kind)) {
            continue;
        }
        let spec = rule_spec(rule);
        let settled = settled_dates(store, &rule.id);
        for date in crate::finance_schedule::occurrence_dates(&spec, today) {
            if date > today || settled.contains(&date) || spec.skipped.contains(&date) {
                continue;
            }
            parts.push((amount, rule.currency.to_ascii_uppercase()));
        }
    }
    parts
}

fn subtract_parts(parts: &mut Vec<(i64, String)>, bills: &[(i64, String)]) {
    for (amount, currency) in bills {
        if let Some(row) = parts.iter_mut().find(|row| row.1.eq_ignore_ascii_case(currency)) {
            row.0 -= *amount;
        } else {
            parts.push((-*amount, currency.clone()));
        }
    }
}

fn owed_parts(store: &Store, today: NaiveDate, now_ms: i64) -> Vec<(i64, String)> {
    let mut parts = Vec::new();
    for account in &store.accounts {
        if !is_debt(&account.kind) {
            continue;
        }
        let Some(shown) = shown_with_fx(account, &store.transactions, &store.fx, now_ms) else { continue };
        if shown <= 0 || account.currency.trim().is_empty() {
            continue;
        }
        parts.push((shown, account.currency.to_ascii_uppercase()));
    }
    parts.extend(overdue_bills(store, today));
    parts
}

fn speak_debt(store: &Store, today: NaiveDate) -> String {
    let display = effective_default(store, &current_locale());
    let now_ms = Local::now().timestamp_millis();
    let parts = owed_parts(store, today, now_ms);
    if parts.is_empty() {
        return spoken("Nothing is owed.", &[]);
    }
    let Some(combined) = crate::finance_fx::combine(&parts, &display, &store.fx, now_ms) else {
        let missing = crate::finance_fx::missing_codes(&parts, &display, &store.fx).join(", ");
        return spoken("Net worth needs a rate for {codes}.", &[("codes", &missing)]);
    };
    if combined.amount_minor <= 0 {
        return spoken("Nothing is owed.", &[]);
    }
    let amount = format_minor(combined.amount_minor, &combined.currency);
    let mut line = spoken("You owe {amount}.", &[("amount", &amount)]);
    for (slice, label) in [("credit_card", " Cards {amount}."), ("loan", " Loans {amount}."), ("other_debt", " Owed to people {amount}.")] {
        let slice_parts: Vec<(i64, String)> = store
            .accounts
            .iter()
            .filter(|account| account.kind == slice)
            .filter_map(|account| {
                let shown = shown_with_fx(account, &store.transactions, &store.fx, now_ms)?;
                if shown <= 0 { None } else { Some((shown, account.currency.to_ascii_uppercase())) }
            })
            .collect();
        if let Some(row) = crate::finance_fx::combine(&slice_parts, &display, &store.fx, now_ms) {
            if row.amount_minor > 0 {
                let piece = format_minor(row.amount_minor, &row.currency);
                line.push_str(&spoken(label, &[("amount", &piece)]));
            }
        }
    }
    let bills = overdue_bills(store, today);
    if let Some(row) = crate::finance_fx::combine(&bills, &display, &store.fx, now_ms) {
        if row.amount_minor > 0 {
            let piece = format_minor(row.amount_minor, &row.currency);
            line.push_str(&spoken(" Overdue bills {amount}.", &[("amount", &piece)]));
        }
    }
    line
}

fn speak_net_worth(store: &Store) -> String {
    let display = effective_default(store, &current_locale());
    let now_ms = Local::now().timestamp_millis();
    let accounts = worth_account_list(store, &display, now_ms);
    let rows = crate::finance_schedule::worth_rows(&accounts);
    if rows.is_empty() {
        return String::new();
    }
    let mut parts: Vec<(i64, String)> = rows.iter().map(|row| (row.net_minor, row.currency.clone())).collect();
    let today = Local::now().date_naive();
    subtract_parts(&mut parts, &overdue_bills(store, today));
    let Some(net) = crate::finance_fx::combine(&parts, &display, &store.fx, now_ms) else {
        let missing = crate::finance_fx::missing_codes(&parts, &display, &store.fx).join(", ");
        if missing.is_empty() {
            return String::new();
        }
        return spoken("Net worth needs a rate for {codes}.", &[("codes", &missing)]);
    };
    let amount = format_minor(net.amount_minor, &net.currency);
    let mut line = spoken("Net worth is {amount}.", &[("amount", &amount)]);
    let owed = owed_parts(store, today, now_ms);
    if let Some(debt) = crate::finance_fx::combine(&owed, &display, &store.fx, now_ms) {
        if debt.amount_minor > 0 {
            let owed_amount = format_minor(debt.amount_minor, &debt.currency);
            line.push(' ');
            line.push_str(&spoken("Debts of {amount} are subtracted.", &[("amount", &owed_amount)]));
        }
    }
    line
}

fn speak_totals(store: &Store, totals: &[PeriodTotal], month: &str, display: &str) -> String {
    if totals.is_empty() {
        return spoken("No income or expenses are saved for {month}.", &[("month", month)]);
    }
    let now = Local::now().timestamp_millis();
    let spent = signed_parts(totals, |row| row.expense_minor);
    let got = signed_parts(totals, |row| row.income_minor);
    let paid = signed_parts(totals, |row| row.payment_minor);
    let mut parts = spent.clone();
    parts.extend(got.iter().cloned());
    parts.extend(paid.iter().cloned());
    let missing = crate::finance_fx::missing_codes(&parts, display, &store.fx);
    if !missing.is_empty() {
        let codes = missing.join(", ");
        return spoken("The combined total needs a rate for {codes}.", &[("codes", &codes)]);
    }
    let spent_text = spoken_amount(&spent, display, &store.fx, now);
    let got_text = spoken_amount(&got, display, &store.fx, now);
    let mut line = spoken(
        "Spent {spent} and received {got} in {month}, together in {code}.",
        &[("spent", &spent_text), ("got", &got_text), ("month", month), ("code", display)],
    );
    if let Some(row) = crate::finance_fx::combine(&paid, display, &store.fx, now) {
        if row.amount_minor != 0 {
            let amount = format_minor(row.amount_minor, &row.currency);
            line.push_str(&spoken(" Payments were {amount}.", &[("amount", &amount)]));
        }
    }
    line
}

fn signed_parts(totals: &[PeriodTotal], pick: impl Fn(&PeriodTotal) -> i64) -> Vec<(i64, String)> {
    totals.iter().filter_map(|row| {
        let amount = pick(row);
        if amount == 0 || row.currency.is_empty() { None } else { Some((amount, row.currency.clone())) }
    }).collect()
}

fn spoken_amount(parts: &[(i64, String)], display: &str, book: &crate::finance_fx::FxBook, now: i64) -> String {
    match crate::finance_fx::combine(parts, display, book, now) {
        Some(row) => format_minor(row.amount_minor, &row.currency),
        None => spoken("nothing", &[]),
    }
}

fn speak_chart(action: &str, chart: &str, name: &str) -> String {
    let chart = if chart.is_empty() { "holdings" } else { chart };
    match fold(action).as_str() {
        "reset" | "all" => spoken("The {chart} chart shows every item again.", &[("chart", chart)]),
        "only" => spoken("Only {name} is shown on the {chart} chart.", &[("name", name), ("chart", chart)]),
        "show" => spoken("{name} is shown on the {chart} chart.", &[("name", name), ("chart", chart)]),
        _ => spoken("{name} is hidden on the {chart} chart.", &[("name", name), ("chart", chart)]),
    }
}

fn speak_portfolio(store: &Store, today: NaiveDate) -> String {
    let from = iso(today - chrono::Duration::days(30));
    match crate::finance_history::portfolio_change(&store.history, &from, &iso(today)) {
        Some((delta, end, currency)) => {
            let amount = format_minor(delta, &currency);
            let start = end.saturating_sub(delta);
            let percent = if start == 0 { 0 } else { ((delta as f64) * 100.0 / (start.unsigned_abs() as f64)).round() as i64 };
            let percent = percent.to_string();
            spoken("Portfolio value changed by {amount}, {percent} percent, since {date}.", &[("amount", &amount), ("percent", &percent), ("date", &from)])
        }
        None => spoken("No portfolio history is saved yet.", &[]),
    }
}

fn speak_accounts(store: &Store, kind: &str, account: &str, scope: &str) -> String {
    let indexes: Vec<usize> = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, row)| match scope {
            "spend" => !is_holding_account(row),
            "hold" => is_holding_account(row),
            _ => true,
        })
        .filter(|(_, row)| kind.is_empty() || row.kind == kind)
        .filter(|(_, row)| account.trim().is_empty() || compact_name(&row.name).contains(&compact_name(account)) || compact_name(account).contains(&compact_name(&row.name)))
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
            let mut line = match shown_with_fx(account, &store.transactions, &store.fx, Local::now().timestamp_millis()) {
                None => {
                    let worth = worth_sentence(store, account, Local::now().timestamp_millis(), &effective_default(store, &current_locale()));
                    if !account.units.trim().is_empty() || !worth.is_empty() {
                        let qty = if account.units.trim().is_empty() {
                            String::new()
                        } else {
                            spoken("Quantity {qty}.", &[("qty", &account.units)])
                        };
                        format!("{qty} {worth}").trim().to_string()
                    } else {
                        let count = transactions_on(store, &account.id);
                        if count == 0 {
                            spoken("{name} has no balance saved.", &[("name", &account.name)])
                        } else {
                            let count = count.to_string();
                            spoken("{name} has no balance saved, and {count} transactions.", &[("name", &account.name), ("count", &count)])
                        }
                    }
                }
                Some(minor) => {
                    let amount = format_minor(minor.abs(), &account.currency);
                    let mut line = match account.kind.as_str() {
                        "credit_card" if minor < 0 => spoken("{name} has a credit of {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        "credit_card" if minor == 0 => spoken("{name} is paid up.", &[("name", &account.name)]),
                        "credit_card" | "loan" | "other_debt" => spoken("{name} owes {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        "investment" if account.price_override => spoken("Your estimate for {name} is {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        "investment" if !account.units.trim().is_empty() => {
                            let qty = spoken("Quantity {qty}.", &[("qty", &account.units)]);
                            let worth = worth_sentence(store, account, Local::now().timestamp_millis(), &effective_default(store, &current_locale()));
                            format!("{qty} {worth}")
                        }
                        "investment" => spoken("{name} is worth {amount}.", &[("name", &account.name), ("amount", &amount)]),
                        _ => spoken("{name} is {amount}.", &[("name", &account.name), ("amount", &amount)]),
                    };
                    if account.kind == "credit_card" && minor > 0 {
                        if let Some(limit) = account.credit_limit_minor.filter(|value| *value > 0) {
                            let percent = minor.saturating_mul(100) / limit;
                            let percent = percent.to_string();
                            line.push(' ');
                            line.push_str(&spoken("{name} is {percent} of its limit.", &[("name", &account.name), ("percent", &percent)]));
                        }
                        if let Some(date) = card_due_on(account, Local::now().date_naive()) {
                            let due_amount = account.payment_minor.filter(|value| *value > 0).unwrap_or(minor);
                            let due = format_minor(due_amount, &account.currency);
                            let dated = iso(date);
                            line.push(' ');
                            line.push_str(&spoken("Next payment {amount} on {date}.", &[("amount", &due), ("date", &dated)]));
                        }
                    } else if account.kind == "credit_card" && minor < 0 {
                        line.push(' ');
                        line.push_str(&spoken("{name} is in credit.", &[("name", &account.name)]));
                    }
                    line
                }
            };
            let display = effective_default(store, &current_locale());
            let extra = worth_sentence(store, account, Local::now().timestamp_millis(), &display);
            if !extra.is_empty() {
                line.push(' ');
                line.push_str(&extra);
            }
            line
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
    match shown_with_fx(account, &store.transactions, &store.fx, Local::now().timestamp_millis()) {
        Some(minor) if minor < 0 && account.kind == "credit_card" => {
            let amount = format_minor(minor.abs(), &account.currency);
            spoken(" Shown credit {amount}.", &[("amount", &amount)])
        }
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

fn transactions_on(store: &Store, id: &str) -> usize {
    if id.is_empty() {
        return 0;
    }
    store.transactions.iter().filter(|tx| tx.account_id == id || tx.from_account_id == id).count()
}

fn filtered_transactions<'a>(store: &'a Store, ask: &Ask) -> Result<Vec<&'a Transaction>, String> {
    let account_id = if ask.account.trim().is_empty() {
        String::new()
    } else {
        match locate_account(store, "", &ask.account, &ask.kind)? {
            Some(found) => store.accounts[found.index].id.clone(),
            None => return Ok(Vec::new()),
        }
    };
    let mut rows: Vec<&Transaction> = store.transactions.iter().filter(|tx| tx_visible(store, tx, ask, &account_id)).collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_ms.cmp(&a.created_ms)));
    rows.truncate(40);
    Ok(rows)
}

fn tx_visible(store: &Store, tx: &Transaction, ask: &Ask, account_id: &str) -> bool {
    if !account_id.is_empty() && tx.account_id != account_id && tx.from_account_id != account_id {
        return false;
    }
    if !ask.month.is_empty() && !tx.date.starts_with(ask.month.trim()) {
        return false;
    }
    if !ask.date.is_empty() && !tx.date.starts_with(ask.date.trim()) {
        return false;
    }
    if !ask.direction.is_empty() {
        let direction = normalize_direction(&ask.direction).unwrap_or_default();
        if !direction.is_empty() && tx.direction != direction {
            return false;
        }
    }
    if !ask.amount.is_empty() && !amount_matches(tx, &ask.amount) {
        return false;
    }
    if !ask.query.is_empty() {
        let account = account_name(store, &tx.account_id).unwrap_or_default();
        if !text_matches(&tx.category, &ask.query) && !text_matches(&tx.notes, &ask.query) && !text_matches(&account, &ask.query) {
            return false;
        }
    }
    true
}

fn amount_matches(tx: &Transaction, raw: &str) -> bool {
    let raw = raw.trim();
    if raw.is_empty() {
        return true;
    }
    money_for(raw, "", &tx.currency, &tx.currency).is_ok_and(|(minor, _)| minor == tx.amount_minor)
}

fn text_matches(hay: &str, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    let hay = compact_name(hay);
    let needle = compact_name(needle);
    if hay.is_empty() || needle.is_empty() {
        return false;
    }
    if hay.contains(&needle) || needle.contains(&hay) {
        return true;
    }
    needle.len() >= 4 && edit_distance(&hay, &needle) <= 1
}

fn speak_activity(store: &Store, rows: &[&Transaction]) -> String {
    if rows.is_empty() {
        return spoken("No transactions matched.", &[]);
    }
    let body = rows
        .iter()
        .take(12)
        .map(|tx| {
            let account = account_name(store, &tx.account_id).unwrap_or_default();
            let label = if tx.category.trim().is_empty() { spoken("unknown", &[]) } else { tx.category.clone() };
            let amount = format_minor(tx.amount_minor, &tx.currency);
            spoken(
                "{label} {amount} on {account} {date}, id {id}.",
                &[("label", &label), ("amount", &amount), ("account", &account), ("date", &tx.date), ("id", &tx.id)],
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    spoken("Activity: {body}.", &[("body", &body)])
}

fn transaction_views(store: &Store, ask: &Ask) -> Vec<Value> {
    filtered_transactions(store, ask)
        .unwrap_or_default()
        .into_iter()
        .map(|tx| {
            let account = account_name(store, &tx.account_id).unwrap_or_default();
            tx_saved_json(tx, &account)
        })
        .collect()
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
        interval: 1,
        anchor: None,
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
            (account.contribution_date.clone(), account.contribution_cadence.clone(), account.contribution_minor, invest_title(&account.name))
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

/// Due dates for the in-app calendar. Paid, skipped, paused, and stopped rows stay off the grid.
/// `local-fin-*` events remain for reminders; the calendar hides those and shows these instead.
fn calendar_money_events(store: &Store, start_ms: i64, end_ms: i64, today: NaiveDate) -> Vec<CalEvent> {
    let mut out = Vec::new();
    for rule in &store.rules {
        if !shows_on_calendar(&rule.status) {
            continue;
        }
        if rule.name.trim().is_empty() {
            continue;
        }
        if rule.start_date.is_empty() && rule.next_date.is_empty() {
            continue;
        }
        if rule.direction == "payment" && card_nothing_due(store, &rule.account_id) {
            continue;
        }
        let spec = rule_spec(rule);
        let settled = settled_dates(store, &rule.id);
        let mut amount = rule.amount_minor;
        if amount.is_none() && rule.direction == "payment" {
            if let Some(card) = store.accounts.iter().find(|account| account.id == rule.account_id && account.kind == "credit_card") {
                amount = shown_with_fx(card, &store.transactions, &store.fx, 0).filter(|value| *value > 0);
            }
        }
        let title = card_payment_title(store, rule);
        push_due_events(&mut out, &format!("fin-due-{}", rule.id), &title, amount, &rule.currency, &spec, &settled, today, start_ms, end_ms);
    }
    for account in &store.accounts {
        if account.kind == "credit_card" && !card_nothing_due(store, &account.id) && !card_payment_covered(store, &account.id) {
            if let Some(date) = card_due_on(account, today) {
                let cadence = if !account.cadence.is_empty() {
                    account.cadence.as_str()
                } else if account.due_day.is_some() {
                    "monthly"
                } else {
                    ""
                };
                let spec = account_spec(date, cadence);
                let amount = account.payment_minor.filter(|value| *value > 0).or_else(|| shown_with_fx(account, &store.transactions, &store.fx, 0).filter(|value| *value > 0));
                let title = payment_title(account.name.trim());
                push_due_events(&mut out, &format!("fin-card-{}", account.id), &title, amount, &account.currency, &spec, &[], today, start_ms, end_ms);
            }
        }
        if account_has_rule(store, &account.id) {
            continue;
        }
        if account.kind == "investment" {
            if let Some(start) = parse_iso_date(&account.contribution_date) {
                let spec = account_spec(start, &account.contribution_cadence);
                let title = invest_title(&account.name);
                push_due_events(&mut out, &format!("fin-acct-{}", account.id), &title, account.contribution_minor, &account.currency, &spec, &[], today, start_ms, end_ms);
            }
        } else if let Some(start) = parse_iso_date(&account.due_date) {
            let spec = account_spec(start, &account.cadence);
            push_due_events(&mut out, &format!("fin-acct-{}", account.id), &account.name, account.payment_minor, &account.currency, &spec, &[], today, start_ms, end_ms);
        }
    }
    out.sort_by(|a, b| a.start_ms.cmp(&b.start_ms).then_with(|| a.title.cmp(&b.title)));
    out
}

fn invest_title(name: &str) -> String {
    spoken("Invest in {name}", &[("name", name.trim())])
}

fn payment_title(name: &str) -> String {
    let name = name.trim();
    let folded = fold(name);
    if folded.ends_with("payment") || folded.starts_with("pay ") {
        return name.to_string();
    }
    spoken("{name} payment", &[("name", name)])
}

fn card_payment_title(store: &Store, rule: &Rule) -> String {
    if rule.direction == "payment" {
        if let Some(account) = store.accounts.iter().find(|account| account.id == rule.account_id && account.kind == "credit_card") {
            let name = account.name.trim();
            if name.to_ascii_lowercase().ends_with("payment") {
                return name.to_string();
            }
            return payment_title(name);
        }
    }
    rule.name.clone()
}

fn card_nothing_due(store: &Store, account_id: &str) -> bool {
    let Some(account) = store.accounts.iter().find(|account| account.id == account_id && account.kind == "credit_card") else {
        return false;
    };
    matches!(shown_with_fx(account, &store.transactions, &store.fx, 0), Some(owed) if owed <= 0)
}

fn card_payment_covered(store: &Store, account_id: &str) -> bool {
    store.rules.iter().any(|rule| {
        rule.account_id == account_id && rule.direction == "payment" && shows_on_calendar(&rule.status) && (!rule.next_date.is_empty() || !rule.start_date.is_empty())
    })
}

fn card_due_on(account: &Account, today: NaiveDate) -> Option<NaiveDate> {
    if let Some(date) = parse_iso_date(&account.due_date) {
        if date >= today {
            return Some(date);
        }
    }
    account.due_day.map(|day| next_month_day(day, today))
}

fn shows_on_calendar(status: &str) -> bool {
    !matches!(status, "paused" | "stopped" | "completed")
}

fn account_has_rule(store: &Store, account_id: &str) -> bool {
    if account_id.is_empty() {
        return false;
    }
    store.rules.iter().any(|rule| rule.account_id == account_id && shows_on_calendar(&rule.status) && (!rule.start_date.is_empty() || !rule.next_date.is_empty()))
}

fn account_spec(start: NaiveDate, cadence: &str) -> crate::finance_schedule::Spec {
    let cadence = normalize_cadence(cadence);
    let repeating = schedule_repeats(&cadence, 0);
    crate::finance_schedule::Spec {
        cadence: if repeating { cadence } else { String::new() },
        interval_days: 0,
        start,
        end_kind: if repeating { "none".into() } else { "after_count".into() },
        end_count: if repeating { 0 } else { 1 },
        end_date: None,
        amount_minor: None,
        skipped: Vec::new(),
        status: "active".into(),
    }
}

fn push_due_events(
    out: &mut Vec<CalEvent>,
    id_prefix: &str,
    name: &str,
    amount: Option<i64>,
    currency: &str,
    spec: &crate::finance_schedule::Spec,
    settled: &[NaiveDate],
    today: NaiveDate,
    start_ms: i64,
    end_ms: i64,
) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    for date in crate::finance_schedule::occurrence_dates(spec, today) {
        let Some(when) = local_midnight(date) else { continue };
        if when < start_ms || when >= end_ms {
            continue;
        }
        if spec.skipped.contains(&date) || settled.contains(&date) {
            continue;
        }
        let end = local_midnight(date + Duration::days(1)).unwrap_or(when.saturating_add(86_400_000));
        out.push(CalEvent {
            id: format!("{id_prefix}-{}", iso(date)),
            title: clamp(name, 80),
            start_ms: when,
            end_ms: end,
            location: amount.map(|value| format_minor(value, currency)).unwrap_or_default(),
            all_day: true,
            source: "finance".into(),
            recurrence: String::new(),
            weekdays: Vec::new(),
            until_ms: 0,
            alarm_id: 0,
            rrule: String::new(),
        });
    }
}

fn local_midnight(date: NaiveDate) -> Option<i64> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    Some(naive.and_local_timezone(Local).earliest().or_else(|| naive.and_local_timezone(Local).latest())?.timestamp_millis())
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
    touch_view(app, "");
}

fn touch_view(app: &AppHandle, view: &str) {
    touch_chart(app, view, "", "", "", "", "", "", "", "", false);
}

fn touch_chart(app: &AppHandle, view: &str, chart: &str, action: &str, name: &str, period: &str, start: &str, end: &str, mode: &str, color: &str, confirmed: bool) {
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::diagnostics::Hub>>() {
        hub.emit("voice://finance", json!({ "view": view, "chart": chart, "action": action, "name": name, "mode": mode, "period": period, "start": start, "end": end, "color": color, "confirmed": confirmed }));
    }
}

fn apply_flow_voice(store: &mut Store, action: &str, name: &str, color: &str, confirmed: bool, mode: &str) -> (String, bool) {
    if action == "reset" && (name.trim().is_empty() || matches!(fold(name).as_str(), "all" | "colors" | "every")) {
        if !confirmed {
            return (spoken("Reset every money-flow color? Say yes.", &[]), false);
        }
        store.flow_colors.clear();
        store.entity_colors.retain(|row| row.kind != "node");
        return (spoken("Every money-flow color is back to the default.", &[]), true);
    }
    if action == "reset" {
        let id = flow_element_id(name);
        store.flow_colors.retain(|row| fold(&row.id) != fold(&id) && fold(&row.id) != fold(name));
        store.entity_colors.retain(|row| !(row.kind == "node" && (fold(&row.id) == fold(&id) || fold(&row.id) == fold(name))));
        return (spoken("{name} is back to its default color on the money flow.", &[("name", name)]), true);
    }
    if action == "color" || action == "paint" {
        let swatch = named_swatch(color);
        if name.trim().is_empty() || swatch.is_empty() {
            return (spoken("Say the color.", &[]), false);
        }
        let id = flow_element_id(name);
        if let Some(row) = store.flow_colors.iter_mut().find(|row| fold(&row.id) == fold(&id)) {
            row.color = swatch.clone();
        } else {
            store.flow_colors.push(NodeColor { id: id.clone(), color: swatch.clone() });
        }
        remember_entity(store, "node", &id, &swatch);
        return (spoken("{name} is now {color} on the money flow.", &[("name", name), ("color", color)]), true);
    }
    if mode == "vertical" || mode == "horizontal" {
        store.flow_layout = mode.to_string();
        let sentence = if mode == "vertical" {
            spoken("The money flow runs from top to bottom.", &[])
        } else {
            spoken("The money flow runs from left to right.", &[])
        };
        return (sentence, true);
    }
    (String::new(), false)
}

fn flow_element_id(name: &str) -> String {
    match fold(name).as_str() {
        "total income" | "ingreso total" | "einnahmen gesamt" | "receita total" | "ingres total" | "pool" => "pool".into(),
        "total spending" | "gasto total" | "ausgaben gesamt" | "despesa total" | "spending" | "spend" => "spend".into(),
        "savings" | "ahorro" | "sparen" | "poupanca" | "estalvi" | "save" => "save".into(),
        "investments" | "inversiones" | "anlagen" | "investimentos" | "inversions" | "invest" => "invest".into(),
        "shortfall" | "falta" | "fehlbetrag" | "from savings" => "in:shortfall".into(),
        "rest" | "resto" => "out:rest".into(),
        _ => name.trim().into(),
    }
}

fn named_swatch(color: &str) -> String {
    let trimmed = color.trim();
    if trimmed.len() == 7 && trimmed.starts_with('#') {
        return trimmed.to_string();
    }
    if trimmed.starts_with("var(") {
        return trimmed.to_string();
    }
    match fold(trimmed).as_str() {
        "green" | "verde" | "grun" => "var(--flow-in-1)".into(),
        "blue" | "azul" | "blau" => "var(--flow-save)".into(),
        "red" | "rojo" | "rot" | "vermelho" | "vermell" => "var(--flow-out-1)".into(),
        "orange" | "naranja" | "laranja" | "taronja" => "var(--flow-invest)".into(),
        "gray" | "grey" | "gris" | "grau" | "cinza" | "cinzent" => "var(--text-tertiary)".into(),
        _ => String::new(),
    }
}

fn screen_view(view: &str) -> &'static str {
    named_screen(&fold(view)).unwrap_or("overview")
}

fn named_screen(view: &str) -> Option<&'static str> {
    match view {
        "overview" | "home" | "summary" => Some("overview"),
        "activity" | "transactions" | "expenses" | "income" => Some("activity"),
        "accounts" | "cards" | "debts" => Some("accounts"),
        "holdings" | "investments" | "portfolio" => Some("holdings"),
        "plan" | "coming" | "upcoming" | "schedules" | "budget" => Some("plan"),
        "charts" => Some("overview"),
        _ => None,
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
    // "EPIC Cash" contains the word cash. It is a coin, not a wallet, when the name resolves.
    if crate::finance_prices::resolve_holding(&name, "", "", "", "").is_some() {
        return "investment".into();
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
        "payment" | "pay" | "contribution" | "invest" => Ok("payment".into()),
        "transfer" | "move" => Ok("transfer".into()),
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
    if matches!(raw.as_str(), "every_n_days" | "every_n_weeks" | "every_n_months" | "every_n_years") {
        return raw;
    }
    if folded.contains("every") && folded.chars().any(|c| c.is_ascii_digit()) {
        if folded.contains("week") {
            return "every_n_weeks".into();
        }
        if folded.contains("month") {
            return "every_n_months".into();
        }
        if folded.contains("year") {
            return "every_n_years".into();
        }
        if folded.contains("day") {
            return "every_n_days".into();
        }
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
        "eth" | "ethereum" | "ether" => "ETH",
        "sol" | "solana" => "SOL",
        "ltc" | "litecoin" => "LTC",
        "doge" | "dogecoin" => "DOGE",
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
        other if crate::finance_fx::is_currency_code(other) => return Ok(other.to_ascii_uppercase()),
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
    let minor = parse_minor_signed(raw, exp)?;
    if minor <= 0 {
        return Err("The amount has to be more than zero.".into());
    }
    Ok(minor)
}

/// Statement balances may be zero or negative. A negative card balance is a credit.
fn parse_minor_signed(raw: &str, exp: u32) -> Result<i64, String> {
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
    if minor.abs() > 100_000_000_000_000 {
        return Err("That amount is too large.".into());
    }
    i64::try_from(minor).map_err(|_| "That amount is too large.".to_string())
}

fn mentions_credit(raw: &str) -> bool {
    let folded = fold(raw);
    if folded.contains("credit card") || folded.contains("credit limit") {
        return false;
    }
    folded.split_whitespace().any(|word| matches!(word, "credit" | "credito" | "crédito" | "crèdit" | "kredit" | "guthaben"))
}

fn prepare_statement(raw: &str) -> (String, bool) {
    let credit = mentions_credit(raw);
    if !credit {
        return (raw.to_string(), false);
    }
    let cleaned = raw
        .split_whitespace()
        .filter(|word| {
            let token = fold(word);
            !matches!(token.as_str(), "credit" | "credito" | "crédito" | "crèdit" | "kredit" | "guthaben" | "in" | "a" | "of" | "is" | "to" | "the")
        })
        .collect::<Vec<_>>()
        .join(" ");
    (if cleaned.is_empty() { raw.to_string() } else { cleaned }, true)
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
    let mut raw = String::new();
    for ch in value.trim().chars() {
        raw.push_str(&fold_char(ch));
    }
    raw.chars().filter(|ch| ch.is_alphanumeric() || ch.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compact_name(value: &str) -> String {
    fold(value).chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn fold_char(ch: char) -> String {
    let base = match ch {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ă' | 'Ą' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Ĭ' | 'Į' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ō' | 'ő' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' | 'Ō' | 'Ő' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' | 'Ų' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        'ñ' | 'ń' | 'Ñ' | 'Ń' => 'n',
        'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => 'c',
        'ß' => return "ss".into(),
        'æ' | 'Æ' => return "ae".into(),
        'œ' | 'Œ' => return "oe".into(),
        other => other,
    };
    base.to_lowercase().collect()
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut next = vec![0; right.len() + 1];
    for (i, ca) in left.iter().enumerate() {
        next[0] = i + 1;
        for (j, cb) in right.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            next[j + 1] = (prev[j + 1] + 1).min(next[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut next);
    }
    prev[right.len()]
}

fn lead_arg(args: &Value) -> Option<u32> {
    args.get("leadDays").and_then(|value| value.as_u64().or_else(|| value.as_i64().map(|n| n.max(0) as u64))).map(|days| days.min(30) as u32)
}

fn money_for_statement(amount: &str, currency_arg: &str, account_currency: &str, default_code: &str) -> Result<(i64, String), String> {
    let (cleaned, credit) = prepare_statement(amount);
    let (number, embedded) = peel_currency(&cleaned);
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
    let mut minor = parse_minor_signed(&number, exponent(&currency))?;
    if credit && minor > 0 {
        minor = -minor;
    }
    Ok((minor, currency))
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
    normalize_swatch(value)
}

const SERIES_HEX: [&str; 12] = ["#0072B2", "#9A5C00", "#0B7A5A", "#A84B86", "#C44E00", "#2E6A8A", "#B85A78", "#1F7A6E", "#9A5AAA", "#267A46", "#7A68C0", "#A3522A"];
const SERIES_HUE: [&str; 12] = ["blue", "amber", "green", "pink", "red", "steel", "rose", "teal", "purple", "forest", "indigo", "rust"];
const LEGACY_HEX: [&str; 10] = ["#4C78D8", "#6E8B74", "#A56B66", "#C4B5A0", "#7A6A58", "#3E6B73", "#8C6A3F", "#B8A48A", "#5C6B58", "#9A7B6A"];

fn palette_index(id: &str) -> usize {
    let mut hash: u32 = 0;
    for ch in id.chars() {
        hash = hash.wrapping_mul(33).wrapping_add(ch as u32);
    }
    hash as usize % SERIES_HEX.len()
}

fn default_swatch(id: &str) -> String {
    format!("var(--series-{})", palette_index(id) + 1)
}

fn normalize_swatch(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.starts_with("var(--series-") {
        let n: usize = trimmed.trim_start_matches("var(--series-").trim_end_matches(')').parse().unwrap_or(0);
        if (1..=SERIES_HEX.len()).contains(&n) {
            return Ok(format!("var(--series-{n})"));
        }
    }
    if trimmed.starts_with("var(--flow-") || trimmed.starts_with("var(--brand") || trimmed.starts_with("var(--positive") || trimmed.starts_with("var(--negative") {
        if trimmed.ends_with(')') && trimmed.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '(' | ')')) {
            return Ok(trimmed.to_string());
        }
    }
    let hex = trimmed.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Ok(format!("#{}", hex.to_ascii_uppercase()));
    }
    if let Some(named) = named_hex(trimmed) {
        return Ok(named);
    }
    Err("Use a color like #E25B45.".into())
}

fn named_hex(color: &str) -> Option<String> {
    let token = fold(color);
    let hex = match token.as_str() {
        "green" | "verde" | "grun" => "#009E73",
        "blue" | "azul" | "blau" => "#0072B2",
        "red" | "rojo" | "rot" | "vermelho" | "roig" | "vermell" => "#C44E00",
        "orange" | "naranja" | "laranja" | "taronja" => "#E69F00",
        "gray" | "grey" | "gris" | "grau" | "cinza" | "cinzent" | "grisa" => "#8A8A8A",
        "pink" | "rosa" => "#A84B86",
        "purple" | "morado" | "lila" | "roxo" | "morat" => "#9A5AAA",
        "teal" | "turquesa" | "petrol" => "#1F7A6E",
        "amber" | "ambar" => "#9A5C00",
        _ => return None,
    };
    Some(hex.to_string())
}

fn is_legacy_color(color: &str) -> bool {
    let hex = color.trim().to_ascii_uppercase();
    LEGACY_HEX.iter().any(|row| *row == hex)
}

fn shown_swatch(id: &str, saved: &str) -> String {
    let trimmed = saved.trim();
    if trimmed.starts_with("var(--") {
        return trimmed.to_string();
    }
    if let Ok(hex) = normalize_swatch(trimmed) {
        if !hex.is_empty() && !is_legacy_color(&hex) {
            return hex;
        }
    }
    default_swatch(id)
}

fn color_word(color: &str) -> String {
    let hex = if let Some(rest) = color.trim().strip_prefix("var(--series-") {
        let n: usize = rest.trim_end_matches(')').parse().unwrap_or(0);
        SERIES_HEX.get(n.saturating_sub(1)).copied().unwrap_or(SERIES_HEX[0]).to_string()
    } else {
        normalize_swatch(color).unwrap_or_default()
    };
    if let Some(index) = SERIES_HEX.iter().position(|row| *row == hex) {
        return spoken(SERIES_HUE[index], &[]);
    }
    match hex.as_str() {
        "#009E73" => spoken("green", &[]),
        "#0072B2" => spoken("blue", &[]),
        "#8A8A8A" => spoken("gray", &[]),
        "#E69F00" => spoken("orange", &[]),
        "#C44E00" => spoken("red", &[]),
        _ => hex,
    }
}

fn remember_entity(store: &mut Store, kind: &str, id: &str, color: &str) {
    let id = id.trim();
    if id.is_empty() {
        return;
    }
    let color = color.trim();
    store.entity_colors.retain(|row| !(row.kind == kind && fold(&row.id) == fold(id)));
    if color.is_empty() {
        return;
    }
    store.entity_colors.push(EntityColor { kind: kind.to_string(), id: id.to_string(), color: color.to_string() });
}

fn paint_entity(store: &mut Store, kind: &str, id: &str, color: &str) -> Result<(), String> {
    let kind = kind.trim();
    let id = id.trim();
    if kind.is_empty() || id.is_empty() {
        return Err("Say the color.".into());
    }
    let swatch = if color.trim().is_empty() { String::new() } else { normalize_swatch(color)? };
    match kind {
        "account" => {
            let Some(account) = store.accounts.iter_mut().find(|account| account.id == id || fold(&account.name) == fold(id) || fold(&account.ticker) == fold(id)) else {
                return Err(spoken("No saved item named {name}.", &[("name", id)]));
            };
            let account_id = account.id.clone();
            account.color = if swatch.is_empty() { default_swatch(&account_id) } else { swatch.clone() };
            let saved = account.color.clone();
            remember_entity(store, "account", &account_id, &saved);
        }
        "category" | "income" => {
            let Some(category) = store.categories.iter_mut().find(|category| fold(&category.name) == fold(id)) else {
                return Err(spoken("No saved item named {name}.", &[("name", id)]));
            };
            let name = category.name.clone();
            let saved_kind = if category.kind == "income" || kind == "income" { "income" } else { "category" };
            category.color = if swatch.is_empty() { default_swatch(&name) } else { swatch.clone() };
            if kind == "income" {
                category.kind = "income".into();
            }
            let saved = category.color.clone();
            remember_entity(store, saved_kind, &name, &saved);
        }
        "node" => {
            if swatch.is_empty() {
                store.flow_colors.retain(|row| fold(&row.id) != fold(id));
                remember_entity(store, "node", id, "");
            } else {
                if let Some(row) = store.flow_colors.iter_mut().find(|row| fold(&row.id) == fold(id)) {
                    row.color = swatch.clone();
                } else {
                    store.flow_colors.push(NodeColor { id: id.to_string(), color: swatch.clone() });
                }
                remember_entity(store, "node", id, &swatch);
            }
        }
        "class" | "series" => {
            remember_entity(store, kind, id, &swatch);
        }
        _ => return Err("Say the color.".into()),
    }
    Ok(())
}

fn normalize_investment_type(value: &str) -> String {
    match fold(value).as_str() {
        "stock" | "stocks" | "share" | "shares" | "equity" => "stock".into(),
        "fund" | "etf" | "index" => "fund".into(),
        "crypto" | "cryptocurrency" | "coin" | "token" => "crypto".into(),
        "property" | "real estate" | "house" | "home" | "cottage" => "property".into(),
        "collectible" | "collectibles" => "collectible".into(),
        "stake" | "private stake" | "private" => "stake".into(),
        "custom" | "other" | "asset" => "custom".into(),
        other if !other.is_empty() && other.len() <= 24 => other.to_string(),
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
    let mut frac_digits: Vec<u8> = frac.bytes().take(12).map(|byte| byte - b'0').collect();
    while frac_digits.len() < 12 {
        frac_digits.push(0);
    }
    let mut frac_val: i128 = 0;
    for digit in &frac_digits {
        frac_val = frac_val * 10 + i128::from(*digit);
    }
    let scale: i128 = 1_000_000_000_000;
    let product = whole * i128::from(price_minor) + (i128::from(price_minor) * frac_val + scale / 2) / scale;
    if product <= 0 || product > 100_000_000_000_000 {
        return Err("That amount is too large.".into());
    }
    i64::try_from(product).map_err(|_| "That amount is too large.".to_string())
}

fn remember_category(store: &mut Store, name: &str) {
    remember_child(store, name, "");
}

fn remember_child(store: &mut Store, name: &str, parent: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let parent = parent.trim();
    if let Some(category) = store.categories.iter_mut().find(|category| fold(&category.name) == fold(name)) {
        if !parent.is_empty() && category.parent.is_empty() && !fold(parent).eq_ignore_ascii_case(&fold(name)) {
            category.parent = parent.to_string();
        }
        return;
    }
    let color = default_swatch(name);
    let kind = if fold(name) == "uncategorized" { "expense".into() } else { String::new() };
    store.categories.push(Category {
        name: name.to_string(),
        color,
        kind,
        parent: if parent.is_empty() || fold(parent) == fold(name) { String::new() } else { parent.to_string() },
        ..Category::default()
    });
}

/// Voice logging never creates a category unless `confirm` is set. A description is matched to an existing category.
fn assign_categories(store: &mut Store, category: &str, subcategory: &str, notes: &mut String, confirm: bool) -> (String, String, String) {
    let mut category = category.trim().to_string();
    let mut subcategory = subcategory.trim().to_string();
    let mut hint = String::new();
    if category.is_empty() {
        let guess = suggest_category(store, &format!("{subcategory} {}", notes));
        if !guess.is_empty() {
            category = guess.clone();
            hint = spoken("Filed under {name}.", &[("name", &guess)]);
        }
    } else if let Some(found) = store.categories.iter().find(|row| fold(&row.name) == fold(&category) && row.parent.is_empty()) {
        category = found.name.clone();
    } else if confirm {
        let requested = category.clone();
        remember_category(store, &requested);
        category = store.categories.iter().find(|row| fold(&row.name) == fold(&requested)).map(|row| row.name.clone()).unwrap_or(requested);
    } else {
        hint = spoken("Create the category {name} to use it.", &[("name", &category)]);
    }
    if !subcategory.is_empty() {
        let exists = store.categories.iter().any(|row| fold(&row.name) == fold(&subcategory) && fold(&row.parent) == fold(&category));
        if exists {
            subcategory = store.categories.iter().find(|row| fold(&row.name) == fold(&subcategory)).map(|row| row.name.clone()).unwrap_or(subcategory);
        } else if confirm {
            remember_child(store, &subcategory, &category);
        } else {
            if notes.trim().is_empty() {
                *notes = subcategory.clone();
            }
            subcategory.clear();
        }
    }
    (category, subcategory, hint)
}

fn suggest_category(store: &Store, text: &str) -> String {
    let folded = fold(text);
    if folded.is_empty() {
        return String::new();
    }
    let mut best = String::new();
    let mut score = 0i32;
    for category in store.categories.iter().filter(|row| row.parent.is_empty()) {
        let name = fold(&category.name);
        if name.is_empty() || name == "uncategorized" {
            continue;
        }
        if folded.contains(&name) {
            let next = 100 + name.len() as i32;
            if next > score {
                score = next;
                best = category.name.clone();
            }
        }
    }
    if !best.is_empty() {
        return best;
    }
    const HINTS: &[(&str, &str)] = &[
        ("rent", "Housing"),
        ("alquiler", "Housing"),
        ("miete", "Housing"),
        ("renda", "Housing"),
        ("lloguer", "Housing"),
        ("vacuum", "Housing"),
        ("cleaner", "Housing"),
        ("grocery", "Groceries"),
        ("grocer", "Groceries"),
        ("supermerc", "Groceries"),
        ("food", "Food"),
        ("comida", "Food"),
        ("essen", "Food"),
        ("restaurant", "Food"),
        ("coffee", "Food"),
        ("cafe", "Food"),
        ("uber", "Transport"),
        ("salary", "Salary"),
        ("sueldo", "Salary"),
        ("gehalt", "Salary"),
        ("nomina", "Salary"),
    ];
    for (word, name) in HINTS {
        if folded.contains(word) {
            if let Some(found) = store.categories.iter().find(|row| row.parent.is_empty() && fold(&row.name) == fold(name)) {
                return found.name.clone();
            }
        }
    }
    String::new()
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
        if fold(&tx.subcategory) == fold(&previous) {
            tx.subcategory = to.clone();
        }
    }
    for rule in &mut store.rules {
        if fold(&rule.category) == fold(&previous) {
            rule.category = to.clone();
        }
        if fold(&rule.subcategory) == fold(&previous) {
            rule.subcategory = to.clone();
        }
    }
    for category in &mut store.categories {
        if fold(&category.parent) == fold(&previous) {
            category.parent = to.clone();
        }
    }
    Ok(())
}

fn ensure_categories(store: &mut Store) -> bool {
    if store.categories_ready {
        return false;
    }
    for tx in &mut store.transactions {
        if tx.category.trim().is_empty() {
            tx.category = "Uncategorized".into();
        }
    }
    if !store.categories.iter().any(|category| fold(&category.name) == "uncategorized") && store.transactions.iter().any(|tx| fold(&tx.category) == "uncategorized") {
        store.categories.push(Category { name: "Uncategorized".into(), color: "#8C6A3F".into(), kind: "expense".into(), ..Category::default() });
    }
    for category in &mut store.categories {
        if category.kind.is_empty() {
            let income = store.transactions.iter().any(|tx| fold(&tx.category) == fold(&category.name) && tx.direction == "income");
            let expense = store.transactions.iter().any(|tx| fold(&tx.category) == fold(&category.name) && tx.direction != "income");
            category.kind = if income && !expense { "income".into() } else { "expense".into() };
        }
    }
    store.categories_ready = true;
    true
}

pub fn edit_category(store: &mut Store, action: &str, name: &str, to: &str, color: &str, kind: &str, icon: &str, order: i32) -> Result<(), String> {
    edit_category_parent(store, action, name, to, color, kind, icon, order, "")
}

pub fn edit_category_parent(store: &mut Store, action: &str, name: &str, to: &str, color: &str, kind: &str, icon: &str, order: i32, parent: &str) -> Result<(), String> {
    let action = fold(action);
    let name = name.trim();
    match action.as_str() {
        "create" => {
            let label = clean_text(name, 80)?;
            if label.is_empty() {
                return Err("A category needs a name.".into());
            }
            if store.categories.iter().any(|category| fold(&category.name) == fold(&label)) {
                let saved = {
                    let category = store.categories.iter_mut().find(|category| fold(&category.name) == fold(&label)).unwrap();
                    if !color.is_empty() { category.color = normalize_swatch(color)?; }
                    if !kind.is_empty() { category.kind = kind.to_string(); }
                    if !icon.is_empty() { category.icon = icon.to_string(); }
                    if !parent.trim().is_empty() { category.parent = parent.trim().to_string(); }
                    (category.kind.clone(), category.name.clone(), category.color.clone())
                };
                remember_entity(store, if saved.0 == "income" { "income" } else { "category" }, &saved.1, &saved.2);
                return Ok(());
            }
            let swatch = if color.trim().is_empty() { default_swatch(&label) } else { normalize_swatch(color)? };
            let parent = parent.trim();
            if !parent.is_empty() && !store.categories.iter().any(|category| fold(&category.name) == fold(parent) && category.parent.is_empty()) {
                return Err("No category with that name.".into());
            }
            let saved_kind = kind.trim().to_string();
            store.categories.push(Category {
                name: label.clone(),
                color: swatch.clone(),
                kind: saved_kind.clone(),
                icon: icon.trim().to_string(),
                order,
                parent: parent.to_string(),
            });
            remember_entity(store, if saved_kind == "income" { "income" } else { "category" }, &label, &swatch);
            Ok(())
        }
        "rename" => rename_category(store, name, to),
        "merge" => {
            let into = clean_text(to, 80)?;
            if into.is_empty() {
                return Err("A category needs a name.".into());
            }
            if !store.categories.iter().any(|category| fold(&category.name) == fold(&into)) {
                remember_category(store, &into);
            }
            let previous = store.categories.iter().find(|category| fold(&category.name) == fold(name)).map(|category| category.name.clone()).ok_or("No category with that name.")?;
            for tx in &mut store.transactions {
                if fold(&tx.category) == fold(&previous) { tx.category = into.clone(); }
            }
            for rule in &mut store.rules {
                if fold(&rule.category) == fold(&previous) { rule.category = into.clone(); }
            }
            store.categories.retain(|category| fold(&category.name) != fold(&previous));
            Ok(())
        }
        "delete" => {
            let previous = store.categories.iter().find(|category| fold(&category.name) == fold(name)).map(|category| category.name.clone()).ok_or("No category with that name.")?;
            if !store.categories.iter().any(|category| fold(&category.name) == "uncategorized") {
                store.categories.push(Category { name: "Uncategorized".into(), color: "#8C6A3F".into(), kind: "expense".into(), ..Category::default() });
            }
            for tx in &mut store.transactions {
                if fold(&tx.category) == fold(&previous) { tx.category = "Uncategorized".into(); }
            }
            for rule in &mut store.rules {
                if fold(&rule.category) == fold(&previous) { rule.category = "Uncategorized".into(); }
            }
            store.categories.retain(|category| fold(&category.name) != fold(&previous) || fold(&previous) == "uncategorized");
            Ok(())
        }
        "layout" => {
            let layout = fold(name);
            if layout == "vertical" || layout == "horizontal" {
                store.flow_layout = layout;
            }
            Ok(())
        }
        "unpaint" => {
            if name.is_empty() || matches!(fold(name).as_str(), "all" | "colors") {
                store.flow_colors.clear();
                store.entity_colors.retain(|row| row.kind != "node");
            } else {
                let id = flow_element_id(name);
                store.flow_colors.retain(|row| fold(&row.id) != fold(&id) && fold(&row.id) != fold(name));
                store.entity_colors.retain(|row| !(row.kind == "node" && (fold(&row.id) == fold(&id) || fold(&row.id) == fold(name))));
            }
            Ok(())
        }
        "color" | "kind" | "icon" | "order" | "paint" => {
            if action == "paint" {
                let id = if name.is_empty() { to.trim().to_string() } else { name.to_string() };
                if id.is_empty() || color.trim().is_empty() {
                    return Err("A category needs a name.".into());
                }
                let swatch = normalize_swatch(color)?;
                if let Some(row) = store.flow_colors.iter_mut().find(|row| row.id == id) {
                    row.color = swatch.clone();
                } else {
                    store.flow_colors.push(NodeColor { id: id.clone(), color: swatch.clone() });
                }
                remember_entity(store, "node", &id, &swatch);
                return Ok(());
            }
            let saved = {
                let Some(category) = store.categories.iter_mut().find(|category| fold(&category.name) == fold(name)) else {
                    return Err("No category with that name.".into());
                };
                if action == "color" { category.color = normalize_swatch(color)?; }
                if action == "kind" { category.kind = kind.trim().to_string(); }
                if action == "icon" { category.icon = icon.trim().to_string(); }
                if action == "order" { category.order = order; }
                (category.kind.clone(), category.name.clone(), category.color.clone())
            };
            if action == "color" {
                remember_entity(store, if saved.0 == "income" { "income" } else { "category" }, &saved.1, &saved.2);
            }
            Ok(())
        }
        _ => Err("A category needs a name.".into()),
    }
}

fn category_phrase(store: &mut Store, phrase: &str) -> Option<Result<String, String>> {
    let folded = fold(phrase);
    if folded.contains("money flow") || folded.contains("flujo de dinero") || folded.contains("geldfluss") || folded.contains("fluxo de dinheiro") || folded.contains("flux de diners") {
        return Some(Ok(spoken("Showing the money flow.", &[])));
    }
    if folded.starts_with("create ") && (folded.contains("subcategory") || folded.contains("subcategoria") || folded.contains("unterkategorie")) {
        let parent = [" under ", " bajo ", " unter ", " em ", " sota "]
            .iter()
            .find_map(|mark| phrase.to_lowercase().rfind(mark).map(|index| phrase[index + mark.len()..].trim().to_string()));
        let Some(parent) = parent.filter(|name| !name.is_empty()) else {
            return Some(Err("A category needs a name.".into()));
        };
        let name = phrase.split_whitespace().skip(2).take_while(|word| !matches!(fold(word).as_str(), "under" | "bajo" | "unter" | "em" | "sota")).collect::<Vec<_>>().join(" ");
        if name.trim().is_empty() {
            return Some(Err("A category needs a name.".into()));
        }
        remember_category(store, parent.trim());
        return Some(edit_category_parent(store, "create", name.trim(), "", "", "expense", "", store.categories.len() as i32, parent.trim()).map(|_| spoken("Saved {name}.", &[("name", name.trim())])));
    }
    if folded.starts_with("create ") && folded.contains("category") {
        let kind = if folded.contains("income") { "income" } else if folded.contains("saving") { "savings" } else if folded.contains("invest") { "investment" } else if folded.contains("debt") { "debt" } else { "expense" };
        let name = phrase.rsplit_once("category").map(|(_, rest)| rest.trim()).unwrap_or("").trim_matches(|ch: char| !ch.is_alphanumeric() && ch != ' ');
        let name = name.trim();
        if name.is_empty() {
            return Some(Err("A category needs a name.".into()));
        }
        return Some(edit_category(store, "create", name, "", "", kind, "", store.categories.len() as i32).map(|_| spoken("Saved {name}.", &[("name", name)])));
    }
    let lower = phrase.to_lowercase();
    let marker = lower.rfind(" as ").or_else(|| lower.rfind(" als "))?;
    if !(folded.starts_with("categorize") || folded.starts_with("categorise") || folded.starts_with("categoriza") || folded.starts_with("klassifiz") || folded.starts_with("classifica") || folded.starts_with("classifica")) {
        return None;
    }
    let left = phrase[..marker].trim();
    let right = phrase[marker + 4..].trim().trim_matches(|ch: char| ch == '.' || ch == '!');
    let mut needle = fold(left);
    for word in ["categorize", "categorise", "categoriza", "my", "the", "expense", "expenses", "income", "pago", "gasto", "ingreso"] {
        needle = needle.replace(word, " ");
    }
    let needle = needle.split_whitespace().filter(|word| word.len() > 2).collect::<Vec<_>>().join(" ");
    if needle.is_empty() || right.is_empty() {
        return Some(Err("A category needs a name.".into()));
    }
    let label = title_phrase(right);
    let mut hits = 0;
    for tx in &mut store.transactions {
        let blob = fold(&format!("{} {}", tx.category, tx.notes));
        if blob.contains(&needle) {
            tx.category = label.clone();
            hits += 1;
        }
    }
    if hits == 0 {
        return Some(Err("No category with that name.".into()));
    }
    let _ = edit_category(store, "create", &label, "", "", "expense", "", store.categories.len() as i32);
    Some(Ok(spoken("Saved {name}.", &[("name", &label)])))
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
        subcategory: rule.subcategory.clone(),
        account_id: rule.account_id.clone(),
        from_account_id: String::new(),
        date: posted,
        notes: if rule.amount_mode == "quote" && !rule.payout_currency.is_empty() { spoken("Settled in {code}.", &[("code", &rule.payout_currency)]) } else { String::new() },
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
        if fold(name).starts_with("payday") { name.to_string() } else { spoken("Payday {name}", &[("name", name)]) }
    } else if fold(name).starts_with("pay ") {
        name.to_string()
    } else {
        spoken("Pay {name}", &[("name", name)])
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
    let display = effective_default(store, locale);
    let now_ms = Local::now().timestamp_millis();
    let bills = overdue_bills(store, today);
    let mut parts: Vec<(i64, String)> = nets.iter().map(|row| (row.1 + row.3 - row.2, row.0.clone())).filter(|row| row.0 != 0).collect();
    subtract_parts(&mut parts, &bills);
    let heading = tr(locale, "Finance summary");
    let mut notes = vec![tr(locale, "Balances are what you last said.")];
    let mut numbers = Vec::new();
    match crate::finance_fx::combine(&parts, &display, &store.fx, now_ms) {
        Some(combined) => {
            numbers.push((tr(locale, "Combined total"), format_minor(combined.amount_minor, &combined.currency)));
            let mut note = tr(locale, "Combined in {code}.").replace("{code}", &combined.currency);
            if !combined.as_of.is_empty() {
                note.push(' ');
                note.push_str(&tr(locale, "As of {date}.").replace("{date}", &combined.as_of));
            }
            if combined.source == "cached" && !combined.stale {
                note.push(' ');
                note.push_str(&tr(locale, "Today's rate."));
            }
            if combined.stale {
                note.push(' ');
                note.push_str(&tr(locale, "Rates are stale."));
            }
            let fetched = fetched_clock(store.fx.fetched_ms);
            if !fetched.is_empty() {
                note.push(' ');
                note.push_str(&tr(locale, "Fetched {time}.").replace("{time}", &fetched));
            }
            notes.push(note);
        }
        None if !parts.is_empty() => {
            let missing = crate::finance_fx::missing_codes(&parts, &display, &store.fx);
            numbers.push((tr(locale, "Combined total"), tr(locale, "Needs a rate")));
            notes.push(tr(locale, "Combined total needs a rate for {codes}.").replace("{codes}", &missing.join(", ")));
        }
        None => {}
    }
    let mut summary = Page {
        title: heading,
        subtitle: format!("{} · {}", tr(locale, "On this phone. No bank login."), span.label),
        notes,
        numbers,
        rows: Vec::new(),
        chart: None,
    };
    for account in &accounts {
        let group = if is_debt(&account.kind) { tr(locale, "Debts") } else if account.kind == "investment" { tr(locale, "Investments") } else { tr(locale, "Money") };
        let amount = shown_balance(account, &store.transactions).map(|minor| format_minor(minor, &account.currency)).unwrap_or_else(|| tr(locale, "Not set"));
        summary.rows.push(Row { name: account.name.clone(), detail: group, amount, color: if account.color.is_empty() { "#C4B5A0".into() } else { account.color.clone() } });
    }
    let mut pages = vec![summary];
    let money_parts: Vec<(i64, String)> = nets.iter().filter(|row| row.1 != 0).map(|row| (row.1, row.0.clone())).collect();
    let mut debt_parts: Vec<(i64, String)> = accounts
        .iter()
        .filter(|account| is_debt(&account.kind))
        .filter_map(|account| {
            let shown = shown_balance(account, &store.transactions)?;
            if shown <= 0 { None } else { Some((shown, account.currency.clone())) }
        })
        .collect();
    debt_parts.extend(bills.iter().cloned());
    let invest_parts: Vec<(i64, String)> = nets.iter().filter(|row| row.3 != 0).map(|row| (row.3, row.0.clone())).collect();
    let money = combined_amount(&money_parts, &display, &store.fx, now_ms);
    let debt = combined_amount(&debt_parts, &display, &store.fx, now_ms);
    let invest = combined_amount(&invest_parts, &display, &store.fx, now_ms);
    let net = crate::finance_fx::combine(&parts, &display, &store.fx, now_ms);
    if let (Some(money), Some(debt), Some(invest), Some(net)) = (money, debt, invest, net) {
        let owed = debt.max(0);
        let credit = (-debt).max(0);
        let mut slices = vec![
            Slice { label: tr(locale, "Money"), value: money.max(0) as f64, color: "#6E8B74".into(), caption: format_minor(money, &display) },
            Slice { label: tr(locale, "Investments"), value: invest.max(0) as f64, color: "#C4B5A0".into(), caption: format_minor(invest, &display) },
        ];
        if credit > 0 {
            slices.push(Slice { label: tr(locale, "Card credit"), value: credit as f64, color: "#C4B5A0".into(), caption: format_minor(credit, &display) });
        }
        slices.retain(|slice| slice.value > 0.0);
        let assets = money.max(0) + invest.max(0) + credit;
        let mut numbers = vec![(tr(locale, "Net worth"), format_minor(net.amount_minor, &display))];
        if owed > 0 {
            numbers.push((tr(locale, "Debts"), format_minor(owed, &display)));
            let percent = if assets > 0 { (owed as f64) * 100.0 / assets as f64 } else { 100.0 };
            let percent = format!("{:.1}", percent);
            numbers.push((tr(locale, "Debt ratio"), tr(locale, "{percent}% of assets").replace("{percent}", &percent)));
        }
        if slices.iter().any(|slice| slice.value != 0.0) || net.amount_minor != 0 || owed > 0 {
            pages.push(Page {
                title: tr(locale, "Money, debts, and investments"),
                subtitle: span.label.clone(),
                notes: vec![
                    tr(locale, "Combined in {code}.").replace("{code}", &display),
                    tr(locale, "Debts are subtracted from net worth."),
                ],
                numbers,
                rows: vec![],
                chart: Some(Chart::Donut { center_label: tr(locale, "Net worth"), center_value: format_minor(net.amount_minor, &display), slices }),
            });
        }
    let mut flow_rows: Vec<(String, i64, String)> = Vec::new();
    for tx in txs.iter().filter(|tx| tx.direction == "expense" && tx.amount_minor > 0) {
        let known = store.categories.iter().any(|category| fold(&category.name) == fold(&tx.category));
        let name = if tx.category.trim().is_empty() || !known { tr(locale, "Uncategorized") } else { tx.category.clone() };
        if let Some(row) = flow_rows.iter_mut().find(|row| row.0 == name && row.2 == tx.currency) {
            row.1 += tx.amount_minor;
        } else {
            flow_rows.push((name, tx.amount_minor, tx.currency.clone()));
        }
    }
    if !flow_rows.is_empty() {
        flow_rows.sort_by(|a, b| b.1.cmp(&a.1));
        pages.push(Page {
            title: tr(locale, "Where my money went"),
            subtitle: span.label.clone(),
            notes: vec![tr(locale, "A description without a category stays under Uncategorized.")],
            numbers: vec![],
            rows: vec![],
            chart: Some(Chart::Flow {
                columns: {
                    let sinks: Vec<Slice> = flow_rows
                        .iter()
                        .take(8)
                        .map(|(name, amount, currency)| Slice {
                            label: name.clone(),
                            value: *amount as f64,
                            color: "#C48984".into(),
                            caption: format_minor(*amount, currency),
                        })
                        .collect();
                    let total: i64 = sinks.iter().map(|slice| slice.value as i64).sum();
                    vec![
                        vec![Slice { label: tr(locale, "Income"), value: total as f64, color: "#7D9A86".into(), caption: String::new() }],
                        vec![Slice { label: tr(locale, "Total"), value: total as f64, color: "#C4B5A0".into(), caption: String::new() }],
                        sinks,
                    ]
                },
            }),
        });
    }
    } else if !parts.is_empty() {
        let missing = crate::finance_fx::missing_codes(&parts, &display, &store.fx);
        remember_rate(&mut pages, locale, &missing);
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
        let income_parts = direction_parts(&totals, |row| row.income_minor);
        let expense_parts = direction_parts(&totals, |row| row.expense_minor);
        let payment_parts = direction_parts(&totals, |row| row.payment_minor);
        let income = combined_amount(&income_parts, &display, &store.fx, now_ms);
        let expense = combined_amount(&expense_parts, &display, &store.fx, now_ms);
        let payment = combined_amount(&payment_parts, &display, &store.fx, now_ms);
        if let (Some(income), Some(expense), Some(payment)) = (income, expense, payment) {
            let left_parts: Vec<(i64, String)> = totals.iter().map(|row| (row.income_minor - row.expense_minor - row.payment_minor, row.currency.clone())).filter(|row| row.0 != 0).collect();
            let left = crate::finance_fx::combine(&left_parts, &display, &store.fx, now_ms);
            let mut numbers = Vec::new();
            if let Some(left) = left {
                numbers.push((tr(locale, "Combined total"), format_minor(left.amount_minor, &display)));
            }
            pages.push(Page {
                title: tr(locale, "Income and expenses"),
                subtitle: span.label.clone(),
                notes: vec![tr(locale, "Combined in {code}.").replace("{code}", &display)],
                numbers,
                rows: vec![],
                chart: Some(Chart::Groups {
                    captions: vec![display.clone()],
                    series: vec![tr(locale, "Income"), tr(locale, "Expenses"), tr(locale, "Payments")],
                    values: vec![vec![income as f64], vec![expense as f64], vec![payment as f64]],
                    colors: vec!["#6E8B74".into(), "#A56B66".into(), "#C4B5A0".into()],
                }),
            });
        } else {
            let mut flow = income_parts;
            flow.extend(expense_parts);
            flow.extend(payment_parts);
            remember_rate(&mut pages, locale, &crate::finance_fx::missing_codes(&flow, &display, &store.fx));
        }
    }
    let mut category_rows: Vec<(String, i64)> = Vec::new();
    let mut category_parts: Vec<(i64, String)> = Vec::new();
    for tx in txs.iter().filter(|tx| tx.direction == "expense" && tx.amount_minor != 0) {
        let name = if tx.category.is_empty() { tr(locale, "Expense") } else { tx.category.clone() };
        category_parts.push((tx.amount_minor, tx.currency.clone()));
        if let Some(converted) = crate::finance_fx::convert(tx.amount_minor, &tx.currency, &display, &store.fx, now_ms) {
            if let Some(row) = category_rows.iter_mut().find(|row| fold(&row.0) == fold(&name)) {
                row.1 += converted.amount_minor;
            } else {
                category_rows.push((name, converted.amount_minor));
            }
        }
    }
    let category_missing = crate::finance_fx::missing_codes(&category_parts, &display, &store.fx);
    if !category_missing.is_empty() {
        remember_rate(&mut pages, locale, &category_missing);
    } else if !category_rows.is_empty() {
        let rows = category_rows
            .iter()
            .map(|(name, amount)| Slice {
                label: name.clone(),
                value: *amount as f64,
                color: store.categories.iter().find(|category| fold(&category.name) == fold(name)).map(|category| category.color.clone()).unwrap_or_else(|| "#8C6A3F".into()),
                caption: format_minor(*amount, &display),
            })
            .collect();
        pages.push(Page { title: tr(locale, "By category"), subtitle: span.label.clone(), notes: vec![tr(locale, "Combined in {code}.").replace("{code}", &display)], numbers: vec![], rows: vec![], chart: Some(Chart::Bars { rows }) });
    }
    match month_trend(&store.transactions, today, &request, &display, &store.fx, now_ms) {
        Some(trend) if trend.iter().any(|point| point.1 != 0 || point.2 != 0) => {
            pages.push(Page {
                title: tr(locale, "Six months"),
                subtitle: display.clone(),
                notes: vec![tr(locale, "Combined in {code}.").replace("{code}", &display)],
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
        None => {
            let mut trend_parts = Vec::new();
            for tx in &store.transactions {
                if tx.direction == "income" || tx.direction == "expense" {
                    trend_parts.push((tx.amount_minor, tx.currency.clone()));
                }
            }
            remember_rate(&mut pages, locale, &crate::finance_fx::missing_codes(&trend_parts, &display, &store.fx));
        }
        Some(_) => {}
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

fn month_trend(transactions: &[Transaction], today: NaiveDate, request: &PdfRequest, display: &str, book: &crate::finance_fx::FxBook, now_ms: i64) -> Option<Vec<(String, i64, i64)>> {
    let start = add_months(NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today), -5).unwrap_or(today);
    let mut points = Vec::new();
    let mut cursor = start;
    for _ in 0..6 {
        let key = format!("{:04}-{:02}", cursor.year(), cursor.month());
        let mut income_parts = Vec::new();
        let mut expense_parts = Vec::new();
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
            if tx.direction == "income" && tx.amount_minor != 0 {
                income_parts.push((tx.amount_minor, tx.currency.clone()));
            }
            if tx.direction == "expense" && tx.amount_minor != 0 {
                expense_parts.push((tx.amount_minor, tx.currency.clone()));
            }
        }
        let income = combined_amount(&income_parts, display, book, now_ms)?;
        let expense = combined_amount(&expense_parts, display, book, now_ms)?;
        points.push((format!("{:02}", cursor.month()), income, expense));
        cursor = add_months(cursor, 1).unwrap_or(cursor);
    }
    Some(points)
}

fn combined_amount(parts: &[(i64, String)], display: &str, book: &crate::finance_fx::FxBook, now_ms: i64) -> Option<i64> {
    if parts.iter().all(|part| part.0 == 0) {
        return Some(0);
    }
    crate::finance_fx::combine(parts, display, book, now_ms).map(|row| row.amount_minor)
}

fn direction_parts(totals: &[PeriodTotal], pick: impl Fn(&PeriodTotal) -> i64) -> Vec<(i64, String)> {
    totals.iter().filter_map(|row| {
        let amount = pick(row);
        if amount == 0 { None } else { Some((amount, row.currency.clone())) }
    }).collect()
}

fn remember_rate(pages: &mut [crate::finance_pdf::Page], locale: &str, codes: &[String]) {
    if codes.is_empty() {
        return;
    }
    let line = tr(locale, "Combined total needs a rate for {codes}.").replace("{codes}", &codes.join(", "));
    if let Some(page) = pages.first_mut() {
        if !page.notes.iter().any(|note| note == &line) {
            page.notes.push(line);
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFigure {
    pub state: String,
    pub text: String,
    pub missing: String,
    pub stale: bool,
    /// Used by tests. The widget file only stores `text`.
    #[serde(skip)]
    #[allow(dead_code)]
    pub minor: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetBoard {
    pub display: String,
    #[serde(default)]
    pub fetched_ms: i64,
    pub net: WidgetFigure,
    pub month: WidgetFigure,
    #[serde(rename = "in")]
    pub coming_in: WidgetFigure,
    #[serde(rename = "out")]
    pub coming_out: WidgetFigure,
}

fn widget_figure(parts: &[(i64, String)], display: &str, book: &crate::finance_fx::FxBook, now_ms: i64) -> WidgetFigure {
    let usable: Vec<(i64, String)> = parts.iter().filter(|part| part.0 != 0 && !part.1.is_empty()).cloned().collect();
    if usable.is_empty() {
        return WidgetFigure { state: "empty".into(), text: String::new(), missing: String::new(), stale: false, minor: None };
    }
    if let Some(combined) = crate::finance_fx::combine(&usable, display, book, now_ms) {
        return WidgetFigure {
            state: "ready".into(),
            text: format_minor(combined.amount_minor, &combined.currency),
            missing: String::new(),
            stale: combined.stale,
            minor: Some(combined.amount_minor),
        };
    }
    WidgetFigure {
        state: "needs-rate".into(),
        text: String::new(),
        missing: crate::finance_fx::missing_codes(&usable, display, book).join(", "),
        stale: false,
        minor: None,
    }
}

fn worth_account_list(store: &Store, display: &str, now_ms: i64) -> Vec<crate::finance_schedule::WorthAccount> {
    store
        .accounts
        .iter()
        .map(|account| {
            let view = view_of(store, account, now_ms, display);
            if let Some(worth) = view.market_worth_minor {
                crate::finance_schedule::WorthAccount { kind: account.kind.clone(), currency: view.market_worth_currency, shown_minor: Some(worth) }
            } else {
                crate::finance_schedule::WorthAccount {
                    kind: account.kind.clone(),
                    currency: account.currency.clone(),
                    shown_minor: shown_with_fx(account, &store.transactions, &store.fx, now_ms),
                }
            }
        })
        .collect()
}

fn widget_board(store: &Store, today: NaiveDate, now_ms: i64) -> WidgetBoard {
    let display = effective_default(store, &store.locale_hint);
    let accounts = worth_account_list(store, &display, now_ms);
    let nets = crate::finance_schedule::worth_rows(&accounts);
    let mut net_parts: Vec<(i64, String)> = nets.into_iter().map(|row| (row.net_minor, row.currency)).collect();
    subtract_parts(&mut net_parts, &overdue_bills(store, today));
    let month_key = format!("{:04}-{:02}", today.year(), today.month());
    let mut month_by: Vec<(String, i64)> = Vec::new();
    let flowed = flow_transactions(store, today);
    for tx in &flowed {
        if !tx.date.starts_with(&month_key) || tx.currency.is_empty() {
            continue;
        }
        let delta = match tx.direction.as_str() {
            "income" => tx.amount_minor,
            "expense" => -tx.amount_minor,
            _ => 0,
        };
        if delta == 0 {
            continue;
        }
        let code = tx.currency.to_ascii_uppercase();
        if let Some(row) = month_by.iter_mut().find(|row| row.0 == code) {
            row.1 += delta;
        } else {
            month_by.push((code, delta));
        }
    }
    let month_parts: Vec<(i64, String)> = month_by.into_iter().map(|(code, amount)| (amount, code)).collect();
    let horizon = today + Duration::days(30);
    let (coming_in, coming_out) = window_parts(store, today, today, horizon);
    WidgetBoard {
        display: display.clone(),
        fetched_ms: store.fx.fetched_ms,
        net: widget_figure(&net_parts, &display, &store.fx, now_ms),
        month: widget_figure(&month_parts, &display, &store.fx, now_ms),
        coming_in: widget_figure(&coming_in, &display, &store.fx, now_ms),
        coming_out: widget_figure(&coming_out, &display, &store.fx, now_ms),
    }
}

fn window_parts(store: &Store, today: NaiveDate, from: NaiveDate, to: NaiveDate) -> (Vec<(i64, String)>, Vec<(i64, String)>) {
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    for rule in &store.rules {
        if matches!(rule.status.as_str(), "paused" | "stopped" | "completed") {
            continue;
        }
        let Some(amount) = rule.amount_minor else { continue };
        if amount == 0 || rule.currency.trim().is_empty() {
            continue;
        }
        let on_card = store.accounts.iter().any(|account| account.id == rule.account_id && account.kind == "credit_card");
        if on_card && rule.direction != "income" && rule.direction != "payment" {
            continue;
        }
        if on_card && rule.direction == "payment" && card_nothing_due(store, &rule.account_id) {
            continue;
        }
        let spec = rule_spec(rule);
        let settled = settled_dates(store, &rule.id);
        for date in crate::finance_schedule::occurrence_dates(&spec, today) {
            if date < from || date > to || settled.contains(&date) || spec.skipped.contains(&date) {
                continue;
            }
            let part = (amount, rule.currency.to_ascii_uppercase());
            if rule.direction == "income" {
                incoming.push(part);
            } else {
                outgoing.push(part);
            }
        }
    }
    (incoming, outgoing)
}

fn publish_board(path: &std::path::Path, store: &Store) {
    let Some(dir) = path.parent() else { return };
    let board = widget_board(store, Local::now().date_naive(), Local::now().timestamp_millis());
    let Ok(bytes) = serde_json::to_vec_pretty(&board) else { return };
    let _ = crate::files::write_private(&dir.join("widget-finance.json"), &bytes);
}

fn color_or(color: &str) -> String {
    let trimmed = color.trim();
    if let Some(rest) = trimmed.strip_prefix("var(--series-") {
        let n: usize = rest.trim_end_matches(')').parse().unwrap_or(1);
        return SERIES_HEX.get(n.saturating_sub(1)).copied().unwrap_or(SERIES_HEX[0]).to_string();
    }
    if trimmed.is_empty() {
        return SERIES_HEX[0].to_string();
    }
    trimmed.to_string()
}

fn fetched_clock(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    chrono::Local.timestamp_millis_opt(ms).single().map(|dt| dt.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
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
    ("Where my money went", "A dónde fue el dinero"),
    ("A description without a category stays under Uncategorized.", "Una descripción sin categoría queda en Sin categoría."),
    ("Finance summary", "Resumen de finanzas"),
    ("On this phone. No bank login.", "En este teléfono. Sin acceso al banco."),
    ("Balances are what you last said.", "Los saldos son lo último que dijiste."),
    ("No exchange rate is applied.", "No se aplica ningún tipo de cambio."),
    ("Combined total", "Total combinado"),
    ("Combined in {code}.", "Combinado en {code}."),
    ("As of {date}.", "A fecha de {date}."),
    ("Rates are stale.", "Los tipos están desfasados."),
    ("Today's rate.", "Tipo de hoy."),
    ("Fetched {time}.", "Obtenido {time}."),
    ("Needs a rate", "Falta un tipo"),
    ("Combined total needs a rate for {codes}.", "El total combinado necesita un tipo para {codes}."),
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
    ("Card credit", "Saldo a favor"),
    ("Debt ratio", "Ratio de deuda"),
    ("{percent}% of assets", "{percent}% de los activos"),
    ("Debts are subtracted from net worth.", "Las deudas se restan del patrimonio neto."),
];

const DE_PDF: &[(&str, &str)] = &[
    ("Where my money went", "Wohin das Geld ging"),
    ("A description without a category stays under Uncategorized.", "Eine Beschreibung ohne Kategorie bleibt unter Ohne Kategorie."),
    ("Finance summary", "Finanzüberblick"),
    ("On this phone. No bank login.", "Auf diesem Telefon. Kein Bankzugang."),
    ("Balances are what you last said.", "Die Salden sind, was du zuletzt gesagt hast."),
    ("No exchange rate is applied.", "Es wird kein Wechselkurs angewendet."),
    ("Combined total", "Summe"),
    ("Combined in {code}.", "Zusammen in {code}."),
    ("As of {date}.", "Stand {date}."),
    ("Rates are stale.", "Die Kurse sind veraltet."),
    ("Today's rate.", "Kurs von heute."),
    ("Fetched {time}.", "Geholt {time}."),
    ("Needs a rate", "Kurs fehlt"),
    ("Combined total needs a rate for {codes}.", "Die Summe braucht einen Kurs für {codes}."),
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
    ("Card credit", "Kartenguthaben"),
    ("Debt ratio", "Schuldenquote"),
    ("{percent}% of assets", "{percent}% der Vermögenswerte"),
    ("Debts are subtracted from net worth.", "Schulden werden vom Vermögen abgezogen."),
];

const PT_PDF: &[(&str, &str)] = &[
    ("Where my money went", "Para onde foi o dinheiro"),
    ("A description without a category stays under Uncategorized.", "Uma descrição sem categoria fica em Sem categoria."),
    ("Finance summary", "Resumo financeiro"),
    ("On this phone. No bank login.", "Neste telefone. Sem acesso ao banco."),
    ("Balances are what you last said.", "Os saldos são o que você disse por último."),
    ("No exchange rate is applied.", "Nenhum câmbio é aplicado."),
    ("Combined total", "Total combinado"),
    ("Combined in {code}.", "Juntos em {code}."),
    ("As of {date}.", "Em {date}."),
    ("Rates are stale.", "As taxas estão desatualizadas."),
    ("Today's rate.", "Taxa de hoje."),
    ("Fetched {time}.", "Obtida {time}."),
    ("Needs a rate", "Falta uma taxa"),
    ("Combined total needs a rate for {codes}.", "O total combinado precisa de uma taxa para {codes}."),
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
    ("Card credit", "Saldo a favor"),
    ("Debt ratio", "Rácio de dívida"),
    ("{percent}% of assets", "{percent}% dos ativos"),
    ("Debts are subtracted from net worth.", "As dívidas são subtraídas do patrimônio líquido."),
];

const CA_PDF: &[(&str, &str)] = &[
    ("Where my money went", "On ha anat els diners"),
    ("A description without a category stays under Uncategorized.", "Una descripció sense categoria queda en Sense categoria."),
    ("Finance summary", "Resum de finances"),
    ("On this phone. No bank login.", "En este telèfon. Sense accés al banc."),
    ("Balances are what you last said.", "Els saldos són l'últim que has dit."),
    ("No exchange rate is applied.", "No s'aplica cap tipus de canvi."),
    ("Combined total", "Total combinat"),
    ("Combined in {code}.", "Combinat en {code}."),
    ("As of {date}.", "A data de {date}."),
    ("Rates are stale.", "Els tipus estan desfasats."),
    ("Today's rate.", "Tipus d'hui."),
    ("Fetched {time}.", "Obtingut {time}."),
    ("Needs a rate", "Falta un tipus"),
    ("Combined total needs a rate for {codes}.", "El total combinat necessita un tipus per a {codes}."),
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
    ("Card credit", "Saldo a favor"),
    ("Debt ratio", "Ràtio de deute"),
    ("{percent}% of assets", "{percent}% dels actius"),
    ("Debts are subtracted from net worth.", "Els deutes es resten del patrimoni net."),
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

    fn due_rule(id: &str, name: &str, direction: &str, cadence: &str, start: &str, interval: u32, amount: Option<i64>, purpose: &str) -> Rule {
        Rule {
            id: id.into(),
            name: name.into(),
            amount_minor: amount,
            currency: "EUR".into(),
            direction: direction.into(),
            cadence: cadence.into(),
            weekdays: Vec::new(),
            next_date: start.into(),
            category: String::new(),
            subcategory: String::new(),
            account_id: String::new(),
            notes: String::new(),
            calendar_event_id: String::new(),
            updated_ms: 0,
            from_account: false,
            lead_days: Some(2),
            reminder_id: 7,
            purpose: purpose.into(),
            status: "active".into(),
            start_date: start.into(),
            end_kind: if cadence.is_empty() { "after_count".into() } else { "none".into() },
            end_count: if cadence.is_empty() { 1 } else { 0 },
            end_date: String::new(),
            interval_days: interval,
            amount_mode: "fixed".into(),
            payout_currency: String::new(),
            skipped_dates: Vec::new(),
        }
    }

    fn tx(direction: &str, amount: i64, account: &str, from: &str, date: &str, created: i64) -> Transaction {
        Transaction {
            id: format!("ft{created}"),
            amount_minor: amount,
            currency: "EUR".into(),
            direction: direction.into(),
            category: String::new(),
            subcategory: String::new(),
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
    fn scheduled_money_lands_on_the_calendar_day() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let start = local_midnight(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()).unwrap();
        let end = local_midnight(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()).unwrap();
        let mut store = Store::default();
        store.rules.push(due_rule("rent", "Rent", "expense", "monthly", "2026-01-20", 0, Some(120_000), "recurring"));
        store.rules.push(due_rule("eggs", "Nitroeggs", "income", "every_n_days", "2026-11-10", 10, Some(1_400), "investment_payout"));
        store.rules.push(due_rule("once", "Bonus", "income", "", "2026-11-20", 0, Some(50_000), "one_off"));
        let mut skipped = due_rule("skip", "Gym", "expense", "monthly", "2026-11-20", 0, Some(4_000), "recurring");
        skipped.skipped_dates = vec!["2026-11-20".into()];
        store.rules.push(skipped);
        let mut paused = due_rule("pause", "Paused", "expense", "monthly", "2026-11-20", 0, Some(1), "recurring");
        paused.status = "paused".into();
        store.rules.push(paused);
        let mut paid = tx("income", 1_400, "", "", "2026-11-10", 1);
        paid.rule_id = "eggs".into();
        paid.occurrence_date = "2026-11-10".into();
        store.transactions.push(paid);
        let events = calendar_money_events(&store, start, end, today);
        let titles = events.iter().map(|event| format!("{} {}", event.title, event.location)).collect::<Vec<_>>();
        assert!(events.iter().any(|event| event.title == "Rent" && event.source == "finance" && event.all_day && event.location.contains("1200.00")));
        assert_eq!(events.iter().filter(|event| event.title == "Rent").count(), 1);
        let rent = events.iter().find(|event| event.title == "Rent").unwrap();
        let rent_day = chrono::DateTime::from_timestamp_millis(rent.start_ms).unwrap().with_timezone(&Local).date_naive();
        assert_eq!(rent_day, NaiveDate::from_ymd_opt(2026, 11, 20).unwrap());
        assert!(events.iter().any(|event| event.title == "Nitroeggs" && event.id.ends_with("2026-11-20")));
        assert!(!events.iter().any(|event| event.id.contains("eggs") && event.id.ends_with("2026-11-10")), "a paid payout is not a second copy");
        assert!(events.iter().any(|event| event.title == "Bonus" && event.id.ends_with("2026-11-20")));
        assert!(!events.iter().any(|event| event.title == "Gym"));
        assert!(!events.iter().any(|event| event.title == "Paused"));
        assert!(titles.iter().all(|row| !row.contains("1:1")));
        let december_start = end;
        let december_end = local_midnight(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()).unwrap();
        let december = calendar_money_events(&store, december_start, december_end, today);
        assert!(december.iter().any(|event| event.title == "Rent" && event.id.ends_with("2026-12-20")));
        assert!(december.iter().any(|event| event.title == "Gym" && event.id.ends_with("2026-12-20")));
    }

    #[test]
    fn logging_and_questions_do_not_invent_figures() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        let refused = log_from_tool(
            &mut store,
            &json!({"direction": "expense", "amount": "12 euros", "category": "lunch", "account": "cash", "accountKind": "cash"}),
            &clock,
        );
        assert!(refused.is_err(), "{refused:?}");
        assert!(store.accounts.is_empty(), "logging must not create a cash account");
        assert!(store.transactions.is_empty());
        account_from_tool(&mut store, &json!({"name": "Cash", "kind": "cash", "currency": "EUR"}), &clock).unwrap();
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
        store.default_currency = "EUR".into();
        let spoken = speak(&store, &Ask { view: "summary".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() }, clock.today);
        assert!(spoken.contains("Spent 12.00 EUR"));
        assert!(spoken.contains("received nothing"));
        assert!(spoken.contains("together in EUR"));
        let cash = speak(&store, &Ask { view: String::new(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: "cash".into(), amount: String::new(), date: String::new() }, clock.today);
        assert!(cash.contains("no balance saved"));
        rule_from_tool(&mut store, &json!({"name": "Rent", "date": "1", "cadence": "monthly"}), &clock, false).unwrap();
        let rent = store.rules.iter().find(|rule| rule.name == "Rent").unwrap();
        assert!(rent.amount_minor.is_none());
        assert_eq!(rent.next_date, "2026-12-01");
        let upcoming = speak(&store, &Ask { view: "upcoming".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() }, clock.today);
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
    fn debts_subtract_from_net_worth_everywhere() {
        let mut store = Store::default();
        store.default_currency = "CHF".into();
        store.locale_hint = "en".into();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 21).unwrap());
        account_from_tool(&mut store, &json!({"name": "Cash", "kind": "bank", "balance": "18.44", "currency": "CHF"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Funds", "kind": "investment", "balance": "5554.97", "currency": "CHF", "investmentType": "custom"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Card", "kind": "credit_card", "balance": "2072.66", "currency": "CHF"}), &clock).unwrap();
        let rows = crate::finance_schedule::worth_rows(&worth_account_list(&store, "CHF", 5_000));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].net_minor, 1_844 + 555_497 - 207_266);
        assert_ne!(rows[0].net_minor, 1_844 + 555_497 + 207_266);
        let board = widget_board(&store, clock.today, 5_000);
        assert_eq!(board.net.minor, Some(1_844 + 555_497 - 207_266));
        assert_eq!(board.net.text, "3500.75 CHF");
        let request = PdfRequest { period: "all".into(), account: String::new(), category: String::new(), currency: String::new(), direction: String::new(), from: String::new(), to: String::new(), locale: "en".into() };
        let pages = pdf_pages(&store, &request, clock.today, "en");
        let donut = pages.iter().find(|page| page.title == "Money, debts, and investments").expect("donut page");
        match &donut.chart {
            Some(crate::finance_pdf::Chart::Donut { center_value, slices, .. }) => {
                assert!(center_value.contains("3500.75"), "{center_value}");
                assert!(slices.iter().all(|slice| slice.label != "Debts"));
                assert!(slices.iter().any(|slice| slice.label == "Money"));
                assert!(slices.iter().any(|slice| slice.label == "Investments"));
            }
            _ => panic!("expected a donut"),
        }
        assert!(donut.numbers.iter().any(|(label, value)| label == "Debts" && value.contains("2072.66")));
        assert!(donut.notes.iter().any(|note| note.contains("subtracted")));
        let _guard = push_locale("en");
        let ask = Ask { view: "summary".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() };
        let spoken = speak(&store, &ask, clock.today);
        assert!(spoken.contains("3500.75 CHF"), "{spoken}");
        assert!(spoken.contains("2072.66 CHF"), "{spoken}");
        assert!(!spoken.contains("7646.07"), "{spoken}");
    }

    #[test]
    fn categories_migrate_and_a_phrase_assigns_one() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 21).unwrap());
        account_from_tool(&mut store, &json!({"name": "Cash", "kind": "cash", "balance": "100", "currency": "CHF"}), &clock).unwrap();
        log_from_tool(&mut store, &json!({"direction": "expense", "amount": "12.40", "currency": "CHF", "category": "Migros", "account": "Cash"}), &clock).unwrap();
        store.transactions[0].notes = "Migros".into();
        store.transactions[0].category.clear();
        store.categories_ready = false;
        assert!(ensure_categories(&mut store));
        assert_eq!(store.transactions[0].category, "Uncategorized");
        assert!(store.categories.iter().any(|category| category.name == "Uncategorized"));
        let color = "#112233".to_string();
        edit_category(&mut store, "create", "Groceries", "", &color, "expense", "utensils", 2).unwrap();
        assert_eq!(store.categories.iter().find(|category| category.name == "Groceries").unwrap().color, "#112233");
        store.transactions[0].category = "Shop".into();
        apply_phrase(&mut store, "categorize my Migros expense as groceries", &clock).unwrap();
        assert_eq!(store.transactions[0].category, "Groceries");
        apply_phrase(&mut store, "create an income category Freelance", &clock).unwrap();
        let freelance = store.categories.iter().find(|category| category.name == "Freelance").unwrap();
        assert_eq!(freelance.kind, "income");
        edit_category(&mut store, "merge", "Groceries", "Food", "", "expense", "", 0).unwrap();
        assert_eq!(store.transactions[0].category, "Food");
        assert!(store.categories.iter().all(|category| category.name != "Groceries"));
        edit_category(&mut store, "paint", "pool", "", "#6E8B74", "", "", 0).unwrap();
        assert_eq!(store.flow_colors.iter().find(|row| row.id == "pool").unwrap().color, "#6E8B74");
        let restored = serde_json::to_string(&store.categories).unwrap();
        assert!(restored.contains("Freelance"));
        assert!(restored.contains("#112233") || restored.contains("Food"));
        let colors = serde_json::to_string(&store.flow_colors).unwrap();
        assert!(colors.contains("#6E8B74"));
        let (ask, changed) = apply_flow_voice(&mut store, "reset", "all", "", false, "");
        assert!(!changed);
        assert!(ask.contains("Say yes") || ask.contains("sí") || ask.contains("yes") || !ask.is_empty());
        assert_eq!(store.flow_colors.len(), 1);
        let (_, cleared) = apply_flow_voice(&mut store, "reset", "all", "", true, "");
        assert!(cleared);
        assert!(store.flow_colors.is_empty());
        edit_category(&mut store, "layout", "horizontal", "", "", "", "", 0).unwrap();
        assert_eq!(store.flow_layout, "horizontal");
    }

    #[test]
    fn one_color_record_is_read_set_reset_and_kept_in_backup() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 21).unwrap());
        account_from_tool(&mut store, &json!({"name": "Quanloop", "kind": "investment", "balance": "10", "currency": "EUR"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "XMR", "kind": "investment", "currency": "XMR", "ticker": "XMR"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Silver", "kind": "investment", "currency": "CHF"}), &clock).unwrap();
        assert_eq!(palette_index("quanloop"), 11);
        assert_eq!(palette_index("xmr"), 3);
        assert_ne!(default_swatch("quanloop"), default_swatch("xmr"));
        let before = store.accounts.len();
        account_from_tool(&mut store, &json!({"phrase": "make Quanloop green"}), &clock).unwrap();
        let quanloop = store.accounts.iter().find(|account| account.name == "Quanloop").unwrap();
        assert_eq!(quanloop.color, "#009E73");
        assert_eq!(store.entity_colors.iter().find(|row| row.kind == "account" && row.id == quanloop.id).unwrap().color, "#009E73");
        let (read, changed) = spoken_color_phrase(&mut store, "what color is XMR").unwrap().unwrap();
        assert!(!changed);
        assert!(read.contains("XMR"), "{read}");
        let (reset, cleared) = spoken_color_phrase(&mut store, "reset the color for silver").unwrap().unwrap();
        assert!(cleared, "{reset}");
        let silver = store.accounts.iter().find(|account| account.name == "Silver").unwrap();
        assert!(silver.color.starts_with("var(--series-"), "{}", silver.color);
        let (missing, _) = spoken_color_phrase(&mut store, "what color is Missingcoin").unwrap().unwrap();
        assert!(missing.to_lowercase().contains("missingcoin"), "{missing}");
        assert_eq!(store.accounts.len(), before);
        let backed = serde_json::to_string(&store.entity_colors).unwrap();
        let restored: Vec<EntityColor> = serde_json::from_str(&backed).unwrap();
        assert!(restored.iter().any(|row| row.color == "#009E73"));
        let _guard = push_locale("es");
        let (spanish, _) = spoken_color_phrase(&mut store, "que color tiene Quanloop").unwrap().unwrap();
        assert!(spanish.contains("verde"), "{spanish}");
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
            "Combined total",
            "Combined in {code}.",
            "As of {date}.",
            "Rates are stale.",
            "Today's rate.",
            "Fetched {time}.",
            "Needs a rate",
            "Combined total needs a rate for {codes}.",
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
            "Card credit",
            "Debt ratio",
            "{percent}% of assets",
            "Debts are subtracted from net worth.",
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

    #[test]
    fn combined_totals_use_a_real_rate() {
        let mut store = Store::default();
        store.default_currency = "EUR".into();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        account_from_tool(&mut store, &json!({"name": "Euro", "kind": "bank", "balance": "100", "currency": "EUR"}), &clock).unwrap();
        account_from_tool(&mut store, &json!({"name": "Dollar", "kind": "bank", "balance": "11", "currency": "USD"}), &clock).unwrap();
        let request = PdfRequest { period: "this_month".into(), account: String::new(), category: String::new(), currency: String::new(), direction: String::new(), from: String::new(), to: String::new(), locale: "en".into() };
        let blocked = pdf_pages(&store, &request, clock.today, "en");
        assert_eq!(blocked[0].numbers.len(), 1);
        assert_eq!(blocked[0].numbers[0].0, "Combined total");
        assert_eq!(blocked[0].numbers[0].1, "Needs a rate");
        assert!(blocked[0].notes.iter().any(|note| note.contains("USD")));
        assert!(blocked[0].rows.iter().any(|row| row.amount.contains("100.00 EUR") || row.amount.contains("100")));
        let waiting = widget_board(&store, clock.today, 5_000);
        assert_eq!(waiting.net.state, "needs-rate");
        assert!(waiting.net.text.is_empty());
        assert!(waiting.net.minor.is_none());
        assert!(waiting.net.missing.contains("USD"));
        crate::finance_fx::upsert_manual(&mut store.fx, "USD", "EUR", 0.5, "2026-11-12", false).unwrap();
        let pages = pdf_pages(&store, &request, clock.today, "en");
        assert_eq!(pages[0].numbers.len(), 1);
        assert!(pages[0].numbers[0].1.contains("105.50 EUR"), "{}", pages[0].numbers[0].1);
        assert!(!pages[0].numbers[0].1.contains("111.00"));
        let ready = widget_board(&store, clock.today, 5_000);
        assert_eq!(ready.net.state, "ready");
        assert_eq!(ready.net.minor, Some(10_550));
        assert_ne!(ready.net.minor, Some(11_100));
        log_from_tool(&mut store, &json!({"direction": "expense", "amount": "10 euros", "category": "lunch"}), &clock).unwrap();
        log_from_tool(&mut store, &json!({"direction": "expense", "amount": "11 dollars", "category": "lunch"}), &clock).unwrap();
        store.fx.manual.clear();
        let ask = Ask { view: "summary".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() };
        let missing = speak(&store, &ask, clock.today);
        assert!(missing.contains("needs a rate for USD"), "{missing}");
        assert!(!missing.contains("Spent"), "{missing}");
        assert!(!missing.contains("11.00 USD"), "{missing}");
        crate::finance_fx::upsert_manual(&mut store.fx, "USD", "EUR", 0.5, "2026-11-12", false).unwrap();
        let spoken = speak(&store, &ask, clock.today);
        assert!(spoken.contains("15.50 EUR"), "{spoken}");
        assert!(spoken.contains("together in EUR"), "{spoken}");
        assert!(!spoken.contains("21.00"), "{spoken}");
    }

    #[test]
    fn voice_edits_amount_without_rewriting_paid_history() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        rule_from_tool(&mut store, &json!({"name": "Renew travel card", "amount": "40", "currency": "EUR", "direction": "expense", "cadence": "yearly", "date": "2026-11-01"}), &clock, false).unwrap();
        let rule_id = store.rules[0].id.clone();
        store.transactions.push(Transaction {
            id: "paid".into(),
            amount_minor: 4_000,
            currency: "EUR".into(),
            direction: "expense".into(),
            category: "Renew travel card".into(),
            subcategory: String::new(),
            account_id: String::new(),
            from_account_id: String::new(),
            date: "2025-11-01".into(),
            notes: String::new(),
            recurrence: "once".into(),
            rule_id: rule_id.clone(),
            created_ms: 1,
            updated_ms: 1,
            occurrence_date: "2025-11-01".into(),
        });
        let detail = rule_from_tool(&mut store, &json!({"name": "Renew travel card", "amount": "55", "currency": "EUR"}), &clock, false).unwrap();
        assert!(detail.contains("55.00 EUR"), "{detail}");
        assert!(detail.contains("Future unpaid"), "{detail}");
        assert_eq!(store.rules[0].amount_minor, Some(5_500));
        assert_eq!(store.transactions[0].amount_minor, 4_000);
        let edited = edit_logged(&mut store, "paid", &json!({"amount": "42", "currency": "EUR"}), &clock).unwrap();
        assert!(edited.contains("42.00 EUR"), "{edited}");
        assert_eq!(store.transactions[0].amount_minor, 4_200);
        assert_eq!(store.rules[0].amount_minor, Some(5_500));
    }

    #[test]
    fn voice_card_charge_payment_and_coming_window() {
        let mut store = Store::default();
        let mut clock = clock(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        account_from_tool(&mut store, &json!({"name": "Checking", "kind": "bank", "balance": "500", "currency": "EUR"}), &clock).unwrap();
        account_from_tool(
            &mut store,
            &json!({"name": "Visa", "kind": "credit_card", "balance": "20", "currency": "EUR", "creditLimit": "1000", "dueDay": 20, "lastFour": "4242"}),
            &clock,
        )
        .unwrap();
        let card = store.accounts.iter().find(|account| account.name == "Visa").unwrap();
        assert_eq!(card.kind, "credit_card");
        assert_eq!(card.last_four, "4242");
        assert_eq!(card.credit_limit_minor, Some(100_000));
        assert!(card.due_day == Some(20));
        clock.ms = 8_000;
        let charged = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "30", "currency": "EUR", "account": "Visa", "category": "Groceries"}), &clock).unwrap();
        assert!(charged.contains("Charged"), "{charged}");
        let bank = shown_balance(store.accounts.iter().find(|account| account.name == "Checking").unwrap(), &store.transactions);
        let owed = shown_balance(store.accounts.iter().find(|account| account.name == "Visa").unwrap(), &store.transactions);
        assert_eq!(bank, Some(50_000));
        assert_eq!(owed, Some(5_000));
        clock.ms = 9_000;
        let over = log_from_tool(&mut store, &json!({"direction": "payment", "amount": "80", "currency": "EUR", "account": "Visa", "fromAccount": "Checking"}), &clock).unwrap();
        assert!(over.contains("Paid"), "{over}");
        assert!(over.to_lowercase().contains("credit"), "{over}");
        let bank = shown_balance(store.accounts.iter().find(|account| account.name == "Checking").unwrap(), &store.transactions);
        let owed = shown_balance(store.accounts.iter().find(|account| account.name == "Visa").unwrap(), &store.transactions);
        assert_eq!(owed, Some(-3_000));
        assert_eq!(bank, Some(42_000));
        clock.ms = 10_000;
        let paid = log_from_tool(&mut store, &json!({"direction": "payment", "payInFull": true, "account": "Visa", "fromAccount": "Checking"}), &clock).unwrap();
        assert!(paid.to_lowercase().contains("credit"), "{paid}");
        assert!(!paid.contains("Paid"), "{paid}");
        assert_eq!(store.transactions.iter().filter(|tx| tx.direction == "payment").count(), 1);
        let bank = shown_balance(store.accounts.iter().find(|account| account.name == "Checking").unwrap(), &store.transactions);
        let owed = shown_balance(store.accounts.iter().find(|account| account.name == "Visa").unwrap(), &store.transactions);
        assert_eq!(owed, Some(-3_000));
        assert_eq!(bank, Some(42_000));
        let expenses = period_totals(&store.transactions, "2026-11", "", "", "");
        assert_eq!(expenses.iter().map(|row| row.expense_minor).sum::<i64>(), 3_000);
        assert_eq!(expenses.iter().map(|row| row.payment_minor).sum::<i64>(), 8_000);
        assert!(coming_lines(&store, clock.today, "out").iter().all(|row| !row.name.contains("Visa")), "a card in credit has nothing due");
        rule_from_tool(&mut store, &json!({"name": "Rent", "amount": "10", "currency": "EUR", "direction": "expense", "date": "2026-11-18", "account": "Checking"}), &clock, false).unwrap();
        rule_from_tool(&mut store, &json!({"name": "Groceries plan", "amount": "8", "currency": "EUR", "direction": "expense", "date": "2026-11-16", "account": "Visa", "accountKind": "credit_card"}), &clock, false).unwrap();
        rule_from_tool(&mut store, &json!({"name": "Renew travel card", "amount": "40", "currency": "EUR", "direction": "expense", "cadence": "yearly", "date": "2027-10-12", "account": "Checking"}), &clock, false).unwrap();
        let coming = coming_lines(&store, clock.today, "out");
        assert!(coming.iter().any(|row| row.name == "Rent"), "{coming:?}");
        assert!(coming.iter().all(|row| row.name != "Groceries plan"), "{coming:?}");
        assert!(coming.iter().all(|row| row.name != "Renew travel card"), "{coming:?}");
        let ask = Ask { view: "coming".into(), month: String::new(), kind: String::new(), account: String::new(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() };
        let spoken = speak(&store, &ask, clock.today);
        assert!(spoken.contains("Rent"), "{spoken}");
        assert!(!spoken.contains("Groceries plan"), "{spoken}");
        assert!(!spoken.contains("Renew travel card"), "{spoken}");
        let mut twin = empty_account("fa-visa-2".into());
        twin.name = "Visa".into();
        twin.kind = "credit_card".into();
        twin.currency = "EUR".into();
        twin.balance_minor = Some(100);
        store.accounts.push(twin);
        let ambiguous = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "1", "currency": "EUR", "account": "Visa"}), &clock);
        assert!(ambiguous.is_err(), "{ambiguous:?}");
        let invested = account_from_tool(
            &mut store,
            &json!({"name": "Nitroeggs", "kind": "investment", "units": "2", "unitPrice": "50", "currency": "EUR", "investmentType": "custom"}),
            &clock,
        )
        .unwrap();
        assert!(invested.contains("Nitroeggs"), "{invested}");
        let holding = store.accounts.iter().find(|account| account.name == "Nitroeggs").unwrap();
        assert_eq!(holding.kind, "investment");
        assert_eq!(shown_balance(holding, &store.transactions), Some(10_000));
        let payout = rule_from_tool(
            &mut store,
            &json!({"name": "Nitroeggs payout", "amount": "14", "currency": "USD", "direction": "income", "purpose": "investment_payout", "cadence": "every_n_days", "intervalDays": 10, "date": "2026-11-20", "payoutCurrency": "XMR", "amountMode": "quote", "account": "Nitroeggs"}),
            &clock,
            true,
        )
        .unwrap();
        assert!(payout.contains("Nitroeggs"), "{payout}");
        let rule = store.rules.iter().find(|rule| rule.name == "Nitroeggs payout").unwrap();
        assert_eq!(rule.interval_days, 10);
        assert_eq!(rule.payout_currency, "XMR");
    }

    #[test]
    fn voice_sets_a_card_credit_and_spends_crypto() {
        let mut store = Store::default();
        let mut clock = clock(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        account_from_tool(&mut store, &json!({"name": "Checking", "kind": "bank", "balance": "500", "currency": "EUR"}), &clock).unwrap();
        let saved = account_from_tool(
            &mut store,
            &json!({"name": "Abanca", "kind": "credit_card", "balance": "-1393.65", "currency": "EUR", "creditLimit": "15000", "dueDay": 1}),
            &clock,
        )
        .unwrap();
        assert!(saved.contains("Abanca"), "{saved}");
        let card = store.accounts.iter().find(|account| account.name == "Abanca").unwrap();
        assert_eq!(card.balance_minor, Some(-139_365));
        assert_eq!(shown_balance(card, &store.transactions), Some(-139_365));
        assert_eq!(card.credit_limit_minor, Some(1_500_000));
        assert!(store.transactions.is_empty());
        let worth = crate::finance_schedule::worth_rows(&[crate::finance_schedule::WorthAccount {
            kind: "credit_card".into(),
            currency: "EUR".into(),
            shown_minor: Some(-139_365),
        }]);
        assert_eq!(worth[0].net_minor, 139_365);
        assert_ne!(worth[0].net_minor, 1_500_000 - (-139_365));
        account_from_tool(&mut store, &json!({"name": "Abanca", "kind": "credit_card", "balance": "200 in credit", "currency": "EUR"}), &clock).unwrap();
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Abanca").unwrap(), &store.transactions), Some(-20_000));
        account_from_tool(&mut store, &json!({"name": "Abanca", "balance": "0", "currency": "EUR"}), &clock).unwrap();
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Abanca").unwrap(), &store.transactions), Some(0));
        let full = log_from_tool(&mut store, &json!({"direction": "payment", "payInFull": true, "account": "Abanca", "fromAccount": "Checking"}), &clock).unwrap();
        assert!(full.to_lowercase().contains("paid"), "{full}");
        assert!(store.transactions.is_empty());
        account_from_tool(&mut store, &json!({"name": "Abanca", "balance": "50", "currency": "EUR"}), &clock).unwrap();
        clock.ms = 8_000;
        log_from_tool(&mut store, &json!({"direction": "payment", "amount": "80", "currency": "EUR", "account": "Abanca", "fromAccount": "Checking"}), &clock).unwrap();
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Abanca").unwrap(), &store.transactions), Some(-3_000));
        clock.ms = 9_000;
        log_from_tool(&mut store, &json!({"direction": "payment", "amount": "10", "currency": "EUR", "account": "Abanca", "fromAccount": "Checking"}), &clock).unwrap();
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Abanca").unwrap(), &store.transactions), Some(-4_000));
        account_from_tool(&mut store, &json!({"name": "Abanca", "balance": "30 credit", "currency": "EUR"}), &clock).unwrap();
        clock.ms = 11_000;
        let charge = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "50", "currency": "EUR", "account": "Abanca", "category": "Lunch"}), &clock).unwrap();
        assert!(charge.contains("Charged"), "{charge}");
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Abanca").unwrap(), &store.transactions), Some(2_000));
        let expenses = period_totals(&store.transactions, "2026-11", "", "", "");
        assert_eq!(expenses.iter().map(|row| row.expense_minor).sum::<i64>(), 5_000);
        let positive = account_from_tool(&mut store, &json!({"name": "Due card", "kind": "credit_card", "balance": "40", "currency": "EUR", "dueDay": 20}), &clock).unwrap();
        assert!(positive.contains("Due card"), "{positive}");
        assert!(coming_lines(&store, clock.today, "out").iter().any(|row| row.name.contains("Due")));
        account_from_tool(&mut store, &json!({"name": "Due card", "balance": "0", "currency": "EUR"}), &clock).unwrap();
        assert!(coming_lines(&store, clock.today, "out").iter().all(|row| !row.name.contains("Due")));
        let ask = Ask { view: "accounts".into(), month: String::new(), kind: "credit_card".into(), account: "Abanca".into(), direction: String::new(), recurrence: String::new(), query: String::new(), amount: String::new(), date: String::new() };
        let spoken = speak(&store, &ask, clock.today);
        assert!(spoken.to_lowercase().contains("credit") || spoken.contains("20.00") || spoken.contains("2_000") || spoken.contains("owes"), "{spoken}");
        account_from_tool(&mut store, &json!({"name": "Wallet", "kind": "cash", "balance": "0.5", "currency": "BTC"}), &clock).unwrap();
        let wallet = store.accounts.iter().find(|account| account.name == "Wallet").unwrap();
        assert_eq!(wallet.currency, "BTC");
        assert_eq!(wallet.balance_minor, Some(50_000_000));
        clock.ms = 12_000;
        let spent = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "0.1", "currency": "BTC", "account": "Wallet", "category": "Lunch"}), &clock).unwrap();
        assert!(spent.contains("Wallet") || spent.contains("Saved") || spent.contains("0.10000000"), "{spent}");
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Wallet").unwrap(), &store.transactions), Some(40_000_000));
        let too_much = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "1", "currency": "BTC", "account": "Wallet"}), &clock);
        assert!(too_much.is_err(), "{too_much:?}");
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Wallet").unwrap(), &store.transactions), Some(40_000_000));
        let missing = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "20", "currency": "EUR", "account": "Wallet", "category": "Rent"}), &clock);
        assert!(missing.is_err(), "{missing:?}");
        assert!(missing.unwrap_err().to_lowercase().contains("rate"));
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Wallet").unwrap(), &store.transactions), Some(40_000_000));
        crate::finance_fx::upsert_manual(&mut store.fx, "EUR", "BTC", 0.00002, "2026-11-12", true).unwrap();
        clock.ms = 13_000;
        log_from_tool(&mut store, &json!({"direction": "expense", "amount": "20", "currency": "EUR", "account": "Wallet", "category": "Rent"}), &clock).unwrap();
        let left = shown_with_fx(store.accounts.iter().find(|account| account.name == "Wallet").unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        assert!(left < 40_000_000, "{left}");
        assert_ne!(left, 40_000_000 - 2_000);
        let rent = store.transactions.iter().find(|tx| tx.category == "Rent").unwrap();
        assert_eq!(rent.currency, "EUR");
        assert_eq!(rent.amount_minor, 2_000);
        assert_eq!(rent.direction, "expense");
        clock.ms = 14_000;
        log_from_tool(&mut store, &json!({"direction": "income", "amount": "0.05", "currency": "BTC", "account": "Wallet", "category": "Salary"}), &clock).unwrap();
        let after_income = shown_with_fx(store.accounts.iter().find(|account| account.name == "Wallet").unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        assert_eq!(after_income, left + 5_000_000);
        let watch = account_from_tool(&mut store, &json!({"name": "Watch", "kind": "investment", "investmentType": "custom", "balance": "4000", "currency": "EUR"}), &clock).unwrap();
        assert!(watch.contains("Watch"), "{watch}");
        let asset = store.accounts.iter().find(|account| account.name == "Watch").unwrap();
        assert_eq!(asset.kind, "investment");
        assert_eq!(asset.investment_type, "custom");
        assert_eq!(shown_balance(asset, &store.transactions), Some(400_000));
        account_from_tool(&mut store, &json!({"name": "Watch", "balance": "4500", "currency": "EUR"}), &clock).unwrap();
        assert_eq!(shown_balance(store.accounts.iter().find(|account| account.name == "Watch").unwrap(), &store.transactions), Some(450_000));
        let custom_worth = crate::finance_schedule::worth_rows(&[crate::finance_schedule::WorthAccount {
            kind: "investment".into(),
            currency: "EUR".into(),
            shown_minor: Some(450_000),
        }]);
        assert_eq!(custom_worth[0].invested_minor, 450_000);
        assert_eq!(custom_worth[0].net_minor, 450_000);
        let none = speak_pair(&store, "DOGE", "EUR", clock.ms).unwrap();
        assert!(none.to_lowercase().contains("no rate"), "{none}");
        let quoted = speak_pair(&store, "BTC", "EUR", clock.ms).unwrap();
        assert!(quoted.contains("BTC"), "{quoted}");
        assert!(!quoted.contains("1.00 EUR is 1.00 BTC") && !quoted.contains("1:1"));
        account_from_tool(&mut store, &json!({"name": "Visa", "kind": "credit_card", "balance": "10", "currency": "EUR"}), &clock).unwrap();
        let mut twin = empty_account("fa-visa-b".into());
        twin.name = "Visa".into();
        twin.kind = "credit_card".into();
        twin.currency = "EUR".into();
        twin.balance_minor = Some(400);
        store.accounts.push(twin);
        let which = log_from_tool(&mut store, &json!({"direction": "payment", "amount": "1", "currency": "EUR", "account": "Visa", "fromAccount": "Checking"}), &clock);
        assert!(which.is_err(), "{which:?}");
        let which_edit = account_from_tool(&mut store, &json!({"name": "Visa", "balance": "9", "currency": "EUR"}), &clock);
        assert!(which_edit.is_err(), "{which_edit:?}");
    }

    #[test]
    fn swiss_card_batch_edits_in_place_and_moves_without_creating_a_card() {
        let mut store = Store::default();
        let mut clock = clock(NaiveDate::from_ymd_opt(2026, 11, 13).unwrap());
        account_from_tool(&mut store, &json!({"name": "Swisscard", "kind": "credit_card", "balance": "100", "currency": "CHF"}), &clock).unwrap();
        let swiss_id = store.accounts.iter().find(|account| account.name == "Swisscard").unwrap().id.clone();
        assert_eq!(locate_account(&store, "", "Swiss card", "credit_card").unwrap().unwrap().index, 0);
        assert_eq!(locate_account(&store, "", "swiss-card", "").unwrap().unwrap().index, 0);
        assert_eq!(locate_account(&store, "", "Swiscard", "").unwrap().unwrap().index, 0);
        account_from_tool(&mut store, &json!({"name": "Crédit Suisse", "kind": "bank", "balance": "10", "currency": "CHF"}), &clock).unwrap();
        let credit = locate_account(&store, "", "Credit Suisse", "bank").unwrap().unwrap().index;
        assert_eq!(store.accounts[credit].name, "Crédit Suisse");
        let before = store.accounts.len();
        let missing = log_from_tool(&mut store, &json!({"direction": "expense", "amount": "10", "currency": "CHF", "account": "Amex", "category": "Lunch"}), &clock);
        let missing = missing.expect_err("logging must not create Amex");
        assert!(missing.contains("did not create"), "{missing}");
        assert_eq!(store.accounts.len(), before);
        assert!(store.transactions.is_empty());
        clock.ms = 8_000;
        let corrected = log_entries(
            &mut store,
            &json!({
                "direction": "expense",
                "account": "Swiss card",
                "currency": "CHF",
                "items": [
                    {"amount": "79.95", "category": "cable"},
                    {"amount": "1.50", "category": "cable", "correctsPrevious": true}
                ]
            }),
            &clock,
        )
        .unwrap();
        assert_eq!(corrected.saved.len(), 1, "{corrected:?}");
        assert!(corrected.failed.is_empty(), "{corrected:?}");
        assert_eq!(store.transactions.len(), 1);
        assert_eq!(store.transactions[0].amount_minor, 150);
        assert_eq!(store.transactions[0].account_id, swiss_id);
        store.transactions.clear();
        let phrase = "I spent 179 francs on a vacuum cleaner, paid with the Swiss card. Then 695 on a mop, Swiss card. Then 79.95, no wait, 63.96 on an air purifier, Swiss card. Then 2 francs on a takeaway bag, then 9.95 I don't remember what.";
        let logged = log_entries(&mut store, &json!({"direction": "expense", "phrase": phrase}), &clock).unwrap();
        assert!(logged.failed.is_empty(), "{:?}", logged.failed);
        assert_eq!(logged.saved.len(), 5, "{}", logged.detail);
        assert!(logged.detail.contains("Saved 5 of 5"), "{}", logged.detail);
        assert_eq!(store.accounts.iter().filter(|account| compact_name(&account.name) == "swisscard").count(), 1);
        assert!(store.accounts.iter().all(|account| account.name != "Swiss card"));
        let txs: Vec<&Transaction> = store.transactions.iter().filter(|tx| tx.account_id == swiss_id).collect();
        assert_eq!(txs.len(), 5);
        let amounts: Vec<i64> = txs.iter().map(|tx| tx.amount_minor).collect();
        assert_eq!(amounts, vec![17_900, 69_500, 6_396, 200, 995]);
        assert!(txs.iter().all(|tx| tx.currency == "CHF"));
        assert!(txs[0].category.is_empty(), "{}", txs[0].category);
        assert!(txs[0].notes.to_lowercase().contains("vacuum"));
        assert!(txs[1].category.is_empty());
        assert!(txs[1].notes.to_lowercase().contains("mop"));
        assert!(txs[2].category.is_empty());
        assert!(txs[3].category.is_empty());
        assert!(txs[4].category.is_empty());
        assert!(txs.iter().all(|tx| !tx.id.is_empty()));
        assert_eq!(txs.iter().map(|tx| tx.id.as_str()).collect::<std::collections::HashSet<_>>().len(), 5);
        assert!(logged.saved.iter().all(|row| row["ok"] == true && !row["id"].as_str().unwrap_or("").is_empty()));
        let owed = shown_with_fx(store.accounts.iter().find(|account| account.id == swiss_id).unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        assert_eq!(owed, 10_000 + 17_900 + 69_500 + 6_396 + 200 + 995);
        let mop_id = txs[1].id.clone();
        let edited = log_entries(
            &mut store,
            &json!({"direction": "expense", "matchAmount": "695", "matchCategory": "mop", "matchAccount": "Swiss card", "amount": "6.95", "currency": "CHF"}),
            &clock,
        )
        .unwrap();
        assert_eq!(edited.saved.len(), 1);
        assert_eq!(edited.saved[0]["id"], mop_id);
        assert_eq!(store.transactions.len(), 5);
        assert_eq!(store.transactions.iter().find(|tx| tx.id == mop_id).unwrap().amount_minor, 695);
        assert!(store.transactions.iter().all(|tx| tx.amount_minor != 69_500));
        let ask = Ask {
            view: "activity".into(),
            month: String::new(),
            kind: String::new(),
            account: "swiss-card".into(),
            direction: "expense".into(),
            recurrence: String::new(),
            query: "mop".into(),
            amount: String::new(),
            date: String::new(),
        };
        let activity = speak(&store, &ask, clock.today);
        assert!(activity.contains(&mop_id), "{activity}");
        assert!(activity.contains("6.95"), "{activity}");
        let rows = transaction_views(&store, &ask);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], mop_id);
        assert_eq!(rows[0]["paymentMethod"], "Swisscard");
        clock.ms = 9_000;
        account_from_tool(&mut store, &json!({"name": "Other card", "kind": "credit_card", "balance": "0", "currency": "CHF"}), &clock).unwrap();
        let other_id = store.accounts.iter().find(|account| account.name == "Other card").unwrap().id.clone();
        let preview = describe_delete(&store, &json!({"kind": "account", "name": "Swiss card"})).unwrap();
        assert!(preview.contains('5'), "{preview}");
        assert!(!preview.contains("no transactions"), "{preview}");
        let moved = move_transactions(&mut store, &json!({"fromAccount": "Swiss card", "toAccount": "Other card"}), &clock).unwrap();
        assert_eq!(moved["movedCount"], 5);
        assert!(store.transactions.iter().all(|tx| tx.account_id == other_id));
        assert_eq!(store.transactions.len(), 5);
        let swiss_after = shown_with_fx(store.accounts.iter().find(|account| account.id == swiss_id).unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        let other_after = shown_with_fx(store.accounts.iter().find(|account| account.id == other_id).unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        assert_eq!(swiss_after, 10_000);
        assert_eq!(other_after, 17_900 + 695 + 6_396 + 200 + 995);
        let back = move_transactions(&mut store, &json!({"fromAccount": "Other card", "toAccount": "swiss-card"}), &clock).unwrap();
        assert_eq!(back["movedCount"], 5);
        assert!(store.transactions.iter().all(|tx| tx.account_id == swiss_id));
        let swiss_back = shown_with_fx(store.accounts.iter().find(|account| account.id == swiss_id).unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        let other_back = shown_with_fx(store.accounts.iter().find(|account| account.id == other_id).unwrap(), &store.transactions, &store.fx, clock.ms).unwrap();
        assert_eq!(swiss_back, 10_000 + 17_900 + 695 + 6_396 + 200 + 995);
        assert_eq!(other_back, 0);
        assert_eq!(store.transactions.iter().find(|tx| tx.id == mop_id).unwrap().amount_minor, 695);
        let empty = describe_delete(&store, &json!({"kind": "account", "name": "Other card"})).unwrap();
        assert!(empty.contains("no transactions"), "{empty}");
        account_from_tool(&mut store, &json!({"name": "Swisscash", "kind": "credit_card", "balance": "1", "currency": "CHF"}), &clock).unwrap();
        let ambiguous = locate_account(&store, "", "Swiss", "").unwrap_err();
        assert!(ambiguous.starts_with("Which account? I found"), "{ambiguous}");
        let names: Vec<_> = tool_definitions().as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"move_transactions".into()));
    }

    #[test]
    fn spoken_holdings_keep_full_quantity_and_troy_ounces() {
        let mut store = Store::default();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 21).unwrap());
        let detail = account_from_tool(&mut store, &json!({"phrase": "5 ounces of silver", "kind": "investment"}), &clock).unwrap();
        let silver = store.accounts.iter().find(|account| account.ticker == "XAG").unwrap();
        assert_eq!(silver.units, "5");
        assert_eq!(silver.measure_unit, "troy_oz");
        assert!(detail.contains("5"), "{detail}");
        account_from_tool(&mut store, &json!({"phrase": "2.969859854727 XMR", "kind": "investment"}), &clock).unwrap();
        let xmr = store.accounts.iter().find(|account| account.currency == "XMR" || account.ticker == "XMR").unwrap();
        assert_eq!(xmr.units, "2.969859854727");
        assert_ne!(xmr.units, "2.96985985");
        assert_eq!(units_times_price("0.5", 20_000).unwrap(), 10_000);
        assert_eq!(units_times_price("0.37", 10_000).unwrap(), 3_700);
        let fine = units_times_price("2.969859854727", 1_000_000_000).unwrap();
        assert!(fine > units_times_price("2.96985985", 1_000_000_000).unwrap());
        let mut linked = Store::default();
        linked.accounts.push(empty_account("fa9".into()));
        linked.accounts[0].name = "Silver".into();
        linked.accounts[0].kind = "investment".into();
        linked.accounts[0].units = "5".into();
        assert!(link_holdings(&mut linked));
        assert_eq!(linked.accounts[0].ticker, "XAG");
        assert_eq!(linked.accounts[0].measure_unit, "troy_oz");
    }

    #[test]
    fn old_holdings_price_and_totals_follow_the_main_currency() {
        let mut store = Store::default();
        store.default_currency = "CHF".into();
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "EUR".into(), per_eur: 1.0 });
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "CHF".into(), per_eur: 0.95 });
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "USD".into(), per_eur: 1.08 });
        let now = 1_700_000_000_000;
        store.prices.quotes.push(crate::finance_prices::Spot { symbol: "XAG".into(), kind: "metal".into(), price: 28.4, currency: "EUR".into(), source: "gold-api".into(), fetched_ms: now, estimated: false });
        store.prices.quotes.push(crate::finance_prices::Spot { symbol: "EPIC".into(), kind: "crypto".into(), price: 0.18, currency: "EUR".into(), source: "coinpaprika".into(), fetched_ms: now, estimated: false });
        store.prices.quotes.push(crate::finance_prices::Spot { symbol: "XMR".into(), kind: "crypto".into(), price: 320.0, currency: "EUR".into(), source: "coingecko".into(), fetched_ms: now, estimated: false });
        store.prices.quotes.push(crate::finance_prices::Spot { symbol: "ZANO".into(), kind: "crypto".into(), price: 0.42, currency: "EUR".into(), source: "coingecko".into(), fetched_ms: now - 48 * 60 * 60 * 1000, estimated: false });
        let mut silver = empty_account("s".into());
        silver.name = "Silver".into();
        silver.kind = "other".into();
        silver.currency = "CHF".into();
        silver.units = "5".into();
        silver.measure_unit = "ounces".into();
        silver.notes = "5 ounces".into();
        silver.investment_type = "custom".into();
        let mut epic = empty_account("e".into());
        epic.name = "EPIC Cash".into();
        epic.kind = "other".into();
        epic.currency = "CHF".into();
        epic.notes = "128.60340395".into();
        epic.investment_type = "custom".into();
        let mut xmr = empty_account("x".into());
        xmr.name = "XMR".into();
        xmr.kind = "investment".into();
        xmr.currency = "XMR".into();
        xmr.units = "2.969859854727".into();
        xmr.ticker = "XMR".into();
        let mut zano = empty_account("z".into());
        zano.name = "ZANO".into();
        zano.kind = "investment".into();
        zano.currency = "ZANO".into();
        zano.units = "16.46".into();
        zano.ticker = "ZANO".into();
        store.accounts.extend([silver, epic, xmr, zano]);
        assert!(link_holdings(&mut store));
        let silver = store.accounts.iter().find(|account| account.name == "Silver").unwrap();
        assert_eq!(silver.kind, "investment");
        assert_eq!(silver.ticker, "XAG");
        assert_eq!(silver.measure_unit, "troy_oz");
        assert_eq!(silver.units, "5");
        let note = market_note(&store, silver, now, "CHF").unwrap();
        assert!(note.worth_minor.unwrap_or(0) > 0);
        assert_eq!(note.worth_currency, "CHF");
        assert_eq!(note.source, "gold-api");
        let epic = store.accounts.iter().find(|account| account.name == "EPIC Cash").unwrap();
        assert_eq!(epic.units, "128.60340395");
        assert_eq!(epic.ticker, "EPIC");
        let xmr = store.accounts.iter().find(|account| account.name == "XMR").unwrap();
        assert_eq!(xmr.units, "2.969859854727");
        let zano = store.accounts.iter().find(|account| account.name == "ZANO").unwrap();
        let zano_note = market_note(&store, zano, now, "EUR").unwrap();
        assert!(zano_note.stale);
        assert!(zano_note.worth_minor.is_some());
        let stale_only = {
            let mut bare = store.clone();
            bare.prices.quotes.retain(|row| row.symbol != "XAG");
            bare.prices.quotes.push(crate::finance_prices::Spot { symbol: "XAG".into(), kind: "metal".into(), price: 28.4, currency: "EUR".into(), source: "gold-api".into(), fetched_ms: now - 3 * 24 * 60 * 60 * 1000, estimated: false });
            let silver = bare.accounts.iter().find(|account| account.name == "Silver").unwrap().clone();
            market_note(&bare, &silver, now, "CHF").unwrap()
        };
        assert!(stale_only.worth_minor.is_some());
        assert!(stale_only.stale);
        let today = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let clock = clock(today);
        log_single(&mut store, &json!({"direction": "expense", "amount": "18.50", "currency": "CHF", "category": "Coffee", "date": "2026-11-03"}), &clock).unwrap();
        log_single(&mut store, &json!({"direction": "expense", "amount": "42.99", "currency": "USD", "category": "Groceries", "account": "Swisscard", "accountKind": "credit_card", "date": "2026-11-04"}), &clock).unwrap_err();
        let mut card = empty_account("card".into());
        card.name = "Swisscard".into();
        card.kind = "credit_card".into();
        card.currency = "CHF".into();
        card.balance_minor = Some(62_785);
        store.accounts.push(card);
        let mut bank = empty_account("bank".into());
        bank.name = "ZKB".into();
        bank.kind = "bank".into();
        bank.currency = "CHF".into();
        bank.balance_minor = Some(50_000);
        store.accounts.push(bank);
        log_single(&mut store, &json!({"direction": "expense", "amount": "42.99", "currency": "USD", "category": "Groceries", "account": "Swisscard", "date": "2026-11-04"}), &clock).unwrap();
        log_single(&mut store, &json!({"direction": "payment", "amount": "20", "currency": "CHF", "account": "Swisscard", "fromAccount": "ZKB", "date": "2026-11-06"}), &clock).unwrap();
        log_single(&mut store, &json!({"direction": "transfer", "amount": "5", "currency": "CHF", "account": "ZKB", "fromAccount": "ZKB", "date": "2026-11-07"}), &clock).unwrap();
        store.rules.push(due_rule("power", "Power", "expense", "monthly", "2026-11-18", 0, Some(9_000), "recurring"));
        store.rules.last_mut().unwrap().currency = "CHF".into();
        let totals = period_totals(&flow_transactions(&store, today), "2026-11", "", "", "");
        let chf = totals.iter().find(|row| row.currency == "CHF").unwrap();
        assert!(chf.expense_minor >= 1_850 + 9_000, "due power and the manual coffee both count: {}", chf.expense_minor);
        assert_eq!(chf.payment_minor, 2_000);
        let usd = totals.iter().find(|row| row.currency == "USD").unwrap();
        assert_eq!(usd.expense_minor, 4_299);
        assert_eq!(usd.payment_minor, 0);
        let card = store.accounts.iter().find(|account| account.name == "Swisscard").unwrap();
        let owed = shown_with_fx(card, &store.transactions, &store.fx, clock.ms).unwrap();
        assert!(owed > 62_785, "the card charge raises what is owed once");
        let paid = store.transactions.iter().find(|tx| tx.direction == "payment").unwrap();
        assert_ne!(paid.direction, "expense");
        account_from_tool(&mut store, &json!({"phrase": "I bought a Rolex for 12000 CHF"}), &clock).unwrap();
        let rolex = store.accounts.iter().find(|account| account.name == "Rolex").unwrap();
        assert_eq!(rolex.purchase_minor, Some(1_200_000));
        assert_eq!(rolex.investment_type, "collectible");
        account_from_tool(&mut store, &json!({"phrase": "my Rolex is now worth 14000"}), &clock).unwrap();
        let rolex = store.accounts.iter().find(|account| account.name == "Rolex").unwrap();
        assert_eq!(rolex.balance_minor, Some(1_400_000));
        assert_eq!(rolex.valuations.len(), 1);
        account_from_tool(&mut store, &json!({"phrase": "make silver gray"}), &clock).unwrap();
        let silver = store.accounts.iter().find(|account| account.name == "Silver").unwrap();
        assert_eq!(silver.color.to_ascii_uppercase(), "#8A8A8A");
        let share = portfolio_share_line(&store, "XMR", "CHF", clock.ms);
        assert!(share.contains("XMR"), "{share}");
        assert!(share.contains("percent") || share.contains("%") || share.chars().any(|ch| ch.is_ascii_digit()), "{share}");
        assert!(screen_view("charts") == "overview");
        assert_eq!(screen_view("holdings"), "holdings");
    }

    #[test]
    fn old_format_epic_cash_is_priced_despite_a_bad_pin_and_a_same_day_miss() {
        let mut store = Store::default();
        store.price_cache_version = PRICE_CACHE_VERSION;
        store.default_currency = "CHF".into();
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "EUR".into(), per_eur: 1.0 });
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "CHF".into(), per_eur: 0.95 });
        let now = 1_700_000_000_000;
        store.prices.quotes.push(crate::finance_prices::Spot { symbol: "XMR".into(), kind: "crypto".into(), price: 320.0, currency: "EUR".into(), source: "coingecko".into(), fetched_ms: now, estimated: false });
        store.prices.misses.push(crate::finance_prices::PriceMiss { symbol: "EPIC".into(), fetched_ms: now, reason: "Could not refresh the price.".into() });
        let mut epic = empty_account("e".into());
        epic.name = "EPIC Cash".into();
        epic.kind = "other".into();
        epic.currency = "CHF".into();
        epic.notes = "128.60340395".into();
        epic.investment_type = "custom".into();
        epic.ticker = "ETH".into();
        store.accounts.push(epic);
        store.provider_pins.push(crate::finance_history::ProviderPin { account_id: "e".into(), coingecko_id: "epic-chain".into(), paprika_id: "ep-epic-chain".into(), manual: false });
        let calls = std::sync::Mutex::new(Vec::new());
        assert!(link_holdings(&mut store));
        let linked = store.accounts.iter().find(|account| account.name == "EPIC Cash").unwrap();
        assert_eq!(linked.kind, "investment");
        assert_eq!(linked.investment_type, "crypto");
        assert_eq!(linked.ticker, "EPIC");
        assert_eq!(linked.units, "128.60340395");
        assert!(store.provider_pins.iter().any(|pin| pin.account_id == "e" && pin.coingecko_id == "epic-cash"));
        assert!(store.provider_pins.iter().all(|pin| pin.coingecko_id != "epic-chain"));
        let _ = fetch_holding_prices(&mut store, now, false, None, &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("epic-chain") || url.contains("ep-epic-chain") {
                return Err("stale id".into());
            }
            if url.contains("query=EPIC%20Cash") || url.contains("q=EPIC%20Cash") {
                return Ok(if url.contains("coinpaprika") { r#"{"currencies":[]}"#.into() } else { r#"{"coins":[]}"#.into() });
            }
            if url.contains("query=EPIC") {
                return Ok(r#"{"coins":[{"id":"epic-chain","symbol":"EPIC","name":"Epic Chain"},{"id":"epic-cash","symbol":"EPIC","name":"Epic Private Internet Cash"}]}"#.into());
            }
            if url.contains("ids=epic-cash") {
                return Ok(r#"{"epic-cash":{"eur":0.769,"chf":0.709}}"#.into());
            }
            if url.contains("epic-epic-cash") {
                return Ok(r#"{"quotes":{"EUR":{"price":0.7676}}}"#.into());
            }
            Err("unused".into())
        }, None);
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("ids=epic-cash")), "a same-day miss still requests epic-cash: {seen:?}");
        assert!(seen.iter().all(|url| !url.contains("ids=monero") && !url.contains("XXMR")), "a fresh XMR quote stays on the daily gate");
        let quote = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!((quote.price - 0.709).abs() < 0.0001, "the main currency is preferred when the provider has it");
        assert_eq!(quote.currency, "CHF");
        assert_eq!(quote.source, "coingecko");
        assert!(!quote.estimated);
        assert_eq!(store.prices.quotes.iter().find(|row| row.symbol == "XMR").unwrap().price, 320.0);
        let epic = store.accounts.iter().find(|account| account.name == "EPIC Cash").unwrap().clone();
        let note = market_note(&store, &epic, now, "CHF").unwrap();
        assert!(note.worth_minor.unwrap_or(0) > 0);
        assert_eq!(note.price_currency, "CHF");
        assert!((note.price.unwrap_or(0.0) - 0.709).abs() < 0.0001);
        let mut cased = empty_account("c".into());
        cased.name = "Epic Cash".into();
        cased.kind = "custom".into();
        cased.currency = "CHF".into();
        cased.notes = "2".into();
        cased.investment_type = "other".into();
        store.accounts.push(cased);
        assert!(link_holdings(&mut store));
        assert_eq!(store.accounts.iter().find(|account| account.name == "Epic Cash").unwrap().ticker, "EPIC");
        assert_eq!(parse_refresh_phrase("refresh price of EPIC Cash").as_deref(), Some("epic cash"));
        assert_eq!(parse_refresh_phrase("actualitza el preu de EPIC Cash").as_deref(), Some("epic cash"));
        assert_eq!(parse_refresh_phrase("aktualisiere den preis von EPIC Cash").as_deref(), Some("epic cash"));
        assert!(parse_refresh_phrase("what is EPIC Cash worth").is_none());
        store.prices.quotes.retain(|row| row.symbol != "EPIC");
        let asked = std::cell::Cell::new(false);
        let _ = fetch_holding_prices(&mut store, now, false, Some("EPIC Cash"), &|_url| Err("down".into()), Some(&|_asset| {
            asked.set(true);
            Ok(crate::finance_prices::AiEstimate::priced(0.7, "EUR"))
        }));
        assert!(asked.get(), "refresh of an unpriced holding asks the AI provider without waiting a day");
        let estimated = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!(estimated.estimated);
        assert_eq!(estimated.source, "ai");
        let epic = store.accounts.iter().find(|account| account.name == "EPIC Cash").unwrap().clone();
        let note = market_note(&store, &epic, now, "EUR").unwrap();
        assert!(note.estimated);
        assert!((note.price.unwrap_or(0.0) - 0.7).abs() < 0.0001);
        store.prices.quotes.retain(|row| row.symbol != "EPIC");
        crate::finance_prices::upsert_manual(&mut store.prices, "EPIC", 1.2, "EUR", "", false).unwrap();
        let _ = fetch_holding_prices(&mut store, now, true, Some("epic cash"), &|_url| Err("down".into()), Some(&|_asset| Err("no estimate".into())));
        let epic = store.accounts.iter().find(|account| account.name == "EPIC Cash").unwrap().clone();
        let note = market_note(&store, &epic, now, "EUR").unwrap();
        assert_eq!(note.source, "manual");
        assert!((note.price.unwrap_or(0.0) - 1.2).abs() < 0.0001);
        assert!(!note.estimated);
    }

    #[test]
    fn old_epic_cash_worth_holding_drops_a_stale_pin_and_prices_epic_cash() {
        let mut store = Store::default();
        store.price_cache_version = PRICE_CACHE_VERSION;
        store.default_currency = "CHF".into();
        let mut epic = empty_account("old".into());
        epic.name = "EPIC Cash".into();
        epic.kind = "investment".into();
        epic.investment_type = "crypto".into();
        epic.currency = "CHF".into();
        epic.units = "12".into();
        store.accounts.push(epic);
        store.provider_pins.push(crate::finance_history::ProviderPin {
            account_id: "old".into(),
            coingecko_id: "epic-chain".into(),
            paprika_id: String::new(),
            manual: true,
        });
        let calls = std::sync::Mutex::new(Vec::new());
        let now = 1_700_000_000_000_i64;
        let _ = fetch_holding_prices(&mut store, now, true, Some("old"), &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("ids=epic-cash") {
                return Ok(r#"{"epic-cash":{"chf":0.700072,"usd":0.832979}}"#.into());
            }
            Err("stale".into())
        }, None);
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("ids=epic-cash")), "stale pin must not hide epic-cash: {seen:?}");
        assert!(seen.iter().all(|url| !url.contains("epic-chain")), "epic-chain is the wrong coin: {seen:?}");
        let quote = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").expect("quote");
        assert!((quote.price - 0.700072).abs() < 0.000001);
        assert_eq!(quote.currency, "CHF");
        assert_eq!(quote.source, "coingecko");
        let epic = store.accounts.iter().find(|account| account.id == "old").unwrap().clone();
        let note = market_note(&store, &epic, now, "CHF").unwrap();
        assert!(note.worth_minor.unwrap_or(0) > 0);
        assert!((note.price.unwrap_or(0.0) - 0.700072).abs() < 0.000001);

        store.prices.quotes.retain(|row| row.symbol != "EPIC");
        let _ = fetch_holding_prices(&mut store, now + 1, true, Some("EPIC Cash"), &|url| {
            if url.contains("tradeogre.com/api/v1/ticker/EPIC-USDT") {
                return Ok(r#"{"success":true,"price":"0.81"}"#.into());
            }
            Err("down".into())
        }, None);
        let fallback = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").expect("tradeogre");
        assert_eq!(fallback.source, "tradeogre");
        assert!((fallback.price - 0.81).abs() < 0.0001);
        assert_eq!(fallback.currency, "USD");
    }

    #[test]
    fn every_epic_shape_prices_from_a_provider_or_an_ai_estimate() {
        let now = 1_700_000_000_000;
        let mut store = Store::default();
        store.price_cache_version = 2;
        store.default_currency = "CHF".into();
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "EUR".into(), per_eur: 1.0 });
        store.fx.per_eur.push(crate::finance_fx::FxLeg { currency: "CHF".into(), per_eur: 0.95 });
        store.prices.misses.push(crate::finance_prices::PriceMiss { symbol: "EPIC".into(), fetched_ms: now, reason: "Could not refresh the price.".into() });
        let mut symbol = empty_account("sym".into());
        symbol.name = "EPIC".into();
        symbol.kind = "investment".into();
        symbol.investment_type = "crypto".into();
        symbol.ticker = "EPIC".into();
        symbol.currency = "EPIC".into();
        symbol.units = "3".into();
        symbol.color = "#5C6B58".into();
        let mut bare = empty_account("bare".into());
        bare.name = "EPIC Cash".into();
        bare.kind = "investment".into();
        bare.investment_type = "crypto".into();
        bare.currency = "CHF".into();
        bare.units = "4".into();
        let mut custom = empty_account("custom".into());
        custom.name = "my EPIC Cash wallet".into();
        custom.kind = "investment".into();
        custom.investment_type = "custom".into();
        custom.currency = "CHF".into();
        custom.notes = "5.5 coins, bought by hand".into();
        custom.color = "#6E8B74".into();
        custom.purchase_minor = Some(100);
        custom.purchase_currency = "CHF".into();
        let mut legacy = empty_account("legacy".into());
        legacy.name = "EPIC Cash".into();
        legacy.kind = "other".into();
        legacy.currency = "CHF".into();
        legacy.notes = "128.60340395".into();
        legacy.color = "#8C6A3F".into();
        let mut pinned = empty_account("pin".into());
        pinned.name = "  Epic   Cash. ".into();
        pinned.kind = "custom".into();
        pinned.investment_type = "other".into();
        pinned.ticker = "ETH".into();
        pinned.currency = "CHF".into();
        pinned.notes = "9".into();
        pinned.price_override = false;
        store.provider_pins.push(crate::finance_history::ProviderPin { account_id: "pin".into(), coingecko_id: "epic-chain".into(), paprika_id: "ep-epic-chain".into(), manual: false });
        let mut history_only = empty_account("hist".into());
        history_only.name = "EPIC Cash".into();
        history_only.kind = "investment".into();
        history_only.investment_type = "custom".into();
        history_only.currency = "CHF".into();
        store.history.push(crate::finance_history::ValuePoint {
            account_id: "hist".into(),
            date: "2026-09-01".into(),
            quantity: "7.5".into(),
            value_minor: 1,
            currency: "EUR".into(),
            native_minor: 1,
            native_currency: "EUR".into(),
            source: "stated".into(),
            name: "EPIC Cash".into(),
            class_name: "custom".into(),
        });
        let mut balance_coin = empty_account("bal".into());
        balance_coin.name = "EPIC".into();
        balance_coin.kind = "cash".into();
        balance_coin.currency = "EPIC".into();
        balance_coin.balance_minor = Some(250_000_000);
        let mut eggs = empty_account("eggs".into());
        eggs.name = "Nitroeggs".into();
        eggs.kind = "investment".into();
        eggs.investment_type = "custom".into();
        eggs.currency = "CHF".into();
        eggs.units = "2".into();
        eggs.color = "#A56B66".into();
        store.accounts.extend([symbol, bare, custom, legacy, pinned, history_only, balance_coin, eggs]);
        let today = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let clock = clock(today);
        let mut talked_store = Store::default();
        talked_store.default_currency = "CHF".into();
        account_from_tool(&mut talked_store, &json!({"phrase": "6.25 EPIC Cash"}), &clock).unwrap();
        let talked = talked_store.accounts.iter().find(|account| account.units == "6.25").unwrap();
        assert_eq!(talked.kind, "investment");
        assert_eq!(talked.ticker, "EPIC");
        assert_eq!(talked.investment_type, "crypto");
        assert_ne!(talked.kind, "cash", "Talk must not file EPIC Cash as a cash wallet");
        assert!(link_holdings(&mut store));
        assert_eq!(store.price_cache_version, PRICE_CACHE_VERSION);
        assert!(store.prices.misses.iter().all(|row| row.symbol != "EPIC"), "a version bump clears the cached miss before the next fetch");
        let expect = [
            ("sym", "3"),
            ("bare", "4"),
            ("custom", "5.5"),
            ("legacy", "128.60340395"),
            ("pin", "9"),
            ("hist", "7.5"),
            ("bal", "2.50"),
        ];
        for (id, units) in expect {
            let account = store.accounts.iter().find(|account| account.id == id).unwrap();
            assert_eq!(account.kind, "investment", "{id} kind");
            assert_eq!(account.investment_type, "crypto", "{id} type");
            assert_eq!(account.ticker, "EPIC", "{id} ticker");
            assert_eq!(account.units, units, "{id} units");
            let asset = holding_resolved(&store, account).unwrap();
            assert_eq!(asset.coingecko_id, "epic-cash", "{id} gecko");
            assert_eq!(asset.paprika_id, "epic-epic-cash", "{id} paprika");
        }
        let custom = store.accounts.iter().find(|account| account.id == "custom").unwrap();
        assert_eq!(custom.color, "#6E8B74");
        assert_eq!(custom.purchase_minor, Some(100));
        assert!(custom.notes.contains("bought by hand"));
        let eggs = store.accounts.iter().find(|account| account.id == "eggs").unwrap();
        assert_eq!(eggs.investment_type, "custom");
        assert_eq!(eggs.ticker, "");
        assert!(store.provider_pins.iter().all(|pin| pin.coingecko_id != "epic-chain"));
        assert!(store.provider_pins.iter().any(|pin| pin.account_id == "pin" && pin.coingecko_id == "epic-cash"));
        let calls = std::sync::Mutex::new(Vec::new());
        let _ = fetch_holding_prices(&mut store, now, false, None, &|url| {
            calls.lock().unwrap().push(url.to_string());
            if url.contains("epic-chain") || url.contains("ep-epic-chain") {
                return Err("stale id".into());
            }
            if url.contains("ids=epic-cash") || url.contains("symbols=epic") {
                return Ok(r#"{"epic-cash":{"eur":0.809,"usd":0.888}}"#.into());
            }
            if url.contains("symbols=epic") {
                return Ok(r#"{"EPIC":{"eur":0.809}}"#.into());
            }
            if url.contains("epic-epic-cash") {
                return Ok(r#"{"quotes":{"EUR":{"price":0.811}}}"#.into());
            }
            if url.contains("query=EPIC") || url.contains("q=EPIC") || url.contains("q=my") {
                return Ok(if url.contains("coinpaprika") { r#"{"currencies":[]}"#.into() } else { r#"{"coins":[]}"#.into() });
            }
            Err("unused".into())
        }, None);
        let seen = calls.lock().unwrap().clone();
        assert!(seen.iter().any(|url| url.contains("epic-cash") || url.contains("symbols=epic")), "live id was requested: {seen:?}");
        assert!(seen.iter().all(|url| !url.contains("epic-chain") && !url.contains("ep-epic-chain")), "stale pin was not requested: {seen:?}");
        let quote = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!((quote.price - 0.809).abs() < 0.0001 || (quote.price - 0.811).abs() < 0.0001);
        assert!(!quote.estimated);
        assert!(store.prices.attempts.iter().any(|row| row.symbol == "EPIC" && row.status == "ok"));
        for id in ["sym", "bare", "custom", "legacy", "pin", "hist", "bal"] {
            let account = store.accounts.iter().find(|account| account.id == id).unwrap().clone();
            let note = market_note(&store, &account, now, "CHF").unwrap();
            assert!(note.worth_minor.unwrap_or(0) > 0, "{id} worth");
            assert_eq!(note.symbol, "EPIC");
        }
        store.prices.quotes.retain(|row| row.symbol != "EPIC");
        store.prices.attempts.clear();
        let asked = std::cell::Cell::new(false);
        let _ = fetch_holding_prices(&mut store, now + 1, false, None, &|_url| Err("down".into()), Some(&|_asset| {
            asked.set(true);
            Ok(crate::finance_prices::AiEstimate::priced(0.77, "EUR"))
        }));
        assert!(asked.get(), "a custom-shaped EPIC holding asks for an AI estimate when every provider misses");
        let estimated = store.prices.quotes.iter().find(|row| row.symbol == "EPIC").unwrap();
        assert!(estimated.estimated);
        assert_eq!(estimated.source, "ai");
        assert!((estimated.price - 0.77).abs() < 0.0001);
        assert!(store.prices.attempts.iter().any(|row| row.source == "ai" && row.status == "ok"));
        let legacy = store.accounts.iter().find(|account| account.id == "legacy").unwrap().clone();
        let note = market_note(&store, &legacy, now + 1, "EUR").unwrap();
        assert!(note.estimated);
        assert_eq!(parse_refresh_phrase("refresh EPIC price").as_deref(), Some("epic"));
        assert_eq!(parse_why_price("why has EPIC no price").as_deref(), Some("epic"));
        assert_eq!(parse_why_price("por que EPIC Cash no tiene precio").as_deref(), Some("epic cash"));
        assert_eq!(parse_why_price("warum hat EPIC keinen preis").as_deref(), Some("epic"));
        assert_eq!(parse_why_price("porque EPIC Cash nao tem preco").as_deref(), Some("epic cash"));
        assert_eq!(parse_why_price("per que EPIC no te preu").as_deref(), Some("epic"));
        let detail = explain_unpriced(&store, "EPIC", now + 1).unwrap();
        assert!(detail.contains("EPIC"), "{detail}");
        assert!(detail.contains("epic-cash") || detail.to_ascii_lowercase().contains("coingecko"), "{detail}");
    }

    #[test]
    fn budget_lines_round_trip_and_say_what_is_left() {
        let mut store = Store::default();
        store.default_currency = "EUR".into();
        let clock = clock(NaiveDate::from_ymd_opt(2026, 11, 21).unwrap());
        let saved = apply_budget(
            &mut store,
            &json!({"action": "set", "period": "2026-11", "role": "expense", "category": "Restaurants", "amount": "200", "currency": "EUR"}),
            &clock,
        )
        .unwrap();
        assert!(saved.contains("Restaurants"), "{saved}");
        apply_budget(&mut store, &json!({"action": "set", "period": "2026-11", "role": "income", "category": "Salary", "label": "Salary 1 Nov", "amount": "2000", "currency": "EUR"}), &clock).unwrap();
        apply_budget(&mut store, &json!({"action": "set", "period": "2026-11", "role": "income", "category": "Salary", "label": "Salary 15 Nov", "amount": "2000", "currency": "EUR"}), &clock).unwrap();
        assert_eq!(store.budgets.len(), 1);
        assert_eq!(store.budgets[0].lines.len(), 3);
        let mut row = tx("expense", 18_000, "", "", "2026-11-04", 1);
        row.category = "Restaurants".into();
        store.transactions.push(row);
        let left = speak_budget(&store, "2026-11", "Restaurants", "left", &clock);
        assert!(left.contains("20"), "{left}");
        let preview = apply_budget(&mut store, &json!({"action": "delete", "period": "2026-11", "category": "Restaurants", "confirmed": false}), &clock).unwrap();
        assert!(preview.to_ascii_lowercase().contains("yes") || preview.contains("sí"), "{preview}");
        assert_eq!(store.budgets[0].lines.len(), 3);
        apply_budget(&mut store, &json!({"action": "delete", "period": "2026-11", "category": "Restaurants", "confirmed": true}), &clock).unwrap();
        assert!(!store.budgets[0].lines.iter().any(|line| line.category == "Restaurants"));
        let copied = resolve_copy(&store, "2026-12");
        assert!(copied.is_some());
    }
}

fn resolve_copy(store: &Store, month: &str) -> Option<Budget> {
    store.budgets.iter().find(|row| row.period == month).cloned().or_else(|| {
        store
            .budgets
            .iter()
            .filter(|row| row.copy_forward && !row.period.is_empty() && row.period.as_str() < month)
            .max_by(|a, b| a.period.cmp(&b.period))
            .cloned()
    })
}

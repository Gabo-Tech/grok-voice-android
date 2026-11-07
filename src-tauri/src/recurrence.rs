//! Next fire time for one-off, daily, weekly, and yearly reminders.
//! Weekdays use ISO numbers: 1 = Monday … 7 = Sunday. All clocks are local.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// `none`, `once`, `daily`, `weekly`, `monthly`, or `yearly`.
    pub kind: String,
    pub weekdays: Vec<u8>,
    pub hour: u32,
    pub minute: u32,
    pub month: u32,
    pub day: u32,
    /// Set for a one-shot that names a year. Yearly reminders ignore it.
    pub year: Option<i32>,
    pub lead_days: u32,
    /// Last day the series may fire. None means it does not end.
    pub until: Option<NaiveDate>,
}

pub fn normalize_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "daily" | "day" | "every day" | "everyday" => "daily".into(),
        "weekly" | "week" | "weekdays" => "weekly".into(),
        "monthly" | "month" | "every month" => "monthly".into(),
        "yearly" | "annual" | "every year" | "annually" => "yearly".into(),
        "once" | "one-off" | "oneoff" | "single" => "once".into(),
        _ => "none".into(),
    }
}

pub fn is_repeating(kind: &str) -> bool {
    matches!(normalize_kind(kind).as_str(), "daily" | "weekly" | "monthly" | "yearly")
}

/// The soonest fire strictly after `after`.
pub fn next_after(spec: &Spec, after: chrono::DateTime<Local>) -> Option<chrono::DateTime<Local>> {
    let kind = normalize_kind(&spec.kind);
    let hour = spec.hour.min(23);
    let minute = spec.minute.min(59);
    let when = match kind.as_str() {
        "daily" => Some(next_daily(after, hour, minute)),
        "weekly" => Some(next_weekly(after, &spec.weekdays, hour, minute)),
        "monthly" => Some(next_monthly(after, spec.day, hour, minute)),
        "yearly" => next_yearly(after, spec.month, spec.day, hour, minute, spec.lead_days, None),
        "once" => next_once(after, spec),
        _ => None,
    }?;
    if let Some(until) = spec.until {
        if when.date_naive() > until {
            return None;
        }
    }
    Some(when)
}

pub fn parse_clock(value: &str) -> Option<(u32, u32)> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }
    let (body, pm, am) = if let Some(rest) = value.strip_suffix("pm") {
        (rest.trim(), true, false)
    } else if let Some(rest) = value.strip_suffix("p.m.") {
        (rest.trim(), true, false)
    } else if let Some(rest) = value.strip_suffix("am") {
        (rest.trim(), false, true)
    } else if let Some(rest) = value.strip_suffix("a.m.") {
        (rest.trim(), false, true)
    } else {
        (value.as_str(), false, false)
    };
    let mut parts = body.split(':');
    let hour: u32 = parts.next()?.trim().parse().ok()?;
    let minute: u32 = parts.next().unwrap_or("0").trim().parse().ok()?;
    if parts.next().is_some() || minute > 59 || hour > 23 {
        return None;
    }
    let hour = if pm || am {
        if hour == 0 || hour > 12 {
            return None;
        }
        let base = if hour == 12 { 0 } else { hour };
        if pm { base + 12 } else { base }
    } else {
        hour
    };
    Some((hour, minute))
}

/// `MM-DD`, `YYYY-MM-DD`, slash or dot forms, Android's `--MM-DD`, or a month name.
pub fn parse_month_day(value: &str) -> Option<(Option<i32>, u32, u32)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(parsed) = parse_numeric_date(value) {
        return Some(parsed);
    }
    parse_named_date(value)
}

fn parse_numeric_date(value: &str) -> Option<(Option<i32>, u32, u32)> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if value.chars().all(|c| c.is_ascii_digit()) && digits.len() == 8 {
        let year: i32 = digits[0..4].parse().ok()?;
        let month: u32 = digits[4..6].parse().ok()?;
        let day: u32 = digits[6..8].parse().ok()?;
        return valid_date(Some(year), month, day);
    }
    let normalized = value.trim_start_matches('-').replace(['/', '.'], "-");
    let parts: Vec<&str> = normalized.split('-').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [m, d] => {
            let month: u32 = m.parse().ok()?;
            let day: u32 = d.parse().ok()?;
            valid_date(None, month, day)
        }
        [a, b, c] => {
            let first: i32 = a.parse().ok()?;
            let mid: u32 = b.parse().ok()?;
            let last: i32 = c.parse().ok()?;
            if (1900..=2200).contains(&first) {
                valid_date(Some(first), mid, last as u32)
            } else if (1900..=2200).contains(&last) {
                let year = last;
                if first > 12 && mid <= 12 {
                    valid_date(Some(year), mid, first as u32)
                } else {
                    valid_date(Some(year), first as u32, mid)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_named_date(value: &str) -> Option<(Option<i32>, u32, u32)> {
    let words: Vec<&str> = value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();
    let mut year = None;
    let mut month = None;
    let mut day = None;
    for word in words {
        if let Some(named) = month_word(word) {
            if month.is_some() {
                return None;
            }
            month = Some(named);
            continue;
        }
        let Ok(n) = word.parse::<i32>() else { continue };
        if (1900..=2200).contains(&n) && year.is_none() {
            year = Some(n);
        } else if (1..=31).contains(&n) && day.is_none() {
            day = Some(n as u32);
        }
    }
    valid_date(year, month?, day?)
}

fn month_word(value: &str) -> Option<u32> {
    match value.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn valid_date(year: Option<i32>, month: u32, day: u32) -> Option<(Option<i32>, u32, u32)> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if let Some(year) = year {
        if !(1900..=2200).contains(&year) {
            return None;
        }
        return Some((Some(year), month, day));
    }
    Some((None, month, day))
}

pub fn to_rrule(kind: &str, weekdays: &[u8], until_ms: i64) -> String {
    let freq = match normalize_kind(kind).as_str() {
        "daily" => "DAILY",
        "weekly" => "WEEKLY",
        "monthly" => "MONTHLY",
        "yearly" => "YEARLY",
        _ => return String::new(),
    };
    let mut rule = format!("FREQ={freq}");
    if freq == "WEEKLY" && !weekdays.is_empty() {
        let days = weekdays.iter().filter_map(|day| byday(*day)).collect::<Vec<_>>().join(",");
        if !days.is_empty() {
            rule.push_str(&format!(";BYDAY={days}"));
        }
    }
    if until_ms > 0 {
        if let Some(until) = chrono::DateTime::from_timestamp_millis(until_ms) {
            rule.push_str(&format!(";UNTIL={}", until.with_timezone(&chrono::Utc).format("%Y%m%dT%H%M%SZ")));
        }
    }
    rule
}

pub fn from_rrule(rule: &str) -> (String, Vec<u8>, i64) {
    let mut kind = String::new();
    let mut days = Vec::new();
    let mut until_ms = 0;
    for part in rule.split(';') {
        let mut bits = part.splitn(2, '=');
        let key = bits.next().unwrap_or("").trim();
        let value = bits.next().unwrap_or("").trim();
        match key.to_ascii_uppercase().as_str() {
            "FREQ" => {
                kind = match value.to_ascii_uppercase().as_str() {
                    "DAILY" => "daily".into(),
                    "WEEKLY" => "weekly".into(),
                    "MONTHLY" => "monthly".into(),
                    "YEARLY" => "yearly".into(),
                    _ => String::new(),
                }
            }
            "BYDAY" => {
                for token in value.split(',') {
                    if let Some(day) = match token.trim().to_ascii_uppercase().as_str() {
                        "MO" => Some(1),
                        "TU" => Some(2),
                        "WE" => Some(3),
                        "TH" => Some(4),
                        "FR" => Some(5),
                        "SA" => Some(6),
                        "SU" => Some(7),
                        _ => None,
                    } {
                        days.push(day);
                    }
                }
            }
            "UNTIL" if value.len() >= 8 => {
                if let Ok(date) = NaiveDate::parse_from_str(&value[..8], "%Y%m%d") {
                    if let Some(when) = date.and_hms_opt(23, 59, 0).and_then(|naive| naive.and_local_timezone(Local).earliest()) {
                        until_ms = when.timestamp_millis();
                    }
                }
            }
            _ => {}
        }
    }
    (kind, days, until_ms)
}

fn byday(day: u8) -> Option<&'static str> {
    match day {
        1 => Some("MO"),
        2 => Some("TU"),
        3 => Some("WE"),
        4 => Some("TH"),
        5 => Some("FR"),
        6 => Some("SA"),
        7 => Some("SU"),
        _ => None,
    }
}

pub fn format_month_day(year: Option<i32>, month: u32, day: u32) -> String {
    match year {
        Some(year) => format!("{year:04}-{month:02}-{day:02}"),
        None => format!("{month:02}-{day:02}"),
    }
}

pub fn parse_weekdays(values: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        for part in value.split([',', ' ']) {
            if let Some(day) = weekday_token(part) {
                if !out.contains(&day) {
                    out.push(day);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

pub fn weekday_token(value: &str) -> Option<u8> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "mon" | "monday" => Some(1),
        "2" | "tue" | "tues" | "tuesday" => Some(2),
        "3" | "wed" | "wednesday" => Some(3),
        "4" | "thu" | "thur" | "thurs" | "thursday" => Some(4),
        "5" | "fri" | "friday" => Some(5),
        "6" | "sat" | "saturday" => Some(6),
        "7" | "sun" | "sunday" => Some(7),
        _ => None,
    }
}

pub fn weekday_label(day: u8) -> &'static str {
    match day {
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        7 => "Sun",
        _ => "",
    }
}

pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

pub fn speak_date(month: u32, day: u32) -> String {
    format!("{} {day}", month_name(month))
}

/// Snooze target. A bare hour such as "5" is the soonest future 5:00 or 17:00.
pub fn snooze_when(
    minutes: Option<u32>,
    until: &str,
    now: chrono::DateTime<Local>,
) -> Result<chrono::DateTime<Local>, String> {
    let until = until.trim();
    if minutes.is_none() && until.is_empty() {
        return Err("Say how long to snooze, or a time.".into());
    }
    if let Some(minutes) = minutes {
        if until.is_empty() {
            if !(1..=24 * 60).contains(&minutes) {
                return Err("Snooze between 1 minute and 24 hours.".into());
            }
            return Ok(now + Duration::minutes(minutes as i64));
        }
        return Err("Say either a delay or a clock time.".into());
    }
    if let Ok(fixed) = chrono::DateTime::parse_from_rfc3339(until) {
        let when = fixed.with_timezone(&Local);
        return if when > now + Duration::seconds(30) {
            Ok(when)
        } else {
            Err("That time is in the past.".into())
        };
    }
    if let Some(when) = parse_local_date_time(until) {
        return if when > now + Duration::seconds(30) {
            Ok(when)
        } else {
            Err("That time is in the past.".into())
        };
    }
    let (hour, minute, meridiem) = parse_loose_clock(until).ok_or_else(|| format!("Could not read '{until}'."))?;
    soonest_clock(now, hour, minute, meridiem).ok_or_else(|| "That time is in the past.".into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Meridiem {
    None,
    Am,
    Pm,
}

fn parse_loose_clock(value: &str) -> Option<(u32, u32, Meridiem)> {
    let value = value.trim().to_ascii_lowercase();
    let (body, meridiem) = if let Some(rest) = value.strip_suffix("pm").or_else(|| value.strip_suffix("p.m.")) {
        (rest.trim(), Meridiem::Pm)
    } else if let Some(rest) = value.strip_suffix("am").or_else(|| value.strip_suffix("a.m.")) {
        (rest.trim(), Meridiem::Am)
    } else {
        (value.as_str(), Meridiem::None)
    };
    let mut parts = body.split(':');
    let hour: u32 = parts.next()?.trim().parse().ok()?;
    let minute: u32 = parts.next().unwrap_or("0").trim().parse().ok()?;
    if parts.next().is_some() || minute > 59 {
        return None;
    }
    Some((hour, minute, meridiem))
}

fn soonest_clock(
    now: chrono::DateTime<Local>,
    hour: u32,
    minute: u32,
    meridiem: Meridiem,
) -> Option<chrono::DateTime<Local>> {
    let hours: Vec<u32> = match meridiem {
        Meridiem::Am => {
            if hour == 12 { vec![0] } else if (1..=11).contains(&hour) { vec![hour] } else { return None }
        }
        Meridiem::Pm => {
            if hour == 12 { vec![12] } else if (1..=11).contains(&hour) { vec![hour + 12] } else { return None }
        }
        Meridiem::None => {
            if hour > 23 {
                return None;
            }
            if (1..=11).contains(&hour) {
                vec![hour, hour + 12]
            } else {
                vec![hour]
            }
        }
    };
    let mut best: Option<chrono::DateTime<Local>> = None;
    for hour in hours {
        for add in 0..3 {
            let date = now.date_naive() + Duration::days(add);
            let Some(when) = local_hms(date, hour, minute) else { continue };
            if when <= now + Duration::seconds(30) {
                continue;
            }
            if best.is_none_or(|current| when < current) {
                best = Some(when);
            }
        }
    }
    best
}

fn next_daily(after: chrono::DateTime<Local>, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let today = local_hms(after.date_naive(), hour, minute).unwrap_or(after + Duration::days(1));
    if today > after {
        today
    } else {
        local_hms(after.date_naive() + Duration::days(1), hour, minute).unwrap_or(today + Duration::days(1))
    }
}

fn next_weekly(after: chrono::DateTime<Local>, weekdays: &[u8], hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let mut days: Vec<u8> = weekdays.iter().copied().filter(|d| (1..=7).contains(d)).collect();
    if days.is_empty() {
        days.push(after.weekday().number_from_monday() as u8);
    }
    days.sort_unstable();
    days.dedup();
    for add in 0..14 {
        let date = after.date_naive() + Duration::days(add);
        let weekday = date.weekday().number_from_monday() as u8;
        if !days.contains(&weekday) {
            continue;
        }
        if let Some(when) = local_hms(date, hour, minute) {
            if when > after {
                return when;
            }
        }
    }
    after + Duration::days(7)
}

fn next_monthly(after: chrono::DateTime<Local>, day: u32, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let mut year = after.year();
    let mut month = after.month();
    for _ in 0..36 {
        if let Some(date) = clamped_month_day(year, month, day.max(1)) {
            if let Some(when) = local_hms(date, hour, minute) {
                if when > after {
                    return when;
                }
            }
        }
        month += 1;
        if month == 13 {
            month = 1;
            year += 1;
        }
    }
    after + Duration::days(31)
}

fn clamped_month_day(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last = next - Duration::days(1);
    NaiveDate::from_ymd_opt(year, month, day.min(last.day()))
}

fn next_yearly(
    after: chrono::DateTime<Local>,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    lead_days: u32,
    only_year: Option<i32>,
) -> Option<chrono::DateTime<Local>> {
    if !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let start = only_year.unwrap_or(after.year());
    let end = only_year.unwrap_or(after.year() + 8);
    for year in start..=end {
        let event = date_in_year(year, month, day)?;
        let fire_day = event - Duration::days(lead_days as i64);
        let when = local_hms(fire_day, hour, minute)?;
        if when > after {
            return Some(when);
        }
        if only_year.is_some() {
            return None;
        }
    }
    None
}

fn next_once(after: chrono::DateTime<Local>, spec: &Spec) -> Option<chrono::DateTime<Local>> {
    if spec.month == 0 {
        return None;
    }
    next_yearly(after, spec.month, spec.day, spec.hour, spec.minute, spec.lead_days, spec.year)
}

/// When a person date should ring next.
///
/// Inside the heads-up window (the notify day through the event day) this is
/// the current occurrence, including after that clock time has passed. Outside
/// the window it is the next future notify time, which is what the alarm uses.
pub fn notify_anchor(spec: &Spec, now: chrono::DateTime<Local>) -> Option<chrono::DateTime<Local>> {
    if let Some(current) = open_window_notify(spec, now.date_naive()) {
        return Some(current);
    }
    next_after(spec, now)
}

fn open_window_notify(spec: &Spec, today: NaiveDate) -> Option<chrono::DateTime<Local>> {
    let kind = normalize_kind(&spec.kind);
    if kind != "yearly" && kind != "once" {
        return None;
    }
    let hour = spec.hour.min(23);
    let minute = spec.minute.min(59);
    let lead = spec.lead_days as i64;
    let years = if kind == "once" {
        vec![spec.year?]
    } else {
        vec![today.year() - 1, today.year(), today.year() + 1]
    };
    for year in years {
        let Some(event) = date_in_year(year, spec.month, spec.day) else { continue };
        let fire_day = event - Duration::days(lead);
        if today >= fire_day && today <= event {
            return local_hms(fire_day, hour, minute);
        }
    }
    None
}

fn date_in_year(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
        return Some(date);
    }
    if month == 2 && day == 29 {
        return NaiveDate::from_ymd_opt(year, 2, 28);
    }
    None
}

fn local_hms(date: NaiveDate, hour: u32, minute: u32) -> Option<chrono::DateTime<Local>> {
    let naive = date.and_time(NaiveTime::from_hms_opt(hour, minute, 0)?);
    naive.and_local_timezone(Local).earliest().or_else(|| naive.and_local_timezone(Local).latest())
}

fn parse_local_date_time(value: &str) -> Option<chrono::DateTime<Local>> {
    let naive = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    naive.and_local_timezone(Local).earliest().or_else(|| naive.and_local_timezone(Local).latest())
}

#[cfg(test)]
pub fn local_on(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    local_hms(NaiveDate::from_ymd_opt(year, month, day).expect("date"), hour, minute).expect("local")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<Local> {
        local_on(y, m, d, h, min)
    }

    #[test]
    fn daily_and_weekly_patterns() {
        let after = t(2026, 3, 3, 10, 0);
        let daily_later = Spec {
            kind: "daily".into(),
            weekdays: vec![],
            hour: 11,
            minute: 0,
            month: 0,
            day: 0,
            year: None,
            lead_days: 0,
            until: None,
        };
        assert_eq!(next_after(&daily_later, after).unwrap(), t(2026, 3, 3, 11, 0));
        let daily_morning = Spec { hour: 9, ..daily_later.clone() };
        assert_eq!(next_after(&daily_morning, after).unwrap(), t(2026, 3, 4, 9, 0));

        let standup = Spec {
            kind: "weekly".into(),
            weekdays: vec![1, 3, 5],
            hour: 9,
            minute: 0,
            month: 0,
            day: 0,
            year: None,
            lead_days: 0,
            until: None,
        };
        let next = next_after(&standup, after).unwrap();
        assert_eq!(next, t(2026, 3, 4, 9, 0));
        assert_eq!(next.weekday().number_from_monday(), 3);
    }

    #[test]
    fn yearly_lead_days_and_once() {
        let birthday = Spec {
            kind: "yearly".into(),
            weekdays: vec![],
            hour: 9,
            minute: 0,
            month: 3,
            day: 3,
            year: None,
            lead_days: 1,
            until: None,
        };
        assert_eq!(next_after(&birthday, t(2026, 2, 1, 8, 0)).unwrap(), t(2026, 3, 2, 9, 0));
        assert_eq!(next_after(&birthday, t(2026, 3, 2, 10, 0)).unwrap(), t(2027, 3, 2, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 3, 2, 8, 0)).unwrap(), t(2026, 3, 2, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 3, 3, 10, 0)).unwrap(), t(2026, 3, 2, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 3, 4, 10, 0)).unwrap(), t(2027, 3, 2, 9, 0));
        let same_day = Spec { lead_days: 0, ..birthday };
        assert_eq!(next_after(&same_day, t(2026, 3, 3, 10, 0)).unwrap(), t(2027, 3, 3, 9, 0));

        let once = Spec {
            kind: "once".into(),
            weekdays: vec![],
            hour: 15,
            minute: 0,
            month: 3,
            day: 3,
            year: Some(2026),
            lead_days: 0,
            until: None,
        };
        assert_eq!(next_after(&once, t(2026, 3, 3, 10, 0)).unwrap(), t(2026, 3, 3, 15, 0));
        assert!(next_after(&once, t(2026, 3, 3, 16, 0)).is_none());
        assert_eq!(notify_anchor(&once, t(2026, 3, 3, 10, 0)).unwrap(), t(2026, 3, 3, 15, 0));
        assert!(notify_anchor(&once, t(2026, 3, 4, 8, 0)).is_none());
    }

    #[test]
    fn notify_window_wraps_the_year_and_waits_seven_days() {
        let birthday = Spec {
            kind: "yearly".into(),
            weekdays: vec![],
            hour: 9,
            minute: 0,
            month: 6,
            day: 15,
            year: None,
            lead_days: 7,
            until: None,
        };
        assert_eq!(notify_anchor(&birthday, t(2026, 3, 3, 10, 0)).unwrap(), t(2026, 6, 8, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 6, 8, 8, 0)).unwrap(), t(2026, 6, 8, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 6, 12, 15, 0)).unwrap(), t(2026, 6, 8, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 6, 15, 18, 0)).unwrap(), t(2026, 6, 8, 9, 0));
        assert_eq!(notify_anchor(&birthday, t(2026, 6, 16, 8, 0)).unwrap(), t(2027, 6, 8, 9, 0));

        let new_year = Spec { month: 1, day: 2, ..birthday };
        assert_eq!(notify_anchor(&new_year, t(2026, 12, 20, 10, 0)).unwrap(), t(2026, 12, 26, 9, 0));
        assert_eq!(notify_anchor(&new_year, t(2026, 12, 28, 10, 0)).unwrap(), t(2026, 12, 26, 9, 0));
        assert_eq!(notify_anchor(&new_year, t(2027, 1, 2, 12, 0)).unwrap(), t(2026, 12, 26, 9, 0));
        assert_eq!(notify_anchor(&new_year, t(2027, 1, 3, 8, 0)).unwrap(), t(2027, 12, 26, 9, 0));
    }

    #[test]
    fn leap_day_falls_back_to_february_28() {
        let leap = Spec {
            kind: "yearly".into(),
            weekdays: vec![],
            hour: 9,
            minute: 0,
            month: 2,
            day: 29,
            year: None,
            lead_days: 0,
            until: None,
        };
        assert_eq!(next_after(&leap, t(2027, 1, 1, 0, 0)).unwrap(), t(2027, 2, 28, 9, 0));
    }

    #[test]
    fn monthly_respects_the_end_date() {
        let rent = Spec {
            kind: "monthly".into(),
            weekdays: vec![],
            hour: 9,
            minute: 0,
            month: 0,
            day: 31,
            year: None,
            lead_days: 0,
            until: Some(NaiveDate::from_ymd_opt(2026, 4, 30).unwrap()),
        };
        assert_eq!(next_after(&rent, t(2026, 1, 1, 0, 0)).unwrap(), t(2026, 1, 31, 9, 0));
        assert_eq!(next_after(&rent, t(2026, 1, 31, 10, 0)).unwrap(), t(2026, 2, 28, 9, 0));
        assert!(next_after(&rent, t(2026, 4, 30, 10, 0)).is_none());
        let rule = to_rrule("weekly", &[1, 3], 0);
        assert_eq!(rule, "FREQ=WEEKLY;BYDAY=MO,WE");
        let (kind, days, until) = from_rrule("FREQ=MONTHLY;UNTIL=20260430T235959Z");
        assert_eq!(kind, "monthly");
        assert!(days.is_empty());
        assert!(until > 0);
    }

    #[test]
    fn dates_accept_contact_and_typed_forms() {
        assert_eq!(parse_month_day("03-03"), Some((None, 3, 3)));
        assert_eq!(parse_month_day("--06-01"), Some((None, 6, 1)));
        assert_eq!(parse_month_day("6/1"), Some((None, 6, 1)));
        assert_eq!(parse_month_day("2026-06-01"), Some((Some(2026), 6, 1)));
        assert_eq!(parse_month_day("01.06.2026"), Some((Some(2026), 1, 6)));
        assert_eq!(parse_month_day("20260601"), Some((Some(2026), 6, 1)));
        assert_eq!(parse_month_day("June 1"), Some((None, 6, 1)));
        assert_eq!(parse_month_day("1 June 2028"), Some((Some(2028), 6, 1)));
        assert!(parse_month_day("not a date").is_none());
    }

    #[test]
    fn snooze_minutes_and_bare_hour() {
        let afternoon = t(2026, 3, 3, 15, 0);
        let ten = snooze_when(Some(10), "", afternoon).unwrap();
        assert_eq!(ten, t(2026, 3, 3, 15, 10));
        let until_five = snooze_when(None, "5", afternoon).unwrap();
        assert_eq!(until_five, t(2026, 3, 3, 17, 0));
        let evening = t(2026, 3, 3, 18, 0);
        assert_eq!(snooze_when(None, "5", evening).unwrap(), t(2026, 3, 4, 5, 0));
        assert_eq!(snooze_when(None, "5pm", t(2026, 3, 3, 12, 0)).unwrap(), t(2026, 3, 3, 17, 0));
        assert!(snooze_when(Some(10), "5", afternoon).is_err());
    }
}

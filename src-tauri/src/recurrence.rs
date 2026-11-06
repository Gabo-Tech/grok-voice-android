//! Next fire time for one-off, daily, weekly, and yearly reminders.
//! Weekdays use ISO numbers: 1 = Monday … 7 = Sunday. All clocks are local.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// `none`, `once`, `daily`, `weekly`, or `yearly`.
    pub kind: String,
    pub weekdays: Vec<u8>,
    pub hour: u32,
    pub minute: u32,
    pub month: u32,
    pub day: u32,
    /// Set for a one-shot that names a year. Yearly reminders ignore it.
    pub year: Option<i32>,
    pub lead_days: u32,
}

pub fn normalize_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "daily" | "day" | "every day" | "everyday" => "daily".into(),
        "weekly" | "week" | "weekdays" | "custom" => "weekly".into(),
        "yearly" | "annual" | "every year" | "annually" => "yearly".into(),
        "once" | "one-off" | "oneoff" | "single" => "once".into(),
        _ => "none".into(),
    }
}

pub fn is_repeating(kind: &str) -> bool {
    matches!(normalize_kind(kind).as_str(), "daily" | "weekly" | "yearly")
}

/// The soonest fire strictly after `after`.
pub fn next_after(spec: &Spec, after: chrono::DateTime<Local>) -> Option<chrono::DateTime<Local>> {
    let kind = normalize_kind(&spec.kind);
    let hour = spec.hour.min(23);
    let minute = spec.minute.min(59);
    match kind.as_str() {
        "daily" => Some(next_daily(after, hour, minute)),
        "weekly" => Some(next_weekly(after, &spec.weekdays, hour, minute)),
        "yearly" => next_yearly(after, spec.month, spec.day, hour, minute, spec.lead_days, None),
        "once" => next_once(after, spec),
        _ => None,
    }
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

/// `MM-DD` or `YYYY-MM-DD`.
pub fn parse_month_day(value: &str) -> Option<(Option<i32>, u32, u32)> {
    let value = value.trim();
    let parts: Vec<&str> = value.split('-').collect();
    match parts.as_slice() {
        [m, d] => {
            let month: u32 = m.parse().ok()?;
            let day: u32 = d.parse().ok()?;
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                Some((None, month, day))
            } else {
                None
            }
        }
        [y, m, d] => {
            let year: i32 = y.parse().ok()?;
            let month: u32 = m.parse().ok()?;
            let day: u32 = d.parse().ok()?;
            if (1..=12).contains(&month) && (1..=31).contains(&day) && (1900..=2200).contains(&year) {
                Some((Some(year), month, day))
            } else {
                None
            }
        }
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
        };
        assert_eq!(next_after(&birthday, t(2026, 2, 1, 8, 0)).unwrap(), t(2026, 3, 2, 9, 0));
        assert_eq!(next_after(&birthday, t(2026, 3, 2, 10, 0)).unwrap(), t(2027, 3, 2, 9, 0));
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
        };
        assert_eq!(next_after(&once, t(2026, 3, 3, 10, 0)).unwrap(), t(2026, 3, 3, 15, 0));
        assert!(next_after(&once, t(2026, 3, 3, 16, 0)).is_none());
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
        };
        assert_eq!(next_after(&leap, t(2027, 1, 1, 0, 0)).unwrap(), t(2027, 2, 28, 9, 0));
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

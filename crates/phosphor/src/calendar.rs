//! Calendar schedules (MadMapper's Calendar Scheduler): an `.ics` file's events start the cues with the same name.
//! Events can repeat daily or weekly (RRULE FREQ=DAILY / WEEKLY, BYDAY, UNTIL). Times are compared as local
//! wall-clock seconds; a time ending in Z (UTC) is moved to local time.

/// One event: when it first starts (local wall-clock seconds since 1970), its title, how it repeats.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start: i64,
    pub title: String,
    pub repeat: Repeat,
    /// Repeats stop after this (local seconds), if set.
    pub until: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Repeat {
    Once,
    Daily,
    /// On these weekdays (0 = Sunday); empty = the start's own weekday.
    Weekly(Vec<u8>),
}

/// Days since 1970-01-01 for a date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn weekday(days: i64) -> u8 {
    // 1970-01-01 was a Thursday.
    (days + 4).rem_euclid(7) as u8
}

/// "20261022T180000" / "20261022T180000Z" → seconds since 1970 (as written: local, or UTC when it ends in Z).
fn parse_time(v: &str, utc_offset: i64) -> Option<i64> {
    let v = v.trim();
    let (date, time) = v.split_once('T').unwrap_or((v, "000000"));
    if date.len() < 8 {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let days = days_from_civil(num(&date[0..4])?, num(&date[4..6])?, num(&date[6..8])?);
    let t = time.trim_end_matches('Z');
    let secs = if t.len() >= 6 { num(&t[0..2])? * 3600 + num(&t[2..4])? * 60 + num(&t[4..6])? } else { 0 };
    let at = days * 86_400 + secs;
    Some(if time.ends_with('Z') { at + utc_offset } else { at })
}

/// The events in an .ics file. `utc_offset` = local time minus UTC, in seconds (for times written in UTC).
pub fn parse(text: &str, utc_offset: i64) -> Vec<Event> {
    // Long lines are folded: a line starting with a space continues the one before.
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        match (l.starts_with([' ', '\t']), lines.last_mut()) {
            (true, Some(last)) => last.push_str(&l[1..]),
            _ => lines.push(l.trim_end_matches('\r').to_string()),
        }
    }
    let mut out = Vec::new();
    let mut cur: Option<(Option<i64>, String, Repeat, Option<i64>)> = None;
    for l in &lines {
        let (key, value) = match l.split_once(':') {
            Some(kv) => kv,
            None => continue,
        };
        let name = key.split(';').next().unwrap_or("");
        match (name, cur.as_mut()) {
            ("BEGIN", _) if value == "VEVENT" => cur = Some((None, String::new(), Repeat::Once, None)),
            ("END", Some(_)) if value == "VEVENT" => {
                if let Some((Some(start), title, repeat, until)) = cur.take()
                    && !title.is_empty()
                {
                    out.push(Event { start, title, repeat, until });
                }
            }
            // All-day events (VALUE=DATE) have no time to start a cue at: skipped.
            ("DTSTART", Some(e)) if !key.contains("VALUE=DATE") || key.contains("DATE-TIME") => e.0 = parse_time(value, utc_offset),
            ("SUMMARY", Some(e)) => e.1 = value.replace("\\,", ",").replace("\\;", ";").trim().to_string(),
            ("RRULE", Some(e)) => {
                let part = |k: &str| value.split(';').find_map(|p| p.strip_prefix(k)).map(str::to_string);
                e.2 = match part("FREQ=").as_deref() {
                    Some("DAILY") => Repeat::Daily,
                    Some("WEEKLY") => Repeat::Weekly(part("BYDAY=").map_or(Vec::new(), |d| {
                        d.split(',').filter_map(|w| ["SU", "MO", "TU", "WE", "TH", "FR", "SA"].iter().position(|x| w.ends_with(x)).map(|i| i as u8)).collect()
                    })),
                    _ => Repeat::Once,
                };
                e.3 = part("UNTIL=").and_then(|u| parse_time(&u, utc_offset));
            }
            _ => {}
        }
    }
    out
}

impl Event {
    /// Does it start in (`prev`, `now`] (local seconds)? Repeats are checked for today and yesterday.
    pub fn starts_between(&self, prev: i64, now: i64) -> bool {
        let hit = |t: i64| t > prev && t <= now && t >= self.start && self.until.is_none_or(|u| t <= u);
        let tod = self.start.rem_euclid(86_400);
        let today = now.div_euclid(86_400);
        match &self.repeat {
            Repeat::Once => hit(self.start),
            Repeat::Daily => [today, today - 1].iter().any(|d| hit(d * 86_400 + tod)),
            Repeat::Weekly(days) => {
                let days: Vec<u8> = if days.is_empty() { vec![weekday(self.start.div_euclid(86_400))] } else { days.clone() };
                [today, today - 1].iter().any(|d| days.contains(&weekday(*d)) && hit(d * 86_400 + tod))
            }
        }
    }
}

/// Now as local wall-clock seconds since 1970, and the local offset from UTC (seconds).
#[cfg(not(target_arch = "wasm32"))]
pub fn local_now() -> (i64, i64) {
    let utc = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (local_tod, _) = crate::clock::local_time();
    let offset = (local_tod as i64 - utc.rem_euclid(86_400) + 43_200).rem_euclid(86_400) - 43_200;
    (utc + offset, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261022T180000\r\nSUMMARY:Lights up\r\nRRULE:FREQ=DAILY\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;TZID=Europe/Paris:20261023T020000\r\nSUMMARY:Night\r\n  mode\r\nRRULE:FREQ=WEEKLY;BYDAY=FR,SA\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261024T120000Z\r\nSUMMARY:Noon UTC\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261025\r\nSUMMARY:All day\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn events_and_repeats() {
        let ev = parse(ICS, 3600);
        assert_eq!(ev.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), vec!["Lights up", "Night mode", "Noon UTC"], "folded title; all-day skipped");
        let at = |d: i64, h: i64| days_from_civil(2026, 10, d) * 86_400 + h * 3600;
        // Daily at 18:00 from the 22nd: fires on the 25th as it passes 18:00, not before the start date.
        assert!(ev[0].starts_between(at(25, 18) - 1, at(25, 18)));
        assert!(!ev[0].starts_between(at(21, 18) - 1, at(21, 18)));
        // Weekly Fri / Sat at 02:00: the 23rd is a Friday, the 25th a Sunday.
        assert_eq!(weekday(days_from_civil(2026, 10, 23)), 5);
        assert!(ev[1].starts_between(at(24, 2) - 5, at(24, 2)), "Saturday");
        assert!(!ev[1].starts_between(at(25, 2) - 5, at(25, 2)), "not Sunday");
        // 12:00 UTC at UTC+1 = 13:00 local.
        assert!(ev[2].starts_between(at(24, 13) - 1, at(24, 13)));
    }
}

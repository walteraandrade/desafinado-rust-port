const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

pub fn format_ago(at: i64, now: i64) -> String {
    let elapsed = now - at;
    if elapsed < MINUTE {
        "just now".to_string()
    } else if elapsed < HOUR {
        format!("{} min ago", elapsed / MINUTE)
    } else if elapsed < DAY {
        format!("{} h ago", elapsed / HOUR)
    } else {
        let days = elapsed / DAY;
        format!("{days} {}", if days == 1 { "day" } else { "days" }) + " ago"
    }
}

struct Civil {
    year: i64,
    month: usize,
    day: i64,
    hour: i64,
    minute: i64,
}

fn civil(at: i64, utc_offset_minutes: i64) -> Civil {
    let local = at + utc_offset_minutes * MINUTE;
    let days = local.div_euclid(DAY);
    let in_day = local.rem_euclid(DAY);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    Civil {
        year,
        month: (month - 1) as usize,
        day,
        hour: in_day / HOUR,
        minute: (in_day % HOUR) / MINUTE,
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn format_clock(at: i64, now: i64, utc_offset_minutes: i64) -> String {
    let date = civil(at, utc_offset_minutes);
    let today = civil(now, utc_offset_minutes);
    let time = format!("{:02}:{:02}", date.hour, date.minute);
    if (date.year, date.month, date.day) == (today.year, today.month, today.day) {
        time
    } else {
        format!("{} {}, {time}", MONTHS[date.month], date.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 10_000 * MINUTE;

    #[test]
    fn says_just_now_under_a_minute() {
        assert_eq!(format_ago(NOW - 59_000, NOW), "just now");
    }

    #[test]
    fn counts_minutes_then_hours_then_days() {
        assert_eq!(format_ago(NOW - 3 * MINUTE, NOW), "3 min ago");
        assert_eq!(format_ago(NOW - 125 * MINUTE, NOW), "2 h ago");
        assert_eq!(format_ago(NOW - 24 * 60 * MINUTE, NOW), "1 day ago");
        assert_eq!(format_ago(NOW - 3 * 24 * 60 * MINUTE, NOW), "3 days ago");
    }

    #[test]
    fn treats_a_time_in_the_future_as_just_now() {
        assert_eq!(format_ago(NOW + MINUTE, NOW), "just now");
    }

    fn local(year: i64, month: i64, day: i64, hour: i64, minute: i64, offset: i64) -> i64 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (month + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days * DAY + hour * HOUR + minute * MINUTE - offset * MINUTE
    }

    #[test]
    fn shows_only_the_time_for_a_moment_of_the_same_day() {
        let offset = -180;
        let today = local(2026, 10, 30, 15, 0, offset);
        let at = local(2026, 10, 30, 8, 54, offset);
        assert_eq!(format_clock(at, today, offset), "08:54");
    }

    #[test]
    fn adds_the_date_for_an_earlier_day() {
        let offset = -180;
        let today = local(2026, 10, 30, 15, 0, offset);
        let at = local(2026, 10, 29, 8, 54, offset);
        let shown = format_clock(at, today, offset);
        assert_eq!(shown, "Oct 29, 08:54");
        assert!(shown.len() > "08:54".len());
    }
}

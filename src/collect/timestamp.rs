//! Reads and writes the UTC timestamps of transcripts as milliseconds since the Unix epoch.
//! Transcripts only ever hold UTC times such as `2026-11-16T00:58:56.467Z`, so no time zone library is needed.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since 1970-01-01T00:00:00Z.
pub type Millis = i64;

/// Parses `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+00:00)`. Returns `None` for any other shape.
pub fn parse(text: &str) -> Option<Millis> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> { text.get(range)?.parse().ok() };
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    let mut rest = &text[19..];
    let mut millis = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let first_three = format!("{:0<3}", &fraction[..digits.min(3)]);
        millis = first_three.parse::<i64>().ok()?;
        rest = &fraction[digits..];
    }
    if rest != "Z" && rest != "+00:00" {
        return None;
    }
    let is_valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 61;
    if !is_valid {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// Writes `2026-11-16T00:58:56.467Z`.
pub fn format(millis: Millis) -> String {
    let days = millis.div_euclid(86_400_000);
    let in_day = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour = in_day / 3_600_000;
    let minute = in_day / 60_000 % 60;
    let second = in_day / 1000 % 60;
    let milli = in_day % 1000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
}

pub fn now() -> Millis {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after 1970");
    i64::try_from(since_epoch.as_millis()).expect("milliseconds since 1970 fit in 64 bits")
}

// Howard Hinnant's civil calendar algorithms: exact for every proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_timestamps_round_trip() {
        let text = "2026-11-16T00:58:56.467Z";
        let millis = parse(text).expect("a transcript timestamp parses");
        assert_eq!(format(millis), text);
        assert_eq!(parse("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(parse("2000-03-01T00:00:00.5+00:00"), Some(951_868_800_500));
    }

    #[test]
    fn other_shapes_are_refused() {
        for text in [
            "",
            "2026-11-16 00:58:56Z",
            "2026-11-16T00:58:56+02:00",
            "2026-13-01T00:00:00Z",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
}

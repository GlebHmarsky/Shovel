//! User-facing timestamps follow Windows regional settings. Commit details
//! retain Git's original wall time and offset; list rows show the local instant.
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use eframe::egui::{
    Context,
    cache::{ComputerMut, FrameCache},
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::SYSTEMTIME,
    Globalization::{
        DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, LOCALE_NOUSEROVERRIDE, TIME_NOSECONDS,
    },
};

#[derive(Clone, Copy, Hash)]
enum Stamp {
    Local(DateTime<Utc>),
    Original(NaiveDateTime, FixedOffset),
}

#[derive(Default)]
struct Formatter;
impl ComputerMut<Stamp, String> for Formatter {
    fn compute(&mut self, stamp: Stamp) -> String {
        match stamp {
            Stamp::Local(utc) => display(utc.with_timezone(&Local).naive_local(), false, None),
            Stamp::Original(wall, offset) => crate::strings::format(
                "RustDateWithOffset",
                &[&display(wall, true, None), &offset.to_string()],
            ),
        }
    }
}

pub fn invalidate(ctx: &Context) {
    ctx.memory_mut(|m| *m.caches.cache::<FrameCache<String, Formatter>>() = Default::default());
}

fn cached(ctx: &Context, stamp: Stamp) -> String {
    ctx.memory_mut(|m| {
        m.caches
            .cache::<FrameCache<String, Formatter>>()
            .get(stamp)
            .clone()
    })
}

pub fn local<T: TimeZone>(ctx: &Context, timestamp: &DateTime<T>) -> String {
    cached(ctx, Stamp::Local(timestamp.to_utc()))
}

pub fn original(ctx: &Context, timestamp: &DateTime<FixedOffset>) -> String {
    cached(
        ctx,
        Stamp::Original(timestamp.naive_local(), *timestamp.offset()),
    )
}

fn display(wall: NaiveDateTime, seconds: bool, locale: Option<&str>) -> String {
    let formatted = || -> Option<String> {
        let time = SYSTEMTIME {
            wYear: wall.year().try_into().ok()?,
            wMonth: wall.month() as u16,
            wDayOfWeek: wall.weekday().num_days_from_sunday() as u16,
            wDay: wall.day() as u16,
            wHour: wall.hour() as u16,
            wMinute: wall.minute() as u16,
            wSecond: wall.second() as u16,
            wMilliseconds: 0,
        };
        let locale_name = locale.map(|name| {
            name.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        });
        let overrides = if locale_name.is_some() {
            LOCALE_NOUSEROVERRIDE
        } else {
            0
        };
        let locale = locale_name.as_ref().map_or(null(), |name| name.as_ptr());
        // A null locale uses the user's current regional settings, including overrides.
        let date = native_part(|buf, cap| unsafe {
            GetDateFormatEx(
                locale,
                DATE_SHORTDATE | overrides,
                &time,
                null(),
                buf,
                cap,
                null(),
            )
        })?;
        let clock = native_part(|buf, cap| unsafe {
            GetTimeFormatEx(
                locale,
                overrides | if seconds { 0 } else { TIME_NOSECONDS },
                &time,
                null(),
                buf,
                cap,
            )
        })?;
        Some(crate::strings::format("RustDateAndTime", &[&date, &clock]))
    };
    formatted().unwrap_or_else(|| {
        wall.format(if seconds {
            "%Y-%m-%d %H:%M:%S"
        } else {
            "%Y-%m-%d %H:%M"
        })
        .to_string()
    })
}

fn native_part(write: impl Fn(*mut u16, i32) -> i32) -> Option<String> {
    let required = write(null_mut(), 0);
    if required <= 1 {
        return None;
    }
    let mut buffer = vec![0u16; required as usize];
    let written = write(buffer.as_mut_ptr(), required);
    if !(2..=required).contains(&written) {
        return None;
    }
    String::from_utf16(&buffer[..written as usize - 1]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_formats_dates_and_times_for_the_requested_culture() {
        let wall = DateTime::parse_from_rfc3339("2026-11-18T15:04:27+02:00")
            .unwrap()
            .naive_local();
        let us = display(wall, false, Some("en-US"));
        let uk = display(wall, false, Some("en-GB"));
        assert!(us.contains("11/18/2026") && us.contains("3:04"), "{us}");
        assert!(uk.contains("18/11/2026") && uk.contains("15:04"), "{uk}");
        assert!(display(wall, true, Some("en-GB")).contains("15:04:27"));
        let early = chrono::NaiveDate::from_ymd_opt(-1, 12, 1)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap();
        assert_eq!(display(early, true, Some("en-GB")), "-0001-12-01 01:02:03");
    }

    #[test]
    fn one_instant_shares_local_time_but_retains_each_original_calendar_date() {
        let ctx = Context::default();
        let east = DateTime::parse_from_rfc3339("2026-11-19T02:03:04+14:00").unwrap();
        let west = DateTime::parse_from_rfc3339("2026-11-18T02:03:04-10:00").unwrap();
        assert_eq!(east, west);
        assert_eq!(local(&ctx, &east), local(&ctx, &west));
        for timestamp in [east, west] {
            let observed = original(&ctx, &timestamp);
            assert!(observed.contains(&display(timestamp.naive_local(), true, None)));
            assert!(observed.contains(&timestamp.offset().to_string()));
        }
        assert_ne!(original(&ctx, &east), original(&ctx, &west));
        use eframe::egui::cache::CacheTrait;
        let cached_count =
            || ctx.memory_mut(|m| m.caches.cache::<FrameCache<String, Formatter>>().len());
        assert_eq!(cached_count(), 3);
        invalidate(&ctx);
        assert_eq!(cached_count(), 0);
        assert_eq!(local(&ctx, &east), local(&ctx, &west));
        assert_eq!(cached_count(), 1);
    }
}

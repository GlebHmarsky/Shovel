//! Display numbers using Windows regional grouping and separators without converting Git's integer counts to floating point.
use std::sync::{LazyLock, RwLock};
use windows_sys::Win32::Globalization::{
    GetLocaleInfoEx, GetNumberFormatEx, LOCALE_ILZERO, LOCALE_INEGNUMBER, LOCALE_NOUSEROVERRIDE,
    LOCALE_SDECIMAL, LOCALE_SGROUPING, LOCALE_STHOUSAND, NUMBERFMTW,
};

struct Rules {
    locale: Option<Vec<u16>>,
    decimal: Vec<u16>,
    thousands: Vec<u16>,
    grouping: u32,
    leading_zero: u32,
    negative_order: u32,
}

impl Rules {
    fn load(name: Option<&str>) -> Option<Self> {
        let locale = name.map(|name| name.encode_utf16().chain(Some(0)).collect::<Vec<_>>());
        let ptr = locale.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
        let flags = if name.is_some() {
            LOCALE_NOUSEROVERRIDE
        } else {
            0
        };
        let property = |kind| {
            let mut data = [0u16; 64];
            let count =
                unsafe { GetLocaleInfoEx(ptr, flags | kind, data.as_mut_ptr(), data.len() as i32) };
            (count > 0).then(|| data[..count as usize].to_vec())
        };
        let string = |kind| {
            let data = property(kind)?;
            String::from_utf16(&data[..data.len() - 1]).ok()
        };
        let grouping = string(LOCALE_SGROUPING)?;
        // Locale strings end in ";0" for a repeating group. NUMBERFMT uses
        // the opposite convention: 3 repeats threes, 32 repeats twos after the first three.
        let mut groups: String = grouping.replace(';', "");
        if groups.ends_with('0') {
            groups.pop();
        } else {
            groups.push('0');
        }
        let grouping = if groups.is_empty() {
            0
        } else {
            groups.parse().ok()?
        };
        Some(Self {
            decimal: property(LOCALE_SDECIMAL)?,
            thousands: property(LOCALE_STHOUSAND)?,
            leading_zero: string(LOCALE_ILZERO)?.parse().ok()?,
            negative_order: string(LOCALE_INEGNUMBER)?.parse().ok()?,
            grouping,
            locale,
        })
    }

    fn format(&self, value: &str, spec: &str) -> Option<String> {
        let (digits, grouped) = match spec {
            "N0" => (0, true),
            "N1" => (1, true),
            "F1" | "0.0" => (1, false),
            _ => return None,
        };
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'-' | b'.'))
        {
            return None;
        }
        let input: Vec<_> = value.encode_utf16().chain(Some(0)).collect();
        let format = NUMBERFMTW {
            NumDigits: digits,
            LeadingZero: self.leading_zero,
            Grouping: if grouped { self.grouping } else { 0 },
            lpDecimalSep: self.decimal.as_ptr().cast_mut(),
            lpThousandSep: self.thousands.as_ptr().cast_mut(),
            NegativeOrder: self.negative_order,
        };
        let mut out = [0u16; 512];
        let count = unsafe {
            GetNumberFormatEx(
                self.locale
                    .as_ref()
                    .map_or(std::ptr::null(), |s| s.as_ptr()),
                0,
                input.as_ptr(),
                &format,
                out.as_mut_ptr(),
                out.len() as i32,
            )
        };
        (count > 0).then(|| String::from_utf16_lossy(&out[..count as usize - 1]))
    }
}

struct RegionalRules(RwLock<Option<Rules>>);

impl RegionalRules {
    fn refresh(&self, locale: Option<&str>) {
        let current = Rules::load(locale);
        *self.0.write().unwrap() = current;
    }

    fn format(&self, value: &str, spec: &str) -> Option<String> {
        self.0.read().unwrap().as_ref()?.format(value, spec)
    }
}

static REGIONAL: LazyLock<RegionalRules> =
    LazyLock::new(|| RegionalRules(RwLock::new(Rules::load(None))));

pub(super) fn refresh() {
    REGIONAL.refresh(None);
}

pub(super) fn format(value: &str, spec: &str) -> Option<String> {
    REGIONAL.format(value, spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_open_session_reloads_number_rules_for_ui_and_worker_readers() {
        let regional = RegionalRules(RwLock::new(Rules::load(Some("en-US"))));
        assert_eq!(
            regional.format("12345.6", "N1").as_deref(),
            Some("12,345.6")
        );
        regional.refresh(Some("de-DE"));
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| regional.format("12345.6", "N1").unwrap());
            assert_eq!(
                regional.format("12345.6", "N1").as_deref(),
                Some("12.345,6")
            );
            assert_eq!(worker.join().unwrap(), "12.345,6");
        });
        regional.refresh(Some("hi-IN"));
        assert_eq!(
            regional.format("12345678", "N0").as_deref(),
            Some("1,23,45,678")
        );
    }

    #[test]
    fn windows_formats_exact_byte_counts_decimals_and_indian_groups() {
        let us = Rules::load(Some("en-US")).unwrap();
        let de = Rules::load(Some("de-DE")).unwrap();
        let india = Rules::load(Some("hi-IN")).unwrap();
        assert_eq!(
            us.format("18446744073709551615", "N0").as_deref(),
            Some("18,446,744,073,709,551,615")
        );
        assert_eq!(de.format("-1234.56", "N1").as_deref(), Some("-1.234,6"));
        assert_eq!(de.format("1234.56", "F1").as_deref(), Some("1234,6"));
        assert_eq!(
            india.format("123456789", "N0").as_deref(),
            Some("12,34,56,789")
        );
        assert!(us.format("unknown", "N0").is_none());
        assert!(us.format("12\0file", "N0").is_none());
    }
}

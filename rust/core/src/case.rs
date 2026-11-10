//! Windows invariant casing for cross-product native lock names and path keys.
//! Rust's full Unicode uppercase expands e.g. sharp-s into SS. The pinned
//! C# baseline uses simple OS casing, which must not merge those distinct paths.
use std::{io, path::Path, sync::OnceLock};
use windows_sys::Win32::{
    Globalization::{LCMAP_UPPERCASE, LCMapStringEx},
    System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
};

type Upper = unsafe extern "C" fn(i32) -> i32;
fn icu_upper() -> Option<Upper> {
    static UPPER: OnceLock<Option<Upper>> = OnceLock::new();
    *UPPER.get_or_init(|| unsafe {
        // Keep this system module loaded while its cached function pointer lives.
        // Windows before 1903 lacks combined ICU; .NET also falls back to NLS.
        let name: Vec<u16> = "icu.dll".encode_utf16().chain(Some(0)).collect();
        let module = LoadLibraryExW(
            name.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if module.is_null() {
            return None;
        }
        GetProcAddress(module, c"u_toupper".as_ptr().cast())
            .map(|f| std::mem::transmute::<unsafe extern "system" fn() -> isize, Upper>(f))
    })
}

pub fn invariant_upper(value: &str) -> io::Result<String> {
    if value.is_ascii() {
        return Ok(value.to_ascii_uppercase());
    }
    if let Some(upper) = icu_upper() {
        return value
            .chars()
            .map(|ch| {
                // Matches .NET 10 ChangeCaseInvariant: dotless i stays dotless.
                let mapped = if ch == '\u{131}' {
                    ch as u32
                } else {
                    (unsafe { upper(ch as i32) }) as u32
                };
                char::from_u32(mapped).ok_or_else(|| io::Error::other("invalid invariant mapping"))
            })
            .collect();
    }
    nls_upper(value)
}

fn nls_upper(value: &str) -> io::Result<String> {
    let source: Vec<u16> = value.encode_utf16().collect();
    let count = i32::try_from(source.len()).map_err(|_| io::Error::other("case input limit"))?;
    let mut destination = vec![0u16; source.len()];
    let written = unsafe {
        LCMapStringEx(
            [0u16].as_ptr(),
            LCMAP_UPPERCASE,
            source.as_ptr(),
            count,
            destination.as_mut_ptr(),
            count,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    String::from_utf16(&destination[..written as usize])
        .map_err(|_| io::Error::other("invalid invariant mapping"))
}

/// Session/cache grouping only. Native mutation gates use the fallible mapper
/// directly and refuse execution on mapping failure. Retaining original text
/// here can separate aliases on failure, but never merges distinct paths.
pub fn path_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('/', "\\");
    invariant_upper(&normalized).unwrap_or(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nls_fallback_keeps_simple_casing_and_surrogate_pairs() {
        assert_eq!(nls_upper("straßeᾀı𐐨").unwrap(), "STRAßEᾈı𐐀");
    }
    #[test]
    fn simple_casing_keeps_distinct_windows_paths_separate() {
        for (input, expected) in [
            ("straße", "STRAßE"),
            ("ﬃ", "ﬃ"),
            ("ᾀ", "ᾈ"),
            ("ı", "ı"),
            ("ſ", "S"),
            ("𐐨", "𐐀"),
            ("שלום", "שלום"),
        ] {
            assert_eq!(invariant_upper(input).unwrap(), expected);
        }
        assert_ne!(
            path_key(Path::new(r"C:\straße")),
            path_key(Path::new(r"C:\strasse"))
        );
        assert_eq!(
            path_key(Path::new("C:/Mixed")),
            path_key(Path::new(r"c:\mixed"))
        );
    }
}

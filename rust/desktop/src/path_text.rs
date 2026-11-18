//! Display-only ordering for paths in egui's left-to-right text painter.
//! Git, the clipboard and accessibility keep their logical character order.
use eframe::egui::{
    self,
    cache::{ComputerMut, FrameCache},
};
use std::{fmt::Write, sync::Arc};
use windows_sys::Win32::Globalization::{
    U_ARABIC_NUMBER, U_RIGHT_TO_LEFT, U_RIGHT_TO_LEFT_ARABIC, U_SHAPE_LETTERS_SHAPE,
    UBIDI_DO_MIRRORING, UBIDI_KEEP_BASE_COMBINING, u_charDirection, u_shapeArabic, ubidi_close,
    ubidi_open, ubidi_setPara, ubidi_writeReordered,
};

pub(super) struct PathText<'a> {
    original: &'a str,
    rendered: Option<Arc<Rendered>>,
}
impl<'a> PathText<'a> {
    pub fn new(ctx: &egui::Context, original: &'a str) -> Self {
        let rendered = if original
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_control())
        {
            None
        } else {
            ctx.memory_mut(|m| {
                m.caches
                    .cache::<FrameCache<Option<Arc<Rendered>>, Formatter>>()
                    .get(original)
                    .clone()
            })
        };
        Self { original, rendered }
    }
    pub fn visual(&self) -> &str {
        self.rendered.as_ref().map_or(self.original, |r| &r.visual)
    }
    pub fn spoken(&self) -> &str {
        self.rendered.as_ref().map_or(self.original, |r| &r.spoken)
    }
    pub fn reordered(&self) -> bool {
        self.visual() != self.spoken()
    }
}

pub(super) fn label(ui: &mut egui::Ui, visual: String, logical: Option<String>) {
    if let Some(logical) = logical {
        let response = ui.add(egui::Label::new(visual).selectable(false));
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_value(logical));
    } else {
        ui.label(visual);
    }
}
struct Rendered {
    visual: String,
    spoken: String,
}
#[derive(Default)]
struct Formatter;
impl ComputerMut<&str, Option<Arc<Rendered>>> for Formatter {
    fn compute(&mut self, path: &str) -> Option<Arc<Rendered>> {
        let escaped = |c: char| {
            c.is_control()
                || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        };
        let mut logical = String::with_capacity(path.len());
        let mut rtl = false;
        for c in path.chars() {
            if escaped(c) {
                write!(logical, "<U+{:04X}>", c as u32).unwrap();
            } else {
                logical.push(c);
                rtl |= !c.is_ascii()
                    && matches!(
                        unsafe { u_charDirection(c as i32) },
                        U_RIGHT_TO_LEFT | U_RIGHT_TO_LEFT_ARABIC | U_ARABIC_NUMBER
                    );
            }
        }
        if !rtl {
            return (logical != path).then(|| {
                Arc::new(Rendered {
                    spoken: logical.clone(),
                    visual: logical,
                })
            });
        }
        let mut visual = String::with_capacity(logical.len());
        for part in logical.split_inclusive(['/', '\\']) {
            let body = part.trim_end_matches(['/', '\\']);
            if body.is_ascii() {
                visual.push_str(body);
            } else {
                visual.push_str(&visual_component(body).unwrap_or_else(|| body.to_owned()));
            }
            visual.push_str(&part[body.len()..]);
        }
        Some(Arc::new(Rendered {
            visual,
            spoken: format!("\u{2066}{logical}\u{2069}"),
        }))
    }
}

fn visual_component(component: &str) -> Option<String> {
    let mut source: Vec<_> = component.encode_utf16().collect();
    let mut result = vec![0u16; source.len().checked_mul(2)?.checked_add(1)?];
    let capacity = i32::try_from(result.len()).ok()?;
    let mut error = 0;
    if component
        .chars()
        .any(|c| unsafe { u_charDirection(c as i32) } == U_RIGHT_TO_LEFT_ARABIC)
    {
        let length = unsafe {
            u_shapeArabic(
                source.as_ptr(),
                source.len() as i32,
                result.as_mut_ptr(),
                capacity,
                U_SHAPE_LETTERS_SHAPE,
                &mut error,
            )
        };
        if error > 0 {
            return None;
        }
        source.clear();
        source.extend_from_slice(&result[..length as usize]);
    }
    // ICU borrows the UTF-16 paragraph until close; keep its allocation alive
    // through both calls. A zero paragraph level keeps path components LTR-based.
    let bidi = unsafe { ubidi_open() };
    if bidi.is_null() {
        return None;
    }
    let length = unsafe {
        ubidi_setPara(
            bidi,
            source.as_ptr(),
            source.len() as i32,
            0,
            std::ptr::null_mut(),
            &mut error,
        );
        let length = ubidi_writeReordered(
            bidi,
            result.as_mut_ptr(),
            capacity,
            (UBIDI_KEEP_BASE_COMBINING | UBIDI_DO_MIRRORING) as u16,
            &mut error,
        );
        ubidi_close(bidi);
        length
    };
    (error <= 0).then(|| String::from_utf16_lossy(&result[..length as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_components_use_visual_order_and_preserve_logical_text() {
        let ctx = egui::Context::default();
        let path = PathText::new(&ctx, "src/אב/שלום.txt");
        assert_eq!(path.visual(), "src/בא/םולש.txt");
        assert_eq!(path.spoken(), "\u{2066}src/אב/שלום.txt\u{2069}");
        let arabic = PathText::new(&ctx, "src/سلام.txt");
        assert!(arabic.visual().starts_with("src/") && arabic.visual().ends_with(".txt"));
        assert!(
            arabic
                .visual()
                .chars()
                .any(|c| ('\u{fe70}'..='\u{fefc}').contains(&c))
        );
        assert_eq!(arabic.spoken(), "\u{2066}src/سلام.txt\u{2069}");
        let controls = PathText::new(&ctx, "file\u{202e}.txt");
        assert_eq!(controls.visual(), "file<U+202E>.txt");
        assert_eq!(controls.spoken(), controls.visual());
        assert_eq!(
            PathText::new(&ctx, "src/日本語.rs").visual(),
            "src/日本語.rs"
        );
    }
}

//! Centralni dispatch pro text edit. Vsechny text input/edit pole pak jen
//! delegate na tyhle dve fce + ad-hoc context (history navigation, autocomplete,
//! commit/cancel) si resi vlastni handler v sandwich pattern.
//!
//! `dispatch_text_key` zna vsechny standard shortcuts:
//! - Backspace/Delete/ArrowLeft/Right/Home/End (Shift = extend selection)
//! - Space (insert " ")
//! - Ctrl+A select all, Ctrl+C copy, Ctrl+X cut, Ctrl+V paste
//! - Char insert
//!
//! Vraci `TextKeyOutcome` aby caller poznal, jestli klavesu konzumoval +
//! pripadne specialni eventy (commit/cancel pres Enter/Tab/Escape).
//!
//! `dispatch_text_click` mapuje screen-x na byte cursor pos pres
//! dt_byte_idx_at_x (CamingoMono advance metric) a vola buffer.set_cursor.

use winit::keyboard::{Key, NamedKey};
use crate::devtools::model::text_buffer::TextBuffer;
use crate::browser::devtools_panel::dt_byte_idx_at_x;

pub enum TextKeyOutcome {
    /// Klavesa konzumovana - text se mohl zmenit, redraw.
    Handled,
    /// Klavesa nezna pro text input (caller necha probublovat dal).
    Unhandled,
    /// Enter (single line) - submit.
    Submit,
    /// Shift+Enter - newline insert.
    Newline,
    /// Tab - autocomplete trigger nebo focus next.
    Tab,
    /// Escape - cancel/blur.
    Cancel,
}

pub fn dispatch_text_key<B: TextBuffer + ?Sized>(
    buffer: &mut B,
    key: &Key,
    ctrl: bool,
    shift: bool,
) -> TextKeyOutcome {
    match key {
        Key::Named(NamedKey::Backspace) => { buffer.backspace(); TextKeyOutcome::Handled }
        Key::Named(NamedKey::Delete) => { buffer.delete_forward(); TextKeyOutcome::Handled }
        Key::Named(NamedKey::ArrowLeft) => { buffer.move_left(shift); TextKeyOutcome::Handled }
        Key::Named(NamedKey::ArrowRight) => { buffer.move_right(shift); TextKeyOutcome::Handled }
        Key::Named(NamedKey::Home) => { buffer.move_home(shift); TextKeyOutcome::Handled }
        Key::Named(NamedKey::End) => { buffer.move_end(shift); TextKeyOutcome::Handled }
        Key::Named(NamedKey::Space) => { buffer.insert(" "); TextKeyOutcome::Handled }
        Key::Named(NamedKey::Enter) if shift => TextKeyOutcome::Newline,
        Key::Named(NamedKey::Enter) => TextKeyOutcome::Submit,
        Key::Named(NamedKey::Tab) => TextKeyOutcome::Tab,
        Key::Named(NamedKey::Escape) => TextKeyOutcome::Cancel,
        Key::Character(s) if ctrl => {
            match s.as_str() {
                "a" | "A" => { buffer.select_all(); TextKeyOutcome::Handled }
                "c" | "C" => {
                    if let Some(t) = buffer.selected_text() {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(t);
                        }
                    }
                    TextKeyOutcome::Handled
                }
                "x" | "X" => {
                    if let Some(t) = buffer.cut() {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(t);
                        }
                    }
                    TextKeyOutcome::Handled
                }
                "v" | "V" => {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        if let Ok(t) = cb.get_text() {
                            buffer.insert(&t);
                        }
                    }
                    TextKeyOutcome::Handled
                }
                _ => TextKeyOutcome::Unhandled,
            }
        }
        Key::Character(s) => { buffer.insert(s); TextKeyOutcome::Handled }
        _ => TextKeyOutcome::Unhandled,
    }
}

/// Klik na text pole - prevod x souradnice (relativni k zacatku textu) na
/// byte cursor pos + set_cursor + clear_selection.
pub fn dispatch_text_click<B: TextBuffer + ?Sized>(buffer: &mut B, rel_x: f32) {
    let idx = dt_byte_idx_at_x(buffer.text(), rel_x);
    buffer.set_cursor(idx);
    buffer.set_anchor(None);
}

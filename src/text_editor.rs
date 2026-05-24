/// A simple embedded text editor that processes raw keyboard events
/// to avoid depending on egui's TextEdit widget (which has platform issues).

use eframe::egui::{self, Color32, FontId, Key, Rounding, Vec2};

/// State for the embedded text editor.
pub struct TextEditor {
    pub open: bool,
    pub text: String,
    cursor: usize,
}

impl Default for TextEditor {
    fn default() -> Self {
        Self {
            open: false,
            text: String::new(),
            cursor: 0,
        }
    }
}

impl TextEditor {
    pub fn open_with(&mut self, initial: &str) {
        self.text = initial.to_string();
        self.cursor = self.text.len();
        self.open = true;
    }

    /// Show the editor window. Returns `Some(text)` on Done,
    /// `Some("")` on Cancel, `None` while still open.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.open {
            return None;
        }

        let mut result = None;

        let window_id = egui::Id::new("embed_text_editor");
        let inner = egui::Window::new("Text Editor")
            .id(window_id)
            .collapsible(false)
            .resizable(true)
            .default_size([500.0, 350.0])
            .min_width(300.0)
            .min_height(200.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                // Group frame for the text display
                let text_color = Color32::WHITE;
                let font = FontId::monospace(16.0);
                let line_height = 20.0;

                // Calculate text area height from available space
                let button_area = 60.0;
                let hint_area = 20.0;
                let text_area_height = (ui.available_height() - button_area - hint_area).max(100.0);

                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(8.0, 8.0))
                    .show(ui, |ui| {
                        // Allocate a response widget to capture keyboard focus
                        let (resp, mut painter) = ui.allocate_painter(
                            Vec2::new(ui.available_width(), text_area_height),
                            egui::Sense::click(),
                        );

                        // Grab focus on click
                        if resp.clicked() {
                            resp.request_focus();
                        }

                        // If we have focus, process keyboard events
                        let has_focus = resp.has_focus();
                        if has_focus {
                            self.process_key_events(ctx);
                            ctx.memory_mut(|m| { m.request_focus(resp.id); });
                        }

                        // Clip drawing to the text area
                        painter.set_clip_rect(resp.rect);

                        // Layout text with wrapping to stay within the textbox
                        let wrap_width = (resp.rect.width() - 8.0).max(10.0);
                        let galley = ui.fonts(|f| {
                            f.layout(
                                self.text.clone(),
                                font.clone(),
                                text_color,
                                wrap_width,
                            )
                        });

                        // Draw the text
                        let text_origin = resp.rect.min;
                        let galley_clone = galley.clone();
                        painter.galley(text_origin, galley_clone, text_color);

                        // Find cursor position from galley glyph positions
                        if has_focus {
                            if self.cursor < self.text.len() {
                                // Find the glyph whose character index matches cursor
                                let text_before = &self.text[..self.cursor];
                                let galley_before = ui.fonts(|f| {
                                    f.layout(
                                        text_before.to_string(),
                                        font.clone(),
                                        text_color,
                                        wrap_width,
                                    )
                                });
                                let last_row = galley_before.rows.last();
                                if let Some(row) = last_row {
                                    let cx = text_origin.x + row.rect.size().x.max(0.0);
                                    let cy = text_origin.y
                                        + (galley_before.rows.len().saturating_sub(1)) as f32
                                            * line_height;
                                    painter.rect_filled(
                                        egui::Rect::from_min_size(
                                            egui::Pos2::new(cx, cy),
                                            Vec2::new(2.0, line_height),
                                        ),
                                        Rounding::ZERO,
                                        Color32::WHITE,
                                    );
                                }
                            } else {
                                // Cursor at end of text
                                let galley_before = ui.fonts(|f| {
                                    f.layout(
                                        self.text.clone(),
                                        font.clone(),
                                        text_color,
                                        wrap_width,
                                    )
                                });
                                if let Some(last_row) = galley_before.rows.last() {
                                    let cx = text_origin.x + last_row.rect.size().x.max(0.0);
                                    let cy = text_origin.y
                                        + (galley_before.rows.len().saturating_sub(1)) as f32
                                            * line_height;
                                    painter.rect_filled(
                                        egui::Rect::from_min_size(
                                            egui::Pos2::new(cx, cy),
                                            Vec2::new(2.0, line_height),
                                        ),
                                        Rounding::ZERO,
                                        Color32::WHITE,
                                    );
                                }
                            }
                        }

                        // Keep focus
                        if ctx.memory(|m| m.has_focus(resp.id)) {
                            ctx.memory_mut(|m| { m.request_focus(resp.id); });
                        } else {
                            resp.request_focus();
                        }
                    });

                ui.add_space(8.0);

                // Buttons
                ui.horizontal(|ui| {
                    let can_done = !self.text.trim().is_empty();
                    if ui.add_enabled(can_done, egui::Button::new("Done")).clicked() {
                        result = Some(self.text.clone());
                        self.open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(String::new());
                        self.open = false;
                    }
                });

                ui.label(
                    egui::RichText::new("Ctrl+C/V/X/A — Arrow keys, Backspace, Enter, Tab work")
                        .size(11.0)
                        .color(Color32::GRAY),
                );
            });

        // If the window was closed by the user (X button), treat as cancel
        if self.open && inner.is_none() {
            self.open = false;
            result = Some(String::new());
        }

        result
    }

    fn process_key_events(&mut self, ctx: &egui::Context) {
        // Snapshot events in one pass
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());

        let _has_paste_event = events.iter().any(|e| matches!(e, egui::Event::Paste(_)));

        // Handle clipboard-specific events (Copy/Cut/Paste) generated by the platform
        for event in &events {
            match event {
                egui::Event::Copy => {
                    ctx.output_mut(|o| o.copied_text = self.text.clone());
                }
                egui::Event::Cut => {
                    ctx.output_mut(|o| o.copied_text = self.text.clone());
                    self.text.clear();
                    self.cursor = 0;
                }
                egui::Event::Paste(text) => {
                    self.insert_text(text.as_str());
                }
                _ => {}
            }
        }

        // Handle key events
        let has_text_events = events.iter().any(|e| matches!(e, egui::Event::Text(_)));
        for event in &events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let ctrl = modifiers.ctrl || modifiers.command;

                    if ctrl && *key == Key::A {
                        self.cursor = self.text.len();
                        continue;
                    }
                    if ctrl && *key == Key::C {
                        ctx.output_mut(|o| o.copied_text = self.text.clone());
                        continue;
                    }
                    if ctrl && *key == Key::X {
                        ctx.output_mut(|o| o.copied_text = self.text.clone());
                        self.text.clear();
                        self.cursor = 0;
                        continue;
                    }
                    if ctrl && *key == Key::V {
                        // Fallback paste via clipboard not available in egui 0.30 RawInput.
                        // Handled by Event::Paste above on most platforms.
                        continue;
                    }

                    if has_text_events {
                        self.handle_special_key(*key, modifiers);
                    } else {
                        self.handle_key(*key, modifiers);
                    }
                }
                egui::Event::Text(text) => {
                    if has_text_events {
                        self.insert_text(text.as_str());
                    }
                }
                // Already handled above
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => {}
                _ => {}
            }
        }
    }

    fn handle_key(&mut self, key: Key, modifiers: &egui::Modifiers) {
        match self.handle_special_key(key, modifiers) {
            Some(()) => {}
            None => {
                if let Some(ch) = key_to_char(key, modifiers) {
                    self.text.insert(self.cursor, ch);
                    self.cursor += ch.len_utf8();
                }
            }
        }
    }

    /// Handle navigation/control keys only. Returns `Some(())` if the key was
    /// handled, `None` if it should be treated as a character key.
    fn handle_special_key(&mut self, key: Key, _modifiers: &egui::Modifiers) -> Option<()> {
        match key {
            Key::Enter => {
                self.text.insert(self.cursor, '\n');
                self.cursor += 1;
                Some(())
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    let prev = self.text[..self.cursor]
                        .chars()
                        .next_back()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    self.text.drain(self.cursor - prev..self.cursor);
                    self.cursor -= prev;
                }
                Some(())
            }
            Key::Delete => {
                if self.cursor < self.text.len() {
                    let next = self.text[self.cursor..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    self.text.drain(self.cursor..self.cursor + next);
                }
                Some(())
            }
            Key::Home => {
                let before = &self.text[..self.cursor];
                if let Some(pos) = before.rfind('\n') {
                    self.cursor = pos + 1;
                } else {
                    self.cursor = 0;
                }
                Some(())
            }
            Key::End => {
                let after = &self.text[self.cursor..];
                if let Some(pos) = after.find('\n') {
                    self.cursor += pos;
                } else {
                    self.cursor = self.text.len();
                }
                Some(())
            }
            Key::ArrowLeft => {
                if self.cursor > 0 {
                    let prev = self.text[..self.cursor]
                        .chars()
                        .next_back()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    self.cursor -= prev;
                }
                Some(())
            }
            Key::ArrowRight => {
                if self.cursor < self.text.len() {
                    let next = self.text[self.cursor..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    self.cursor += next;
                }
                Some(())
            }
            Key::ArrowUp => {
                let before = &self.text[..self.cursor];
                if let Some(pos) = before.rfind('\n') {
                    let line_start = pos + 1;
                    let col = self.text[line_start..self.cursor].chars().count();
                    if pos > 0 {
                        let prev_line_end = pos;
                        let prev_line_start = self.text[..prev_line_end]
                            .rfind('\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let prev_line = &self.text[prev_line_start..prev_line_end];
                        let target_col = col.min(prev_line.chars().count().saturating_sub(1));
                        self.cursor = prev_line_start
                            + prev_line
                                .chars()
                                .take(target_col)
                                .map(|c| c.len_utf8())
                                .sum::<usize>();
                    } else {
                        self.cursor = 0;
                    }
                }
                Some(())
            }
            Key::ArrowDown => {
                let after = &self.text[self.cursor..];
                if let Some(pos) = after.find('\n') {
                    let current_line_start = self.text[..self.cursor]
                        .rfind('\n')
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let col = self.text[current_line_start..self.cursor]
                        .chars()
                        .count();
                    let next_line_start = self.cursor + pos + 1;
                    let remaining = &self.text[next_line_start..];
                    let next_line_end = remaining
                        .find('\n')
                        .map(|p| next_line_start + p)
                        .unwrap_or(self.text.len());
                    let next_line = &self.text[next_line_start..next_line_end];
                    let target_col = col.min(next_line.chars().count().saturating_sub(1));
                    self.cursor = next_line_start
                        + next_line
                            .chars()
                            .take(target_col)
                            .map(|c| c.len_utf8())
                            .sum::<usize>();
                } else {
                    self.cursor = self.text.len();
                }
                Some(())
            }
            Key::Tab => {
                self.text.insert_str(self.cursor, "    ");
                self.cursor += 4;
                Some(())
            }
            Key::Escape => Some(()),
            _ => None,
        }
    }

    fn insert_text(&mut self, text: &str) {
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            self.text.insert(self.cursor, ch);
            self.cursor += ch.len_utf8();
        }
    }
}

fn key_to_char(key: Key, modifiers: &egui::Modifiers) -> Option<char> {
    let shift = modifiers.shift;
    match key {
        Key::Space => Some(' '),
        Key::A => Some(if shift { 'A' } else { 'a' }),
        Key::B => Some(if shift { 'B' } else { 'b' }),
        Key::C => Some(if shift { 'C' } else { 'c' }),
        Key::D => Some(if shift { 'D' } else { 'd' }),
        Key::E => Some(if shift { 'E' } else { 'e' }),
        Key::F => Some(if shift { 'F' } else { 'f' }),
        Key::G => Some(if shift { 'G' } else { 'g' }),
        Key::H => Some(if shift { 'H' } else { 'h' }),
        Key::I => Some(if shift { 'I' } else { 'i' }),
        Key::J => Some(if shift { 'J' } else { 'j' }),
        Key::K => Some(if shift { 'K' } else { 'k' }),
        Key::L => Some(if shift { 'L' } else { 'l' }),
        Key::M => Some(if shift { 'M' } else { 'm' }),
        Key::N => Some(if shift { 'N' } else { 'n' }),
        Key::O => Some(if shift { 'O' } else { 'o' }),
        Key::P => Some(if shift { 'P' } else { 'p' }),
        Key::Q => Some(if shift { 'Q' } else { 'q' }),
        Key::R => Some(if shift { 'R' } else { 'r' }),
        Key::S => Some(if shift { 'S' } else { 's' }),
        Key::T => Some(if shift { 'T' } else { 't' }),
        Key::U => Some(if shift { 'U' } else { 'u' }),
        Key::V => Some(if shift { 'V' } else { 'v' }),
        Key::W => Some(if shift { 'W' } else { 'w' }),
        Key::X => Some(if shift { 'X' } else { 'x' }),
        Key::Y => Some(if shift { 'Y' } else { 'y' }),
        Key::Z => Some(if shift { 'Z' } else { 'z' }),
        Key::Num0 => Some(if shift { ')' } else { '0' }),
        Key::Num1 => Some(if shift { '!' } else { '1' }),
        Key::Num2 => Some(if shift { '@' } else { '2' }),
        Key::Num3 => Some(if shift { '#' } else { '3' }),
        Key::Num4 => Some(if shift { '$' } else { '4' }),
        Key::Num5 => Some(if shift { '%' } else { '5' }),
        Key::Num6 => Some(if shift { '^' } else { '6' }),
        Key::Num7 => Some(if shift { '&' } else { '7' }),
        Key::Num8 => Some(if shift { '*' } else { '8' }),
        Key::Num9 => Some(if shift { '(' } else { '9' }),
        Key::Minus => Some(if shift { '_' } else { '-' }),
        Key::Equals => Some(if shift { '+' } else { '=' }),
        Key::OpenBracket => Some(if shift { '{' } else { '[' }),
        Key::CloseBracket => Some(if shift { '}' } else { ']' }),
        Key::Backslash => Some(if shift { '|' } else { '\\' }),
        Key::Semicolon => Some(if shift { ':' } else { ';' }),
        Key::Quote => Some(if shift { '"' } else { '\'' }),
        Key::Comma => Some(if shift { '<' } else { ',' }),
        Key::Period => Some(if shift { '>' } else { '.' }),
        Key::Slash => Some(if shift { '?' } else { '/' }),
        Key::Backtick => Some(if shift { '~' } else { '`' }),
        _ => None,
    }
}

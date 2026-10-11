use crate::platform::{UI_FONT, command};
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

// A native, IME-aware text field. GPUI handles text entry; this view owns selection
// and wrapping. The reader's shortcuts never consume keys while this is focused.
pub struct Editor {
    pub text: String,
    pub focus: FocusHandle,
    selection: Range<usize>,
    anchor: usize,
    marked: Option<Range<usize>>,
    lines: Vec<(usize, Pixels, WrappedLine)>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    reveal_cursor: bool,
    multiline: bool,
    selecting: bool,
    undo: Vec<(String, Range<usize>, usize)>,
    redo: Vec<(String, Range<usize>, usize)>,
    ink: Hsla,
    paper: Hsla,
}

impl Editor {
    pub fn new(
        text: String,
        multiline: bool,
        ink: Hsla,
        paper: Hsla,
        cx: &mut Context<Self>,
    ) -> Self {
        let end = text.len();
        Self {
            text,
            focus: cx.focus_handle(),
            selection: end..end,
            anchor: end,
            marked: None,
            lines: Vec::new(),
            bounds: None,
            scroll: px(0.),
            reveal_cursor: true,
            multiline,
            selecting: false,
            undo: Vec::new(),
            redo: Vec::new(),
            ink,
            paper,
        }
    }

    fn utf8(&self, offset: usize) -> usize {
        utf8_offset(&self.text, offset)
    }

    fn utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }

    fn selected(&self) -> Range<usize> {
        self.utf16(self.selection.start)..self.utf16(self.selection.end)
    }

    fn cursor(&self) -> usize {
        if self.anchor == self.selection.end {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn move_cursor(&mut self, index: usize, extend: bool, cx: &mut Context<Self>) {
        if !extend {
            self.anchor = index;
        }
        self.selection = self.anchor.min(index)..self.anchor.max(index);
        self.marked = None;
        self.reveal_cursor = true;
        cx.notify();
    }

    fn position(&self, index: usize) -> Point<Pixels> {
        self.lines
            .iter()
            .rev()
            .find(|(start, _, _)| *start <= index)
            .and_then(|(start, y, line)| {
                line.position_for_index((index - start).min(line.len()), px(26.))
                    .map(|p| point(p.x, p.y + *y))
            })
            .unwrap_or(point(px(0.), px(0.)))
    }

    fn index_at(&self, p: Point<Pixels>) -> usize {
        let Some(bounds) = self.bounds else {
            return 0;
        };
        let p = point(p.x - bounds.left(), p.y - bounds.top() + self.scroll);
        for (start, y, line) in self.lines.iter().rev() {
            if p.y >= *y {
                let index = line
                    .closest_index_for_position(point(p.x, p.y - *y), px(26.))
                    .unwrap_or_else(|i| i);
                return (*start + index).min(self.text.len());
            }
        }
        0
    }

    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let command = command(k.modifiers);
        let cursor = self.cursor();
        let previous = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .rfind(|i| *i < cursor)
            .unwrap_or(0);
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > cursor)
            .unwrap_or(self.text.len());
        match (command, k.key.as_str()) {
            (true, "enter") => return,
            (true, "z") => {
                let state = if k.modifiers.shift {
                    self.redo.pop()
                } else {
                    self.undo.pop()
                };
                if let Some((text, selection, anchor)) = state {
                    let current = (
                        std::mem::replace(&mut self.text, text),
                        self.selection.clone(),
                        self.anchor,
                    );
                    if k.modifiers.shift {
                        self.undo.push(current);
                    } else {
                        self.redo.push(current);
                    }
                    self.selection = selection;
                    self.anchor = anchor;
                    self.marked = None;
                    self.reveal_cursor = true;
                    cx.notify();
                }
            }
            (true, "a") => {
                self.anchor = 0;
                self.selection = 0..self.text.len();
                cx.notify();
            }
            (true, "v") => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    self.replace_text_in_range(None, &text, window, cx);
                }
            }
            (true, "c" | "x") => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.text[self.selection.clone()].into(),
                ));
                if k.key == "x" {
                    self.replace_text_in_range(None, "", window, cx);
                }
            }
            (_, "left") => self.move_cursor(
                if command {
                    0
                } else if !k.modifiers.shift && !self.selection.is_empty() {
                    self.selection.start
                } else {
                    previous
                },
                k.modifiers.shift,
                cx,
            ),
            (_, "right") => self.move_cursor(
                if command {
                    self.text.len()
                } else if !k.modifiers.shift && !self.selection.is_empty() {
                    self.selection.end
                } else {
                    next
                },
                k.modifiers.shift,
                cx,
            ),
            (_, "home") => self.move_cursor(0, k.modifiers.shift, cx),
            (_, "end") => self.move_cursor(self.text.len(), k.modifiers.shift, cx),
            (_, "up" | "down") => {
                let p = self.position(cursor);
                if let Some(bounds) = self.bounds {
                    let y = if k.key == "up" {
                        p.y - px(26.)
                    } else {
                        p.y + px(26.)
                    };
                    let index =
                        self.index_at(point(bounds.left() + p.x, bounds.top() + y - self.scroll));
                    self.move_cursor(index, k.modifiers.shift, cx);
                }
            }
            (_, "backspace" | "delete") => {
                if self.selection.is_empty() {
                    self.selection = if k.key == "backspace" {
                        previous..cursor
                    } else {
                        cursor..next
                    };
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            (_, "enter") if self.multiline => self.replace_text_in_range(None, "\n", window, cx),
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }
}

fn utf8_offset(text: &str, offset: usize) -> usize {
    let mut count = 0;
    for (i, ch) in text.char_indices() {
        if count >= offset {
            return i;
        }
        count += ch.len_utf16();
    }
    text.len()
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.utf8(range.start)..self.utf8(range.end);
        *actual = Some(self.utf16(range.start)..self.utf16(range.end));
        Some(self.text[range].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.selected(),
            reversed: self.anchor == self.selection.end,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| self.utf16(r.start)..self.utf16(r.end))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.utf8(r.start)..self.utf8(r.end))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = if self.multiline {
            text
        } else {
            text.replace('\n', " ")
        };
        if self.text.len() - range.len() + text.len() > 2 * 1024 * 1024 {
            return;
        }
        if self.undo.len() == 50 {
            self.undo.remove(0);
        }
        self.undo
            .push((self.text.clone(), self.selection.clone(), self.anchor));
        self.redo.clear();
        self.text.replace_range(range.clone(), &text);
        self.anchor = range.start + text.len();
        self.selection = self.anchor..self.anchor;
        self.marked = None;
        self.reveal_cursor = true;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range
            .as_ref()
            .map(|r| self.utf8(r.start))
            .or(self.marked.as_ref().map(|r| r.start))
            .unwrap_or(self.selection.start);
        self.replace_text_in_range(range, text, window, cx);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = selected {
            self.selection = start + utf8_offset(text, r.start)..start + utf8_offset(text, r.end);
            self.anchor = self.selection.start;
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let p = self.position(self.utf8(range.start));
        Some(Bounds::new(
            point(bounds.left() + p.x, bounds.top() + p.y - self.scroll),
            size(px(2.), px(26.)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.utf16(self.index_at(point)))
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let focus = self.focus.clone();
        div()
            .id("editor")
            .w_full()
            .h(px(if self.multiline {
                (f32::from(window.viewport_size().height) - 220.).clamp(90., 260.)
            } else {
                42.
            }))
            .bg(self.paper)
            .border_1()
            .border_color(self.ink.opacity(0.2))
            .rounded_lg()
            .p_2()
            .overflow_hidden()
            .track_focus(&focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let total = this
                    .lines
                    .last()
                    .map(|(_, y, line)| *y + line.size(px(26.)).height)
                    .unwrap_or(px(0.));
                let height = this.bounds.map(|b| b.size.height).unwrap_or(px(0.));
                this.scroll = (this.scroll - event.delta.pixel_delta(px(26.)).y)
                    .clamp(px(0.), (total - height).max(px(0.)));
                this.reveal_cursor = false;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    this.selecting = true;
                    this.move_cursor(this.index_at(e.position), e.modifiers.shift, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.selecting {
                    this.move_cursor(this.index_at(e.position), true, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        entity.update(cx, |input, _| {
                            let content: SharedString = if input.text.is_empty() {
                                if input.multiline {
                                    "Write or paste text…"
                                } else {
                                    "Title (optional)"
                                }
                                .into()
                            } else {
                                input.text.clone().into()
                            };
                            let normal = TextRun {
                                len: content.len(),
                                font: font(UI_FONT),
                                color: if input.text.is_empty() {
                                    input.ink.opacity(0.4)
                                } else {
                                    input.ink
                                },
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let mut runs = Vec::new();
                            let highlighted =
                                input.marked.clone().unwrap_or(input.selection.clone());
                            if !highlighted.is_empty() {
                                runs.push(TextRun {
                                    len: highlighted.start,
                                    ..normal.clone()
                                });
                                runs.push(TextRun {
                                    len: highlighted.len(),
                                    background_color: Some(rgba(0xda684938).into()),
                                    ..normal.clone()
                                });
                                runs.push(TextRun {
                                    len: content.len() - highlighted.end,
                                    ..normal
                                });
                            } else {
                                runs.push(normal);
                            }
                            let lines = window
                                .text_system()
                                .shape_text(content, px(15.), &runs, Some(bounds.size.width), None)
                                .unwrap_or_default();
                            let mut y = px(0.);
                            let mut start = 0;
                            input.lines = lines
                                .into_iter()
                                .map(|line| {
                                    let item = (start, y, line.clone());
                                    y += line.size(px(26.)).height;
                                    start += line.len() + 1;
                                    item
                                })
                                .collect();
                            input.bounds = Some(bounds);
                            let cursor = input.position(input.cursor());
                            if input.reveal_cursor
                                && cursor.y - input.scroll + px(26.) > bounds.size.height
                            {
                                input.scroll = cursor.y + px(26.) - bounds.size.height;
                            }
                            if input.reveal_cursor && cursor.y < input.scroll {
                                input.scroll = cursor.y;
                            }
                            input.scroll = input
                                .scroll
                                .clamp(px(0.), (y - bounds.size.height).max(px(0.)));
                            input.reveal_cursor = false;
                            (
                                entity.clone(),
                                input.lines.clone(),
                                input.scroll,
                                cursor,
                                input.selection.is_empty(),
                                input.ink,
                            )
                        })
                    },
                    move |bounds, (entity, lines, scroll, cursor, collapsed, ink), window, cx| {
                        let focus = entity.read(cx).focus.clone();
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                        for (_, y, line) in lines {
                            let origin = point(bounds.left(), bounds.top() + y - scroll);
                            let _ = line.paint_background(
                                origin,
                                px(26.),
                                TextAlign::Left,
                                Some(bounds),
                                window,
                                cx,
                            );
                            let _ = line.paint(
                                origin,
                                px(26.),
                                TextAlign::Left,
                                Some(bounds),
                                window,
                                cx,
                            );
                        }
                        if collapsed && focus.is_focused(window) {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.left() + cursor.x,
                                        bounds.top() + cursor.y - scroll + px(3.),
                                    ),
                                    size(px(1.5), px(20.)),
                                ),
                                ink,
                            ));
                        }
                    },
                )
                .size_full(),
            )
    }
}

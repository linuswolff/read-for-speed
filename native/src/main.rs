#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod editor;
mod library;
mod platform;
mod reader;

use editor::Editor;
use gpui::{prelude::*, *};
use library::{Library, import_text};
use platform::{COMMAND_LABEL, UI_FONT};
use reader::{Reader, Status};
use std::{
    borrow::Cow,
    path::PathBuf,
    time::{Duration, Instant},
};

actions!(speed, [Quit, Minimize]);

const FOCAL_ANCHOR: f32 = 0.38;
const CONTROLS_IDLE: Duration = Duration::from_secs(3);

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/lucide"]
struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::get(path).map(|asset| asset.data))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|name| name.starts_with(path))
            .map(|name| name.into_owned().into())
            .collect())
    }
}

fn icon(name: &'static str, color: Hsla) -> Svg {
    svg()
        .path(format!("{name}.svg"))
        .size(px(16.))
        .flex_none()
        .text_color(color)
}

#[derive(Clone, Copy)]
struct Palette {
    paper: Hsla,
    sidebar: Hsla,
    card: Hsla,
    ink: Hsla,
    muted: Hsla,
    line: Hsla,
    accent: Hsla,
    tint: Hsla,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                paper: rgb(0x20221f).into(),
                sidebar: rgb(0x191b18).into(),
                card: rgb(0x2a2d28).into(),
                ink: rgb(0xece9df).into(),
                muted: rgb(0xa3a79a).into(),
                line: rgb(0x393d35).into(),
                accent: rgb(0xf18a6c).into(),
                tint: rgb(0x3a2a23).into(),
            }
        } else {
            Self {
                paper: rgb(0xf8f6f0).into(),
                sidebar: rgb(0xefede5).into(),
                card: rgb(0xfffdf8).into(),
                ink: rgb(0x30352d).into(),
                muted: rgb(0x6b7163).into(),
                line: rgb(0xdedfd4).into(),
                accent: rgb(0xc85b3c).into(),
                tint: rgb(0xf1e4d8).into(),
            }
        }
    }
}

struct Draft {
    id: Option<u64>,
    title: Entity<Editor>,
    body: Entity<Editor>,
}

struct Hint(SharedString, Palette);

impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(self.1.card)
            .border_1()
            .border_color(self.1.line)
            .font_family(UI_FONT)
            .text_size(px(12.))
            .text_color(self.1.ink)
            .shadow_sm()
            .child(self.0.clone())
    }
}

struct Speed {
    library: Library,
    reader: Reader,
    path: PathBuf,
    storage_ok: bool,
    focus: FocusHandle,
    library_open: bool,
    settings_open: bool,
    show_help: bool,
    draft: Option<Draft>,
    message: Option<String>,
    timer: Option<Task<()>>,
    idle_timer: Option<Task<()>>,
    last_activity: Instant,
    pointer: Option<Point<Pixels>>,
    controls_visible: bool,
    controls_hovered: bool,
    scrub_bounds: Option<Bounds<Pixels>>,
    scrubbing: bool,
}

impl Speed {
    fn new(cx: &mut Context<Self>, demo: bool) -> Self {
        let path = Library::path();
        let (library, message, storage_ok) = if demo {
            (Library::default(), None, false)
        } else {
            match Library::load(&path) {
                Ok(l) => (l, None, true),
                Err(e) => (Library::default(), Some(e), false),
            }
        };
        let reader = library
            .active_document()
            .map(|d| Reader::new(&d.text, d.position))
            .unwrap_or_else(|| Reader::new("", 0));
        let mut view = Self {
            library,
            reader,
            path,
            storage_ok,
            focus: cx.focus_handle(),
            library_open: false,
            settings_open: false,
            show_help: false,
            draft: None,
            message,
            timer: None,
            idle_timer: None,
            last_activity: Instant::now(),
            pointer: None,
            controls_visible: true,
            controls_hovered: false,
            scrub_bounds: None,
            scrubbing: false,
        };
        view.reveal_controls(cx);
        view
    }

    fn reveal_controls(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        if !self.controls_visible {
            self.controls_visible = true;
            cx.notify();
        }
        if self.idle_timer.is_some() {
            return;
        }
        self.idle_timer = Some(cx.spawn(async move |this, cx| {
            let mut delay = CONTROLS_IDLE;
            loop {
                cx.background_executor().timer(delay).await;
                let next = this.update(cx, |this, cx| {
                    if this.controls_hovered
                        || this.scrubbing
                        || this.library_open
                        || this.settings_open
                        || this.show_help
                        || this.draft.is_some()
                    {
                        return Some(CONTROLS_IDLE);
                    }
                    if let Some(remaining) = CONTROLS_IDLE.checked_sub(this.last_activity.elapsed())
                    {
                        return Some(remaining);
                    }
                    this.controls_visible = false;
                    this.idle_timer = None;
                    cx.notify();
                    None
                });
                match next {
                    Ok(Some(remaining)) => delay = remaining,
                    _ => break,
                }
            }
        }));
    }

    fn save(&mut self) {
        if let Some(d) = self
            .library
            .documents
            .iter_mut()
            .find(|d| Some(d.id) == self.library.active)
        {
            d.position = self.reader.position();
        }
        // A corrupt/unreadable file must never be overwritten by the fallback library.
        if self.storage_ok
            && let Err(e) = self.library.save(&self.path)
        {
            self.message = Some(e);
        }
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        if self.reader.status != Status::Playing {
            return;
        }
        let Some(word) = self.reader.current() else {
            return;
        };
        let delay = cx
            .background_executor()
            .timer(word.duration(self.library.wpm, self.library.natural));
        self.timer = Some(cx.spawn(async move |this, cx| {
            delay.await;
            let _ = this.update(cx, |this, cx| {
                this.reader.tick();
                if this.reader.status == Status::Finished {
                    this.save();
                }
                this.schedule(cx);
                cx.notify();
            });
        }));
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        self.reader.toggle();
        self.schedule(cx);
        if self.reader.status != Status::Playing {
            self.save();
        }
        cx.notify();
    }

    fn seek(&mut self, index: usize, cx: &mut Context<Self>) {
        self.timer = None;
        self.reader.seek(index);
        self.save();
        cx.notify();
    }

    fn speed(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.library.wpm = (self.library.wpm as i32 + delta).clamp(100, 1200) as u32;
        self.schedule(cx);
        self.save();
        cx.notify();
    }

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        self.reader = self
            .library
            .active_document()
            .map(|d| Reader::new(&d.text, d.position))
            .unwrap_or_else(|| Reader::new("", 0));
        self.save();
        self.reveal_controls(cx);
        cx.notify();
    }

    fn select(&mut self, id: u64, cx: &mut Context<Self>) {
        self.save();
        self.library.active = Some(id);
        self.library_open = false;
        self.load_active(cx);
    }

    fn toggle_library(&mut self, cx: &mut Context<Self>) {
        self.library_open = !self.library_open;
        self.settings_open = false;
        if self.library_open {
            self.pause();
        }
        self.reveal_controls(cx);
        cx.notify();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx
            .read_from_clipboard()
            .and_then(|i| i.text())
            .filter(|t| !t.trim().is_empty())
        {
            if text.len() > 2 * 1024 * 1024 {
                self.message = Some("Paste less than 2 MB of text at a time.".into());
            } else {
                self.save();
                self.library.add(&text, None, "Clipboard");
                self.library_open = false;
                self.load_active(cx);
                self.message = None;
            }
        } else {
            self.message = Some(
                "Your clipboard does not contain any text. Copy something, then try again.".into(),
            );
        }
        cx.notify();
    }

    fn pause(&mut self) {
        self.timer = None;
        if self.reader.status == Status::Playing {
            self.reader.status = Status::Paused;
        }
        self.save();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        self.pause();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a text or Markdown file".into()),
        });
        cx.spawn(async move |this, cx| match picker.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
                    let loaded = cx
                        .background_executor()
                        .spawn(async move { import_text(&path) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        match loaded {
                            Ok(text) => {
                                this.save();
                                this.library.add(&text, title, "Text file");
                                this.library_open = false;
                                this.load_active(cx);
                                this.message = None;
                            }
                            Err(e) => this.message = Some(e),
                        }
                        cx.notify();
                    });
                }
            }
            Ok(Ok(None)) => {}
            _ => {
                let _ = this.update(cx, |this, cx| {
                    this.message = Some("The file picker could not be opened.".into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn edit(&mut self, existing: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        self.library_open = false;
        self.settings_open = false;
        let doc = if existing {
            self.library.active_document().cloned()
        } else {
            None
        };
        let p = Palette::new(self.library.dark);
        let title = cx.new(|cx| {
            Editor::new(
                doc.as_ref().map(|d| d.title.clone()).unwrap_or_default(),
                false,
                p.ink,
                p.card,
                cx,
            )
        });
        let body = cx.new(|cx| {
            Editor::new(
                doc.as_ref().map(|d| d.text.clone()).unwrap_or_default(),
                true,
                p.ink,
                p.card,
                cx,
            )
        });
        window.focus(&body.read(cx).focus);
        self.draft = Some(Draft {
            id: doc.map(|d| d.id),
            title,
            body,
        });
        cx.notify();
    }

    fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &self.draft else {
            return;
        };
        let text = draft.body.read(cx).text.trim().to_owned();
        let title = draft.title.read(cx).text.trim().to_owned();
        if text.is_empty() || text.len() > 2 * 1024 * 1024 {
            self.message = Some("Add some text, up to 2 MB, before saving.".into());
            cx.notify();
            return;
        }
        if let Some(id) = draft.id {
            if let Some(d) = self.library.documents.iter_mut().find(|d| d.id == id) {
                if d.text != text {
                    d.position = 0;
                }
                d.word_count = text.split_whitespace().count();
                d.text = text;
                if !title.is_empty() {
                    d.title = title;
                }
            }
        } else {
            self.library.add(&text, Some(title), "Written here");
        }
        self.draft = None;
        self.message = None;
        window.focus(&self.focus);
        self.load_active(cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        let Some(doc) = self.library.active_document() else {
            return;
        };
        let id = doc.id;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Remove this reading?",
            Some(&doc.title),
            &[PromptButton::ok("Remove"), PromptButton::cancel("Cancel")],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.ok() == Some(0) {
                let _ = this.update(cx, |this, cx| {
                    this.save();
                    this.library.documents.retain(|d| d.id != id);
                    if this.library.active == Some(id) {
                        this.library.active = this.library.documents.first().map(|d| d.id);
                        this.load_active(cx);
                    }
                    this.save();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_controls(cx);
        let k = &e.keystroke;
        if self.draft.is_some() {
            if k.key == "escape" {
                self.draft = None;
                self.message = None;
                window.focus(&self.focus);
                cx.notify();
            } else if platform::command(k.modifiers) && k.key == "enter" {
                self.save_draft(window, cx);
            } else if k.key == "tab" {
                if let Some(draft) = &self.draft {
                    let next = if draft.body.read(cx).focus.is_focused(window) {
                        &draft.title
                    } else {
                        &draft.body
                    };
                    window.focus(&next.read(cx).focus);
                }
            } else {
                return;
            }
        } else if self.show_help {
            if k.key == "escape" || k.key == "/" {
                self.show_help = false;
                cx.notify();
            }
        } else {
            if self.library_open
                && !matches!(
                    (platform::command(k.modifiers), k.key.as_str()),
                    (true, "v" | "o" | "n" | "e" | "backspace") | (false, "l" | "escape" | "t")
                )
            {
                return;
            }
            match (platform::command(k.modifiers), k.key.as_str()) {
                (true, "v") => self.paste(cx),
                (true, "o") => self.import(cx),
                (true, "n") => self.edit(false, window, cx),
                (true, "e") => self.edit(true, window, cx),
                (true, "backspace") => self.delete(window, cx),
                (false, "space") if !e.is_held => self.play(cx),
                (false, "left") => self.seek(
                    self.reader
                        .index
                        .saturating_sub(if k.modifiers.shift { 10 } else { 1 }),
                    cx,
                ),
                (false, "right") => self.seek(
                    self.reader
                        .index
                        .saturating_add(if k.modifiers.shift { 10 } else { 1 }),
                    cx,
                ),
                (false, "up") => self.speed(20, cx),
                (false, "down") => self.speed(-20, cx),
                (false, "r") => self.seek(0, cx),
                (false, "l") => self.toggle_library(cx),
                (false, "escape") => {
                    self.library_open = false;
                    self.settings_open = false;
                    cx.notify();
                }
                (false, "p") => {
                    self.library.natural = !self.library.natural;
                    self.schedule(cx);
                    self.save();
                    cx.notify();
                }
                (false, "t") => {
                    self.library.dark = !self.library.dark;
                    self.save();
                    cx.notify();
                }
                (false, "+" | "=") => {
                    self.library.font_size = (self.library.font_size + 4.).min(112.);
                    self.save();
                    cx.notify();
                }
                (false, "-" | "_") => {
                    self.library.font_size = (self.library.font_size - 4.).max(40.);
                    self.save();
                    cx.notify();
                }
                (false, "0") => {
                    self.library.font_size = 76.;
                    self.save();
                    cx.notify();
                }
                (false, "/" | "?") => {
                    self.pause();
                    self.show_help = true;
                    cx.notify();
                }
                _ => return,
            }
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn button(
        &self,
        id: &'static str,
        label: impl IntoElement,
        primary: bool,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let p = Palette::new(self.library.dark);
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .h(px(36.))
            .px_3()
            .rounded_lg()
            .cursor_pointer()
            .bg(if primary { p.ink } else { p.paper })
            .text_color(if primary { p.paper } else { p.ink })
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .border_1()
            .border_color(if primary { p.ink } else { p.line })
            .hover(move |s| {
                s.bg(if primary { p.accent } else { p.tint })
                    .border_color(if primary { p.accent } else { p.line })
            })
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.reveal_controls(cx);
                f(this, window, cx)
            }))
    }

    fn icon_button(
        &self,
        id: &'static str,
        name: &'static str,
        hint: &'static str,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let p = Palette::new(self.library.dark);
        self.button(id, icon(name, p.muted), false, cx, f)
            .border_0()
            .px_0()
            .w(px(32.))
            .h(px(32.))
            .rounded_md()
            .flex_none()
            .text_color(p.muted)
            .tooltip(move |_, cx| cx.new(|_| Hint(hint.into(), p)).into())
    }

    fn library_overlay(&self, window: &Window, cx: &Context<Self>) -> Div {
        let p = Palette::new(self.library.dark);
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.paper.opacity(0.88))
            .child(
                div()
                    .id("library-dialog")
                    .occlude()
                    .flex()
                    .flex_col()
                    .w(px(560.))
                    .max_w(relative(0.9))
                    .max_h(px(
                        (f32::from(window.viewport_size().height) - 96.).clamp(220., 420.)
                    ))
                    .p_4()
                    .rounded_xl()
                    .shadow_lg()
                    .bg(p.paper)
                    .border_1()
                    .border_color(p.line)
                    .on_mouse_down_out(cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        if e.button == MouseButton::Left {
                            this.toggle_library(cx);
                        }
                    }))
                    .child(
                        div()
                            .h(px(32.))
                            .mb_3()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Library")
                            .child(self.icon_button(
                                "close-library",
                                "x",
                                "Close library · Esc",
                                cx,
                                |this, _, cx| this.toggle_library(cx),
                            )),
                    )
                    .child(
                        div()
                            .mb_3()
                            .flex_none()
                            .flex()
                            .gap_2()
                            .child(
                                self.button("import", "Open file", false, cx, |this, _, cx| {
                                    this.import(cx)
                                })
                                .flex_1(),
                            )
                            .child(
                                self.button("new", "New", false, cx, |this, window, cx| {
                                    this.edit(false, window, cx)
                                })
                                .flex_1(),
                            ),
                    )
                    .child(
                        div()
                            .id("library")
                            .flex_shrink()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(self.library.documents.iter().map(|d| {
                                let id = d.id;
                                let active = Some(id) == self.library.active;
                                let position = if active {
                                    self.reader.position()
                                } else {
                                    d.position
                                };
                                let progress = if d.word_count == 0 {
                                    0.
                                } else {
                                    position.min(d.word_count) as f32 / d.word_count as f32
                                };
                                div()
                                    .id(("doc", id))
                                    .p_3()
                                    .mb_2()
                                    .rounded_lg()
                                    .cursor_pointer()
                                    .bg(if active { p.sidebar } else { p.paper })
                                    .hover(move |s| s.bg(p.card))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .line_height(relative(1.4))
                                            .truncate()
                                            .child(d.title.clone()),
                                    )
                                    .child(
                                        div().mt_2().text_size(px(11.)).text_color(p.muted).child(
                                            format!(
                                                "{} words{}",
                                                d.word_count,
                                                if progress >= 1. {
                                                    " · Done".into()
                                                } else if position > 0 {
                                                    format!(
                                                        " · {}%",
                                                        (progress * 100.).round() as u32
                                                    )
                                                } else {
                                                    String::new()
                                                }
                                            ),
                                        ),
                                    )
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.select(id, cx)),
                                    )
                            })),
                    )
                    .when(self.library.active.is_some(), |s| {
                        s.child(
                            div()
                                .pt_3()
                                .flex_none()
                                .flex()
                                .gap_2()
                                .child(
                                    self.button("edit", "Edit", false, cx, |this, window, cx| {
                                        this.edit(true, window, cx)
                                    })
                                    .flex_1(),
                                )
                                .child(
                                    self.button(
                                        "remove",
                                        "Remove",
                                        false,
                                        cx,
                                        |this, window, cx| this.delete(window, cx),
                                    )
                                    .flex_1(),
                                ),
                        )
                    }),
            )
    }

    fn word_canvas(&self) -> impl IntoElement {
        let p = Palette::new(self.library.dark);
        let word = self.reader.current().cloned();
        let preferred_size = self.library.font_size;
        canvas(
            move |bounds, window, _| {
                word.map(|word| {
                    let run = TextRun {
                        len: word.text.len(),
                        font: font("Georgia"),
                        color: p.ink,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let runs: Vec<_> = [
                        TextRun {
                            len: word.focal.start,
                            ..run.clone()
                        },
                        TextRun {
                            len: word.focal.len(),
                            color: p.accent,
                            ..run.clone()
                        },
                        TextRun {
                            len: word.text.len() - word.focal.end,
                            ..run
                        },
                    ]
                    .into_iter()
                    .filter(|r| r.len > 0)
                    .collect();
                    let mut font_size = preferred_size;
                    let mut line = window.text_system().shape_line(
                        word.text.clone().into(),
                        px(font_size),
                        &runs,
                        None,
                    );
                    // Keep the highlighted grapheme on the tick, including with proportional
                    // fonts and long tokens. Fit both sides, not just the word's total width.
                    let mut focal_x = (line.x_for_index(word.focal.start)
                        + line.x_for_index(word.focal.end))
                        / 2.;
                    let left = (bounds.size.width * FOCAL_ANCHOR - px(28.)).max(px(1.));
                    let right = (bounds.size.width * (1. - FOCAL_ANCHOR) - px(28.)).max(px(1.));
                    let fit = (f32::from(left) / f32::from(focal_x.max(px(1.))))
                        .min(f32::from(right) / f32::from((line.width - focal_x).max(px(1.))))
                        .min(1.);
                    if fit < 1. {
                        font_size *= fit;
                        line = window.text_system().shape_line(
                            word.text.into(),
                            px(font_size),
                            &runs,
                            None,
                        );
                        focal_x = (line.x_for_index(word.focal.start)
                            + line.x_for_index(word.focal.end))
                            / 2.;
                    }
                    (line, focal_x, font_size)
                })
            },
            move |bounds, shaped, window, cx| {
                let center = point(
                    bounds.left() + bounds.size.width * FOCAL_ANCHOR,
                    bounds.center().y,
                );
                let guide = p.line;
                for y in [bounds.top() + px(24.), bounds.bottom() - px(24.)] {
                    window.paint_quad(fill(
                        Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.5))),
                        guide,
                    ));
                }
                window.paint_quad(fill(
                    Bounds::new(
                        point(center.x - px(0.75), bounds.top() + px(24.)),
                        size(px(1.5), px(26.)),
                    ),
                    p.accent.opacity(0.65),
                ));
                window.paint_quad(fill(
                    Bounds::new(
                        point(center.x - px(0.75), bounds.bottom() - px(50.)),
                        size(px(1.5), px(26.)),
                    ),
                    p.accent.opacity(0.65),
                ));
                if let Some((line, focal_x, font_size)) = shaped {
                    let height = px(font_size * 1.3);
                    let _ = line.paint(
                        point(center.x - focal_x, center.y - height / 2.),
                        height,
                        window,
                        cx,
                    );
                }
            },
        )
        .w_full()
        .h(px(260.))
    }

    fn scrub(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(bounds) = self.scrub_bounds {
            let fraction = f32::from(p.x - bounds.left()) / f32::from(bounds.size.width);
            let index = (fraction.clamp(0., 1.) * self.reader.words.len().saturating_sub(1) as f32)
                .round() as usize;
            // Save on release, not on every mouse movement.
            self.timer = None;
            self.reader.seek(index);
            cx.notify();
        }
    }

    fn transport(&self, cx: &Context<Self>) -> Stateful<Div> {
        let p = Palette::new(self.library.dark);
        let playing = self.reader.status == Status::Playing;
        let finished = self.reader.status == Status::Finished;
        let count = self.reader.words.len();
        let progress = if finished {
            1.
        } else if count <= 1 {
            0.
        } else {
            self.reader.index as f32 / (count - 1) as f32
        };
        let entity = cx.entity();
        div()
            .id("controls")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(56.))
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .bg(p.paper)
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.controls_hovered = *hovered;
                this.reveal_controls(cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap_1()
                    .child(
                        self.button(
                            "pacing",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(
                                    if self.library.natural {
                                        "waves-horizontal"
                                    } else {
                                        "text-align-justify"
                                    },
                                    p.muted,
                                ))
                                .child(if self.library.natural {
                                    "Natural"
                                } else {
                                    "Steady"
                                }),
                            false,
                            cx,
                            |this, _, cx| {
                                this.library.natural = !this.library.natural;
                                this.schedule(cx);
                                this.save();
                                cx.notify();
                            },
                        )
                        .border_0()
                        .px_2()
                        .h(px(32.))
                        .rounded_md()
                        .w(px(92.))
                        .text_color(p.muted),
                    )
                    .child(self.icon_button(
                        "back",
                        "skip-back",
                        "Back ten words",
                        cx,
                        |this, _, cx| this.seek(this.reader.index.saturating_sub(10), cx),
                    ))
                    .child(
                        self.icon_button(
                            "play",
                            if playing {
                                "pause"
                            } else if finished {
                                "rotate-ccw"
                            } else {
                                "play"
                            },
                            if playing {
                                "Pause"
                            } else if finished {
                                "Replay"
                            } else {
                                "Read"
                            },
                            cx,
                            |this, _, cx| this.play(cx),
                        )
                        .text_color(p.ink),
                    )
                    .child(self.icon_button(
                        "forward",
                        "skip-forward",
                        "Forward ten words",
                        cx,
                        |this, _, cx| this.seek(this.reader.index.saturating_add(10), cx),
                    )),
            )
            .child(
                div()
                    .id("scrubber")
                    .h(px(32.))
                    .flex_1()
                    .min_w(px(48.))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            this.scrubbing = true;
                            this.scrub(e.position, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                        if this.scrubbing {
                            this.scrub(e.position, cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.scrubbing = false;
                            this.save();
                            this.reveal_controls(cx);
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if this.scrubbing {
                                this.scrubbing = false;
                                this.save();
                                this.reveal_controls(cx);
                            }
                        }),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                entity.update(cx, |this, _| this.scrub_bounds = Some(bounds));
                                let y = bounds.center().y;
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left(), y - px(1.5)),
                                        size(bounds.size.width, px(3.)),
                                    ),
                                    p.line,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left(), y - px(1.5)),
                                        size(bounds.size.width * progress, px(3.)),
                                    ),
                                    p.accent,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(
                                            bounds.left() + bounds.size.width * progress - px(3.),
                                            y - px(4.),
                                        ),
                                        size(px(6.), px(8.)),
                                    ),
                                    p.accent,
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .w(px(32.))
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .text_right()
                    .child(format!("{}%", (progress * 100.).round() as u32)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap_1()
                    .child(self.icon_button(
                        "slower",
                        "minus",
                        "Slower · ↓",
                        cx,
                        |this, _, cx| this.speed(-20, cx),
                    ))
                    .child(
                        div()
                            .w(px(64.))
                            .text_center()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(format!("{} WPM", self.library.wpm)),
                    )
                    .child(self.icon_button(
                        "faster",
                        "plus",
                        "Faster · ↑",
                        cx,
                        |this, _, cx| this.speed(20, cx),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap_1()
                    .child(
                        self.icon_button(
                            "library-toggle",
                            "book-open",
                            "Library · L",
                            cx,
                            |this, _, cx| {
                                this.toggle_library(cx);
                            },
                        )
                        .text_color(if self.library_open {
                            p.accent
                        } else {
                            p.muted
                        }),
                    )
                    .child(self.icon_button(
                        "paste",
                        "clipboard-paste",
                        "Paste text",
                        cx,
                        |this, _, cx| this.paste(cx),
                    ))
                    .child(
                        self.icon_button(
                            "settings",
                            "sliders-horizontal",
                            "Settings",
                            cx,
                            |this, _, cx| {
                                this.settings_open = !this.settings_open;
                                cx.notify();
                            },
                        )
                        .text_color(if self.settings_open {
                            p.accent
                        } else {
                            p.muted
                        }),
                    ),
            )
    }

    fn settings(&self, cx: &Context<Self>) -> Div {
        let p = Palette::new(self.library.dark);
        div()
            .absolute()
            .right_4()
            .bottom(px(60.))
            .w(px(218.))
            .p_3()
            .rounded_lg()
            .bg(p.card)
            .border_1()
            .border_color(p.line)
            .shadow_lg()
            .text_size(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, e: &MouseDownEvent, window, cx| {
                // Keep the toolbar interactive, including the settings toggle itself.
                if e.button == MouseButton::Left
                    && e.position.y < window.viewport_size().height - px(56.)
                {
                    this.settings_open = false;
                    this.reveal_controls(cx);
                    cx.notify();
                }
            }))
            .child(
                self.button(
                    "theme",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(if self.library.dark { "sun" } else { "moon" }, p.ink))
                        .child(if self.library.dark {
                            "Light appearance"
                        } else {
                            "Dark appearance"
                        }),
                    false,
                    cx,
                    |this, _, cx| {
                        this.library.dark = !this.library.dark;
                        this.save();
                        cx.notify();
                    },
                )
                .bg(p.card)
                .border_0()
                .w_full()
                .justify_start()
                .px_2(),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_2()
                    .mt_1()
                    .child(icon("type", p.ink))
                    .child(div().flex_1().child("Text size"))
                    .child(
                        self.icon_button("smaller", "minus", "Smaller text", cx, |this, _, cx| {
                            this.library.font_size = (this.library.font_size - 4.).max(40.);
                            this.save();
                            cx.notify();
                        })
                        .bg(p.card),
                    )
                    .child(
                        self.icon_button("larger", "plus", "Larger text", cx, |this, _, cx| {
                            this.library.font_size = (this.library.font_size + 4.).min(112.);
                            this.save();
                            cx.notify();
                        })
                        .bg(p.card),
                    ),
            )
            .child(
                self.button(
                    "help",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon("circle-question-mark", p.ink))
                        .child("Shortcuts"),
                    false,
                    cx,
                    |this, _, cx| {
                        this.pause();
                        this.settings_open = false;
                        this.show_help = true;
                        cx.notify();
                    },
                )
                .bg(p.card)
                .border_0()
                .w_full()
                .justify_start()
                .px_2()
                .mt_1(),
            )
    }

    fn overlay(&self, cx: &Context<Self>) -> Div {
        let p = Palette::new(self.library.dark);
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.ink.opacity(0.25))
            .child(
                div()
                    .id("modal")
                    .w(px(560.))
                    .max_w(relative(0.92))
                    .max_h(relative(0.92))
                    .overflow_y_scroll()
                    .p_5()
                    .bg(p.paper)
                    .border_1()
                    .border_color(p.line)
                    .rounded_xl()
                    .shadow_lg()
                    .when_some(self.draft.as_ref(), |s, draft| {
                        s.child(
                            div()
                                .mb_3()
                                .text_size(px(22.))
                                .font_family("Georgia")
                                .child(if draft.id.is_some() {
                                    "Edit text"
                                } else {
                                    "New text"
                                }),
                        )
                        .child(draft.title.clone())
                        .child(div().mt_3().child(draft.body.clone()))
                        .child(
                            div()
                                .mt_3()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .child(format!("{COMMAND_LABEL}Enter to save")),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(self.button(
                                            "cancel-draft",
                                            "Cancel",
                                            false,
                                            cx,
                                            |this, window, cx| {
                                                this.draft = None;
                                                this.message = None;
                                                window.focus(&this.focus);
                                                cx.notify();
                                            },
                                        ))
                                        .child(self.button(
                                            "save-draft",
                                            "Save",
                                            true,
                                            cx,
                                            |this, window, cx| this.save_draft(window, cx),
                                        )),
                                ),
                        )
                    })
                    .when(self.show_help, |s| {
                        s.child(
                            div()
                                .text_size(px(28.))
                                .font_family("Georgia")
                                .mb_4()
                                .child("Shortcuts"),
                        )
                        .children(
                            [
                                ("Space", "Read / pause / replay"),
                                ("← / →", "Previous / next word"),
                                ("Shift ← / →", "Jump ten words"),
                                ("↑ / ↓", "Adjust speed by 20 WPM"),
                                ("R", "Return to the beginning"),
                                ("Esc", "Close library / settings"),
                                ("L", "Show / hide library"),
                                ("P / T", "Natural pacing / theme"),
                                ("+ / − / 0", "Larger / smaller / reset text"),
                                ("V / O", "Paste / open text file"),
                                ("N / E", "Write / edit reading"),
                                ("Backspace", "Remove reading (asks first)"),
                                ("W / M", "Close / minimize window"),
                            ]
                            .into_iter()
                            .map(|(key, label)| {
                                div()
                                    .flex()
                                    .justify_between()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(p.line)
                                    .text_size(px(13.))
                                    .child(label)
                                    .child(div().text_color(p.muted).child(
                                        if matches!(key, "V / O" | "N / E" | "Backspace" | "W / M")
                                        {
                                            key.split(" / ")
                                                .map(|k| format!("{COMMAND_LABEL}{k}"))
                                                .collect::<Vec<_>>()
                                                .join(" / ")
                                        } else {
                                            key.to_owned()
                                        },
                                    ))
                            }),
                        )
                        .child(div().mt_5().flex().justify_end().child(self.button(
                            "close-help",
                            "Got it",
                            true,
                            cx,
                            |this, _, cx| {
                                this.show_help = false;
                                cx.notify();
                            },
                        )))
                    }),
            )
    }

    fn window_buttons(&self, window: &Window) -> Div {
        let p = Palette::new(self.library.dark);
        div()
            .absolute()
            .top_0()
            .right_0()
            .h(px(48.))
            .flex()
            .children(
                [
                    ("window-min", "minus", WindowControlArea::Min),
                    (
                        "window-max",
                        if window.is_maximized() {
                            "copy"
                        } else {
                            "square"
                        },
                        WindowControlArea::Max,
                    ),
                    ("window-close", "x", WindowControlArea::Close),
                ]
                .into_iter()
                .map(|(id, label, area)| {
                    div()
                        .id(id)
                        .w(px(42.))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(16.))
                        .cursor_pointer()
                        .window_control_area(area)
                        .bg(p.paper)
                        .hover(move |s| {
                            if area == WindowControlArea::Close {
                                s.bg(p.accent).text_color(p.paper)
                            } else {
                                s.bg(p.tint)
                            }
                        })
                        .child(icon(label, p.ink))
                }),
            )
    }
}

impl Render for Speed {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(self.library.dark);
        div()
            .id("speed")
            .size_full()
            .relative()
            .bg(p.paper)
            .text_color(p.ink)
            .font_family(UI_FONT)
            .text_size(px(14.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                // Repaints can synthesize moves at the same position during playback.
                if this.pointer != Some(e.position) {
                    this.pointer = Some(e.position);
                    this.reveal_controls(cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    if this.draft.is_none() {
                        window.focus(&this.focus);
                    }
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(48.))
                    .bottom(px(56.))
                    .left_0()
                    .right_0()
                    .flex()
                    .child(
                        div()
                            .id("reading-surface")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |e, window, _| {
                                platform::drag_window(window, e.click_count);
                            })
                            .child(self.word_canvas())
                            .when(self.library.active_document().is_none(), |s| {
                                s.child(
                                    div()
                                        .text_center()
                                        .text_size(px(13.))
                                        .text_color(p.muted)
                                        .child(format!("{COMMAND_LABEL}V to paste text")),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("window-heading")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(48.))
                    .px_5()
                    .when(cfg!(target_os = "macos"), |s| s.pl(px(100.)))
                    .when(cfg!(target_os = "windows"), |s| s.pr(px(150.)))
                    .flex()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down(MouseButton::Left, |e, window, _| {
                        platform::drag_window(window, e.click_count);
                    })
                    .when(self.controls_visible, |s| {
                        s.child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::MEDIUM)
                                .child("READ FOR SPEED"),
                        )
                    }),
            )
            .when(self.controls_visible, |s| s.child(self.transport(cx)))
            .when(self.settings_open, |s| s.child(self.settings(cx)))
            .when(self.library_open, |s| {
                s.child(self.library_overlay(window, cx))
            })
            .when(self.draft.is_some() || self.show_help, |s| {
                s.child(self.overlay(cx))
            })
            .when_some(self.message.clone(), |s, message| {
                s.child(
                    div()
                        .absolute()
                        .top(px(56.))
                        .left_3()
                        .right_3()
                        .p_3()
                        .bg(p.tint)
                        .border_1()
                        .border_color(p.accent)
                        .rounded_lg()
                        .flex()
                        .justify_between()
                        .gap_4()
                        .text_size(px(12.))
                        .child(message)
                        .child(
                            self.button("dismiss-message", "Dismiss", false, cx, |this, _, cx| {
                                this.message = None;
                                cx.notify();
                            })
                            .flex_none(),
                        ),
                )
            })
            .when(cfg!(target_os = "windows"), |s| {
                s.child(self.window_buttons(window))
            })
    }
}

fn main() {
    let demo = std::env::args().any(|arg| arg == "--demo");
    Application::new()
        .with_assets(Icons)
        .run(move |cx: &mut App| {
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new(platform::QUIT_KEY, Quit, None),
                KeyBinding::new(platform::CLOSE_KEY, Quit, None),
                KeyBinding::new(platform::MINIMIZE_KEY, Minimize, None),
            ]);
            cx.set_menus(vec![Menu {
                name: "SPEED Native".into(),
                items: vec![MenuItem::action("Quit SPEED Native", Quit)],
            }]);
            let bounds = Bounds::centered(None, size(px(960.), px(560.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_min_size: Some(size(px(680.), px(360.))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("READ FOR SPEED".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(16.), px(17.))),
                        }),
                        ..Default::default()
                    },
                    |window, cx| {
                        let view = cx.new(|cx| Speed::new(cx, demo));
                        window.focus(&view.read(cx).focus);
                        let weak = view.downgrade();
                        window.on_window_should_close(cx, move |_, cx| {
                            let _ = weak.update(cx, |this, _| this.save());
                            true
                        });
                        view
                    },
                )
                .expect("Could not open the native reader window");
            let weak = window
                .update(cx, |_, _, cx| cx.entity().downgrade())
                .unwrap();
            cx.on_app_quit(move |cx| {
                let _ = weak.update(cx, |this, _| this.save());
                async {}
            })
            .detach();
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}

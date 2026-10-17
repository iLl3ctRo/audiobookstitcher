//! GPUI user interface. All non-visual logic lives in [`crate::form`].

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, rgb, rgba, size, AnyElement, App,
    Application, Bounds, Context, Entity, IntoElement, PathPromptOptions, Render, ScrollHandle,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    progress::Progress,
    Disableable, Root, Theme, ThemeMode,
};

use crate::{
    form::{self, FormState},
    stitcher::{stitch_audiobook, ProgressSink, ProgressStage, StitchProgress, StitchResult},
};

/// Messages streamed from the tokio worker into the view.
enum JobMessage {
    Progress(StitchProgress),
    Done(Result<StitchResult, String>),
}

/// A message tagged with the id of the job that produced it.
struct Tagged {
    job: u64,
    message: JobMessage,
}

const TEXT: u32 = 0xe2e8f0;
const MUTED: u32 = 0x94a3b8;
const STATUS_WIDTH: f32 = 380.;

pub struct StitcherApp {
    runtime: Arc<tokio::runtime::Runtime>,
    source_dir: Entity<InputState>,
    output_file: Entity<InputState>,
    title: Entity<InputState>,
    author: Entity<InputState>,
    narrator: Entity<InputState>,
    album: Entity<InputState>,
    year: Entity<InputState>,
    cover_image: Entity<InputState>,
    bitrate: Entity<InputState>,
    sample_rate: Entity<InputState>,
    workers: Entity<InputState>,
    running: bool,
    progress: Option<StitchProgress>,
    log: Vec<String>,
    error: Option<String>,
    /// Form/cover validation message, kept separate from job errors.
    validation_error: Option<String>,
    job_id: u64,
    result: Option<StitchResult>,
    log_scroll: ScrollHandle,
    /// The running stitch task, aborted on quit so its ffmpeg children are killed.
    job_task: Option<tokio::task::AbortHandle>,
    _quit_hook: gpui::Subscription,
}

fn make_input(
    window: &mut Window,
    cx: &mut Context<StitcherApp>,
    value: &str,
    placeholder: &str,
) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    let value = value.to_string();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).placeholder(placeholder);
        if !value.is_empty() {
            state.set_value(value, window, cx);
        }
        state
    })
}

fn timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

impl StitcherApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let defaults = FormState::default();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("stitch-worker")
            .build()
            .expect("failed to start tokio runtime");
        let quit_hook = cx.on_app_quit(|this, _cx| {
            this.abort_job();
            async {}
        });
        Self {
            runtime: Arc::new(runtime),
            source_dir: make_input(
                window,
                cx,
                "",
                "Select the folder containing chapter audio files",
            ),
            output_file: make_input(
                window,
                cx,
                "",
                "Defaults to <folder>.m4b inside the source directory",
            ),
            title: make_input(window, cx, "", ""),
            author: make_input(window, cx, "", ""),
            narrator: make_input(window, cx, "", ""),
            album: make_input(window, cx, "", ""),
            year: make_input(window, cx, "", ""),
            cover_image: make_input(window, cx, "", ""),
            bitrate: make_input(window, cx, &defaults.bitrate, ""),
            sample_rate: make_input(window, cx, &defaults.sample_rate, ""),
            workers: make_input(window, cx, "", "Auto"),
            running: false,
            progress: None,
            log: Vec::new(),
            error: None,
            validation_error: None,
            job_id: 0,
            log_scroll: ScrollHandle::new(),
            job_task: None,
            _quit_hook: quit_hook,
            result: None,
        }
    }

    fn read(&self, input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    fn form_state(&self, cx: &App) -> FormState {
        FormState {
            source_dir: self.read(&self.source_dir, cx),
            output_file: self.read(&self.output_file, cx),
            title: self.read(&self.title, cx),
            author: self.read(&self.author, cx),
            narrator: self.read(&self.narrator, cx),
            album: self.read(&self.album, cx),
            year: self.read(&self.year, cx),
            cover_image: self.read(&self.cover_image, cx),
            bitrate: self.read(&self.bitrate, cx),
            sample_rate: self.read(&self.sample_rate, cx),
            workers: self.read(&self.workers, cx),
        }
    }

    fn append_log(&mut self, message: &str) {
        form::push_log(&mut self.log, format!("[{}] {message}", timestamp()));
        self.log_scroll.scroll_to_bottom();
    }

    fn set_input(
        input: &Entity<InputState>,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.to_string();
        input.update(cx, |state, cx| state.set_value(value, window, cx));
    }

    fn pick_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select chapter directory".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let value = path.display().to_string();
                    this.update_in(cx, |this, window, cx| {
                        Self::set_input(&this.source_dir, &value, window, cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn pick_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select cover image".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let value = path.display().to_string();
                    this.update_in(cx, |this, window, cx| {
                        // The native picker cannot filter extensions, so validate here.
                        if form::has_extension(&value, &["jpg", "jpeg", "png"]) {
                            this.validation_error = None;
                            Self::set_input(&this.cover_image, &value, window, cx);
                        } else {
                            this.validation_error = Some(form::COVER_ERROR.into());
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn pick_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.read(&self.source_dir, cx);
        let source = source.trim();
        let dir = if !source.is_empty() && Path::new(source).is_dir() {
            PathBuf::from(source)
        } else {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        };
        let name = format!("{}.m4b", form::infer_default_name(source));
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let value = form::ensure_m4b(&path.display().to_string());
                this.update_in(cx, |this, window, cx| {
                    Self::set_input(&this.output_file, &value, window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let d = FormState::default();
        let pairs = [
            (&self.source_dir, d.source_dir),
            (&self.output_file, d.output_file),
            (&self.title, d.title),
            (&self.author, d.author),
            (&self.narrator, d.narrator),
            (&self.album, d.album),
            (&self.year, d.year),
            (&self.cover_image, d.cover_image),
            (&self.bitrate, d.bitrate),
            (&self.sample_rate, d.sample_rate),
            (&self.workers, d.workers),
        ];
        for (input, value) in pairs {
            Self::set_input(input, &value, window, cx);
        }
        self.progress = None;
        self.log.clear();
        self.error = None;
        self.validation_error = None;
        self.result = None;
        cx.notify();
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let options = match self.form_state(cx).to_options() {
            Ok(options) => options,
            Err(message) => {
                self.validation_error = Some(message.to_string());
                self.progress = None;
                cx.notify();
                return;
            }
        };

        self.running = true;
        self.job_id += 1;
        let job = self.job_id;
        self.progress = None;
        self.error = None;
        self.validation_error = None;
        self.result = None;
        self.append_log("Starting stitch job...");
        cx.notify();

        let (tx, rx) = async_channel::unbounded::<Tagged>();
        let progress_tx = tx.clone();
        let sink: ProgressSink = Arc::new(move |p| {
            let _ = progress_tx.try_send(Tagged {
                job,
                message: JobMessage::Progress(p),
            });
        });
        let failed_sink = sink.clone();
        let handle = self.runtime.spawn(async move {
            match stitch_audiobook(sink, options).await {
                Ok(result) => Ok(result),
                Err(err) => {
                    let message = err.to_string();
                    failed_sink(StitchProgress {
                        stage: ProgressStage::Failed,
                        current: 0,
                        total: 0,
                        message: Some(message.clone()),
                    });
                    Err(message)
                }
            }
        });
        self.job_task = Some(handle.abort_handle());
        self.runtime.spawn(async move {
            let outcome = match handle.await {
                Ok(outcome) => outcome,
                Err(_) => Err("stitch job panicked".to_string()),
            };
            let _ = tx.try_send(Tagged {
                job,
                message: JobMessage::Done(outcome),
            });
        });

        cx.spawn_in(window, async move |this, cx| {
            let mut finished = false;
            while let Ok(tagged) = rx.recv().await {
                let done = matches!(tagged.message, JobMessage::Done(_));
                let alive = this
                    .update(cx, |this, cx| {
                        this.handle_message(tagged);
                        cx.notify();
                    })
                    .is_ok();
                if done || !alive {
                    finished = done;
                    break;
                }
            }
            if !finished {
                // Channel closed without a Done message: treat as failure.
                this.update(cx, |this, cx| {
                    this.handle_message(Tagged {
                        job,
                        message: JobMessage::Done(Err("stitch job ended unexpectedly".to_string())),
                    });
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Cancels the running job and waits briefly for it to be dropped. Dropping
    /// it drops the ffmpeg `Child`ren, which `kill_on_drop` then kills. Without
    /// this, quitting mid-job leaves ffmpeg running as orphans.
    fn abort_job(&mut self) {
        let Some(task) = self.job_task.take() else {
            return;
        };
        task.abort();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !task.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn handle_message(&mut self, tagged: Tagged) {
        if tagged.job != self.job_id {
            return; // stale message from an older job
        }
        match tagged.message {
            JobMessage::Progress(progress) => {
                if let Some(text) = progress.message.as_deref().filter(|m| !m.is_empty()) {
                    self.append_log(text);
                }
                self.progress = Some(progress);
            }
            JobMessage::Done(Ok(result)) => {
                self.append_log(&format!("Stitching complete: {}", result.output));
                self.result = Some(result);
                self.running = false;
            }
            JobMessage::Done(Err(message)) => {
                self.append_log(&format!("Error: {message}"));
                self.error = Some(message);
                self.running = false;
            }
        }
    }

    // ---- view helpers ----

    fn label(text: &str) -> impl IntoElement {
        div()
            .text_sm()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(TEXT))
            .child(text.to_string())
    }

    fn legend(text: &str) -> impl IntoElement {
        div()
            .text_xs()
            .text_color(rgb(MUTED))
            .mb_2()
            .child(text.to_uppercase())
    }

    fn field(&self, label: &str, input: &Entity<InputState>) -> AnyElement {
        div()
            .flex_1()
            .min_w(px(160.))
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::label(label))
            .child(Input::new(input).disabled(self.running))
            .into_any_element()
    }

    fn browse_field(
        &self,
        label: &str,
        id: &'static str,
        input: &Entity<InputState>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_w(px(160.))
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::label(label))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(input).disabled(self.running)),
                    )
                    .child(
                        Button::new(id)
                            .label("Browse")
                            .disabled(self.running)
                            .on_click(
                                cx.listener(move |this, _, window, cx| on_click(this, window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn card() -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .p_5()
            .rounded_2xl()
            .border_1()
            .border_color(rgba(0x94a3b81f))
            .bg(rgba(0x0f172acc))
    }

    fn form_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.running;
        let source = self.browse_field(
            "Chapter directory",
            "browse-source",
            &self.source_dir,
            |this, window, cx| this.pick_source(window, cx),
            cx,
        );
        let output = self.browse_field(
            "Output file",
            "browse-output",
            &self.output_file,
            |this, window, cx| this.pick_output(window, cx),
            cx,
        );
        let cover = self.browse_field(
            "Cover",
            "browse-cover",
            &self.cover_image,
            |this, window, cx| this.pick_cover(window, cx),
            cx,
        );

        let fields = div()
            .id("form-scroll")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .overflow_y_scroll()
            .child(Self::legend("Source"))
            .child(source)
            .child(output)
            .child(div().mt_3().child(Self::legend("Metadata")))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.field("Title", &self.title))
                    .child(self.field("Author", &self.author)),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.field("Narrator", &self.narrator))
                    .child(self.field("Album", &self.album)),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.field("Year", &self.year))
                    .child(cover),
            )
            .child(div().mt_3().child(Self::legend("Encoding")))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.field("Bitrate", &self.bitrate))
                    .child(self.field("Sample rate", &self.sample_rate))
                    .child(self.field("Workers", &self.workers)),
            );

        Self::card()
            .id("form")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_3()
            .child(fields)
            .child(
                div()
                    .flex()
                    .gap_3()
                    .justify_end()
                    .child(
                        Button::new("start")
                            .primary()
                            .label(if running {
                                "Stitching…"
                            } else {
                                "Start stitching"
                            })
                            .disabled(running)
                            .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
                    )
                    .child(
                        Button::new("reset")
                            .label("Reset form")
                            .disabled(running)
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    ),
            )
    }

    fn status_panel(&self) -> impl IntoElement {
        let (stage_label, pill_bg, pill_fg) = match self.progress.as_ref().map(|p| p.stage) {
            None => ("idle", 0x94a3b833, 0xe2e8f0d9),
            Some(ProgressStage::Complete) => ("complete", 0x22c55e33, 0x34d399ff),
            Some(ProgressStage::Failed) => ("failed", 0xf8717133, 0xf87171ff),
            Some(stage) => (stage.label(), 0x94a3b833, 0xe2e8f0d9),
        };
        let percent = self
            .progress
            .as_ref()
            .map(|p| form::percent(p.current, p.total))
            .unwrap_or(0);
        let details: SharedString = self
            .progress
            .as_ref()
            .and_then(|p| p.message.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "Waiting to start".to_string())
            .into();

        let progress_card = Self::card()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Progress"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .text_xs()
                            .bg(rgba(pill_bg))
                            .text_color(rgba(pill_fg))
                            .child(stage_label.to_uppercase()),
                    ),
            )
            .child(Progress::new().value(percent as f32))
            // Only a short summary here; the full message lives in the Error card and log.
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .line_clamp(3)
                    .text_ellipsis()
                    .child(details),
            );

        let mut panel = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(STATUS_WIDTH))
            .min_h_0()
            .gap_5()
            .child(progress_card);

        if let Some(result) = &self.result {
            let row = |label: &str, value: String| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(label.to_uppercase()),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(value),
                    )
            };
            panel = panel.child(
                Self::card()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Latest result"),
                    )
                    .child(row("Output", result.output.clone()))
                    .child(row("Chapters merged", result.file_count.to_string()))
                    .child(row(
                        "Elapsed",
                        format!("{:.1}s", result.duration_ms as f64 / 1000.0),
                    )),
            );
        }

        if let Some(error) = self.validation_error.as_ref().or(self.error.as_ref()) {
            panel = panel.child(
                Self::card()
                    .gap_3()
                    .border_color(rgba(0xf8717159))
                    .bg(rgba(0xf871711f))
                    .text_color(rgb(0xfee2e2))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Error"),
                    )
                    .child(
                        div()
                            .id("error-text")
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .text_sm()
                            .child(error.clone()),
                    ),
            );
        }

        let log_body = if self.log.is_empty() {
            div()
                .text_color(rgb(MUTED))
                .child("No messages yet.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(self.log.iter().map(|line| div().child(line.clone())))
                .into_any_element()
        };
        panel.child(
            Self::card()
                .flex_1()
                .min_h(px(120.))
                .gap_3()
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Log"),
                )
                .child(
                    div()
                        .id("log-view")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.log_scroll)
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(rgb(0xcbd5e1))
                        .child(log_body),
                ),
        )
    }
}

impl Render for StitcherApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_5()
            .p_6()
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgb(0x0f172a), 0.),
                linear_color_stop(rgb(0x132042), 1.),
            ))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Audiobook Stitcher"),
                    )
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child("Normalise chapters and produce polished M4B audiobooks."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_5()
                    .child(self.form_panel(cx))
                    .child(self.status_panel()),
            )
    }
}

gpui::actions!(audiobook_stitcher, [Quit]);

fn set_menus(cx: &mut App) {
    use gpui::{KeyBinding, Menu, MenuItem};
    use gpui_component::input::{Copy, Cut, Paste, SelectAll};

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
    cx.set_menus(vec![
        Menu {
            name: "Audiobook Stitcher".into(),
            items: vec![MenuItem::action("Quit Audiobook Stitcher", Quit)],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Cut", Cut),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Paste", Paste),
                MenuItem::separator(),
                MenuItem::action("Select All", SelectAll),
            ],
        },
    ]);
}

/// Opens the main window and runs the GPUI event loop.
pub fn run() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        set_menus(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        Theme::change(ThemeMode::Dark, None, cx);

        let bounds = Bounds::centered(None, size(px(1100.), px(820.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Audiobook Stitcher".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| StitcherApp::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open window");
        cx.activate(true);
    });
}

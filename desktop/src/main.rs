//! Rightchess desktop: a chess.com game browser that shows the recovered games as raw objects.

// The rsx! macro's expansion carries spans that trip this lint on otherwise-qualified paths.
#![allow(unused_qualifications, reason = "rsx! macro expansion spans")]

use components::badge::{Badge, BadgeVariant};
use components::button::{Button, ButtonSize, ButtonVariant};
use components::progress::Progress;
use dioxus::prelude::*;
use rightchess::adapters::MemoryStore;
use rightchess::{FailedBatch, Game, SyncReport, YearMonth};

mod components;
mod json;
mod service;
mod time;

const CSS: &str = include_str!("../assets/main.css");
/// Theme variables the vendored components style themselves from.
const THEME: &str = include_str!("../assets/dx-components-theme.css");

fn main() {
    let config = dioxus::desktop::Config::new()
        .with_window(dioxus::desktop::WindowBuilder::new().with_title("Rightchess"))
        .with_background_color((246, 245, 242, 255));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

/// The username and date range a sync was run with. Results are labelled with this, not with the
/// live form, so editing the form doesn't misrepresent what's on screen.
#[derive(Clone, PartialEq, Eq)]
struct Submission {
    username: String,
    since: Option<YearMonth>,
}

#[derive(Clone, PartialEq)]
enum Load {
    Idle,
    Running(Submission),
    Ready {
        submission: Submission,
        report: SyncReport,
    },
    Failed(Submission, String),
}

/// A game as shown in the list.
#[derive(Clone, PartialEq)]
struct Row {
    index: usize,
    id: String,
    caption: String,
}

impl Row {
    fn new(index: usize, game: &Game) -> Self {
        Self {
            index,
            id: short_id(&game.id.external_id),
            caption: json::meta(game).caption(),
        }
    }
}

fn short_id(id: &str) -> String {
    const KEEP: usize = 12;
    if id.chars().count() <= KEEP {
        return id.to_string();
    }
    let head: String = id.chars().take(KEEP).collect();
    format!("{head}…")
}

#[component]
fn App() -> Element {
    let mut username = use_signal(String::new);
    let mut slider = use_signal(max_slider);
    let held = use_signal(MemoryStore::new);
    let state = use_signal(|| Load::Idle);
    let mut selected = use_signal(|| None::<usize>);
    let max = max_slider();
    let running = matches!(&*state.read(), Load::Running(_));

    let since_text = match selected_since(slider()) {
        Some(month) => time::label(month),
        None => "All history".to_string(),
    };

    let (rows, code, status) = {
        let current = state.read();
        match &*current {
            Load::Idle => (
                Vec::new(),
                None,
                "Enter a Chess.com username and press Sync.".to_string(),
            ),
            Load::Running(submission) => (
                Vec::new(),
                None,
                format!("Syncing {}…", submission.username),
            ),
            Load::Failed(submission, error) => (
                Vec::new(),
                None,
                format!("Sync failed for {}: {error}", submission.username),
            ),
            Load::Ready { submission, report } => (
                report
                    .games
                    .iter()
                    .enumerate()
                    .map(|(index, game)| Row::new(index, game))
                    .collect(),
                selected()
                    .and_then(|index| report.games.get(index))
                    .map(json::object),
                summary(submission, report),
            ),
        }
    };

    let highlighted = code.as_deref().map(json::highlight);
    let has_code = code.is_some();
    let copy_text = code;

    rsx! {
        style { dangerous_inner_html: THEME }
        style { dangerous_inner_html: CSS }
        div { class: "app",
            header { class: "topbar",
                span { class: "brand", "Rightchess" }
                span { class: "brand-sub", "Chess.com game browser" }
            }
            section { class: "controls",
                div { class: "field",
                    label { class: "field-label", r#for: "username", "Chess.com username" }
                    input {
                        id: "username",
                        class: "text-input",
                        r#type: "text",
                        placeholder: "e.g. hikaru",
                        autocomplete: "off",
                        value: "{username}",
                        disabled: running,
                        oninput: move |event| username.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter {
                                start_sync(username, slider, held, state, selected);
                            }
                        },
                    }
                }
                div { class: "field since-field",
                    label { class: "field-label", r#for: "since", "Since" }
                    input {
                        id: "since",
                        class: "slider",
                        r#type: "range",
                        min: "0",
                        max: "{max}",
                        step: "1",
                        value: "{slider}",
                        disabled: running,
                        oninput: move |event| {
                            if let Ok(value) = event.value().parse::<i32>() {
                                slider.set(value);
                            }
                        },
                    }
                    div { class: "slider-caption",
                        span { "All history" }
                        span { class: "slider-value", "{since_text}" }
                        span { "Current month" }
                    }
                }
                Button {
                    disabled: running,
                    onclick: move |_| start_sync(username, slider, held, state, selected),
                    if running { "Syncing…" } else { "Sync" }
                }
            }
            main { class: "workspace",
                aside { class: "games-panel",
                    div { class: "panel-head",
                        span { "Games" }
                        Badge { variant: BadgeVariant::Secondary, "{rows.len()}" }
                    }
                    if rows.is_empty() {
                        div { class: "empty", "No games loaded yet." }
                    } else {
                        ul { class: "games",
                            for row in rows.iter() {
                                li {
                                    key: "{row.index}",
                                    class: if selected() == Some(row.index) { "game selected" } else { "game" },
                                    onclick: {
                                        let index = row.index;
                                        move |_| selected.set(Some(index))
                                    },
                                    span { class: "game-id", "{row.id}" }
                                    if !row.caption.is_empty() {
                                        span { class: "game-caption", "{row.caption}" }
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "inspector",
                    div { class: "panel-head",
                        span { "Game object" }
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            disabled: !has_code,
                            onclick: move |_| {
                                if let Some(text) = copy_text.clone() {
                                    json::copy(&text);
                                }
                            },
                            "Copy"
                        }
                    }
                    if let Some(html) = highlighted {
                        pre { class: "code", dangerous_inner_html: html }
                    } else {
                        div { class: "empty", "Select a game to inspect its raw game object." }
                    }
                }
            }
            footer { class: "statusbar",
                if running {
                    Progress { class: "sync-progress", aria_label: "Sync progress" }
                }
                span { "{status}" }
            }
        }
    }
}

fn start_sync(
    username: Signal<String>,
    slider: Signal<i32>,
    mut held: Signal<MemoryStore>,
    mut state: Signal<Load>,
    mut selected: Signal<Option<usize>>,
) {
    let name = username().trim().to_string();
    let since = selected_since(slider());
    if name.is_empty() {
        state.set(Load::Failed(
            Submission {
                username: name,
                since,
            },
            "Enter a Chess.com username first.".to_string(),
        ));
        return;
    }

    let submission = Submission {
        username: name,
        since,
    };
    let store = std::mem::take(&mut *held.write());
    state.set(Load::Running(submission.clone()));
    selected.set(None);

    let worker_user = submission.username.clone();
    spawn(async move {
        let outcome =
            tokio::task::spawn_blocking(move || service::run(store, worker_user, since)).await;
        let load = match outcome {
            Err(join_error) => {
                Load::Failed(submission, format!("the sync task failed: {join_error}"))
            }
            Ok(synced) => {
                held.set(synced.store);
                match synced.result {
                    Err(error) => Load::Failed(submission, format!("{error:#}")),
                    Ok(SyncReport {
                        games,
                        failed: Some(failed),
                        ..
                    }) if games.is_empty() => Load::Failed(submission, failure(&failed)),
                    Ok(report) => {
                        selected.set((!report.games.is_empty()).then_some(0));
                        Load::Ready { submission, report }
                    }
                }
            }
        };
        state.set(load);
    });
}

/// The largest slider position: the current month.
fn max_slider() -> i32 {
    time::index(time::current()) + 1
}

/// Position `0` is "all history"; every other position is that many months after the first.
fn selected_since(position: i32) -> Option<YearMonth> {
    (position > 0).then(|| time::at(position - 1))
}

fn summary(submission: &Submission, report: &SyncReport) -> String {
    let range = match submission.since {
        Some(month) => format!("since {}", time::label(month)),
        None => "all history".to_string(),
    };
    let partial = report
        .failed
        .as_ref()
        .map(|failed| format!("partial: {}", failure(failed)));
    [
        format!("Synced {}", submission.username),
        range,
        format!("{} game(s)", report.games.len()),
        format!("fetched {}", report.num_fetched),
        format!("skipped {}", report.num_skipped),
    ]
    .into_iter()
    .chain(partial)
    .collect::<Vec<_>>()
    .join(" · ")
}

fn failure(failed: &FailedBatch) -> String {
    format!("batch {}: {}", failed.batch.id, failed.reason)
}

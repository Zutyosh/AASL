//! The compile screen: shown while a recipe's bundle builds. Progress is
//! read from the builder's own log lines (build.sh and podman), so it
//! follows the real stages instead of a timer.

use std::time::Duration;

use freya::animation::{AnimNum, Ease, Function, OnCreation, OnFinish, use_animation};
use freya::prelude::*;

use crate::i18n::t;
use crate::loader::OrbitLoader;
use crate::state::Ui;
use crate::svc;
use crate::theme::{RADIUS_PANEL, T, alpha, size, space};
use crate::widgets::{Tone, body, dot, muted};

/// The stages of a build, in order, with the share of the bar each ends at.
const STAGES: &[(&str, f32)] = &[
    ("build.stage.image", 0.10),
    ("build.stage.sources", 0.25),
    ("build.stage.prep", 0.40),
    ("build.stage.patch", 0.45),
    ("build.stage.compile", 0.95),
    ("build.stage.package", 1.00),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// Index into STAGES.
    pub stage: usize,
    /// 0.0 to 1.0.
    pub fraction: f32,
    /// What is happening right now, from the log.
    pub detail: String,
    pub failed: Option<String>,
}

/// Read the progress of the latest build from log lines.
pub fn parse(lines: &[String], total_dlls: usize) -> Progress {
    let start = lines
        .iter()
        .rposition(|l| l.starts_with("patch_build: started"))
        .map_or(0, |i| i + 1);
    let mut p = Progress::default();
    let mut makes = 0usize;
    let mut frac: f32 = 0.0;
    let mut set = |p: &mut Progress, stage: usize, f: f32| {
        p.stage = p.stage.max(stage);
        frac = frac.max(f);
    };
    for l in &lines[start..] {
        let s = l.trim();
        if let Some(rest) = s.strip_prefix("STEP ") {
            // podman: "STEP 2/6: RUN ..."
            let (n, m) = rest
                .split(':')
                .next()
                .and_then(|x| x.split_once('/'))
                .unwrap_or(("0", "1"));
            let (n, m) = (
                n.parse::<f32>().unwrap_or(0.),
                m.parse::<f32>().unwrap_or(1.).max(1.),
            );
            set(&mut p, 0, 0.10 * n / m);
        } else if s.starts_with("fetching ") {
            set(&mut p, 1, 0.18);
        } else if s.starts_with("GE wine preparation") {
            set(&mut p, 2, 0.28);
        } else if s.starts_with("GE preparation done") {
            set(&mut p, 2, 0.40);
        } else if s.starts_with("apply ") || s.starts_with("skip ") {
            set(&mut p, 3, 0.43);
        } else if s.starts_with("make dlls/") {
            makes += 1;
            let done = (makes.saturating_sub(1)) as f32 / total_dlls.max(makes) as f32;
            set(&mut p, 4, 0.45 + 0.5 * done);
        } else if s.starts_with("== Done") {
            set(&mut p, 5, 1.0);
        } else if let Some(r) = s.strip_prefix("FATAL ") {
            p.failed = Some(r.to_string());
        }
        if !s.is_empty() && !s.starts_with("[") && s.len() < 200 {
            p.detail = s.to_string();
        }
    }
    p.fraction = frac;
    p
}

#[derive(PartialEq)]
pub struct BuildScreen(pub Ui);

impl Component for BuildScreen {
    fn render(&self) -> impl IntoElement {
        let mut ui = self.0;
        let total = use_state(|| {
            svc()
                .active_plan()
                .map(|p| p.groups().iter().map(|g| g.dlls.len()).sum())
                .unwrap_or(4)
        });
        let mut progress = use_state(Progress::default);
        let mut started = use_state(std::time::Instant::now);
        use_future(move || async move {
            started.set(std::time::Instant::now());
            loop {
                let lines: Vec<String> = svc()
                    .tail_logs(None, 2000)
                    .into_iter()
                    .map(|l| l.line)
                    .collect();
                let p = parse(&lines, *total.peek());
                if *progress.peek() != p {
                    progress.set(p);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
        // The bar eases towards the parsed value instead of jumping.
        let shimmer = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::reverse());
            AnimNum::new(0., 1.)
                .time(1200)
                .function(Function::Sine)
                .ease(Ease::InOut)
        });
        let s = shimmer.get().value();
        let p = progress.read().clone();
        let elapsed = started.read().elapsed().as_secs();
        let stages: Vec<Element> = STAGES
            .iter()
            .enumerate()
            .map(|(i, (key, _))| {
                let (tone, op) = match i.cmp(&p.stage) {
                    std::cmp::Ordering::Less => (Tone::Ok, 1.0),
                    std::cmp::Ordering::Equal if p.failed.is_some() => (Tone::Error, 1.0),
                    std::cmp::Ordering::Equal => (Tone::Accent, 0.6 + 0.4 * s),
                    std::cmp::Ordering::Greater => (Tone::Neutral, 0.45),
                };
                rect()
                    .key(*key)
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(space::S)
                    .opacity(op)
                    .child(dot(tone))
                    .child(body(t(key)))
                    .into_element()
            })
            .collect();
        // Percent, not fill: an absolute child's fill is the space left
        // after its siblings, which is none here.
        rect()
            .position(Position::new_absolute().top(0.).left(0.))
            .width(Size::percent(100.))
            .height(Size::percent(100.))
            .background(alpha(T.bg, 0.94))
            .layer(Layer::Overlay)
            .center()
            .child(
                rect()
                    .width(Size::px(560.))
                    .padding(space::XXL)
                    .spacing(space::L)
                    .corner_radius(RADIUS_PANEL)
                    .background(T.surface)
                    .border(Border::new().fill(T.border).width(1.))
                    .cross_align(Alignment::Center)
                    .child(OrbitLoader)
                    .child(
                        label()
                            .text(t("build.title"))
                            .font_size(size::TITLE)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(T.text),
                    )
                    .child(muted(t("build.sub")))
                    .child(
                        rect().width(Size::fill()).child(
                            ProgressBar::new((p.fraction * 100.).clamp(1., 100.))
                                .show_progress(true)
                                .width(Size::fill()),
                        ),
                    )
                    .child(muted(format!(
                        "{} · {}:{:02}",
                        t(STAGES[p.stage].0),
                        elapsed / 60,
                        elapsed % 60
                    )))
                    .child(
                        rect()
                            .width(Size::fill())
                            .spacing(space::S)
                            .children(stages),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding(space::M)
                            .corner_radius(8.)
                            .background(T.bg)
                            .child(
                                label()
                                    .text(p.failed.clone().unwrap_or_else(|| p.detail.clone()))
                                    .font_family("monospace")
                                    .font_size(size::SMALL)
                                    .max_lines(2)
                                    .color(if p.failed.is_some() { T.error } else { T.muted }),
                            ),
                    )
                    .child(
                        Button::new()
                            .flat()
                            .on_press(move |_| ui.build_hidden.set(true))
                            .child(t("build.hide")),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn follows_the_build() {
        let p = parse(
            &l(&[
                "old line",
                "patch_build: started",
                "STEP 3/6: COPY build.sh",
            ]),
            4,
        );
        assert_eq!(p.stage, 0);
        assert!((p.fraction - 0.05).abs() < 1e-3);
        let p = parse(
            &l(&[
                "patch_build: started",
                "   fetching https://x",
                "   apply 0001-a.patch",
                "   make dlls/secur32",
                "   make dlls/crypt32",
            ]),
            4,
        );
        assert_eq!(p.stage, 4);
        assert!((p.fraction - (0.45 + 0.5 * 0.25)).abs() < 1e-3, "{p:?}");
        assert_eq!(p.detail, "make dlls/crypt32");
        let p = parse(&l(&["patch_build: started", "== Done: /out/x"]), 4);
        assert_eq!((p.stage, p.fraction), (5, 1.0));
        let p = parse(
            &l(&[
                "patch_build: started",
                "FATAL 0001-x.patch does not apply to this tree",
            ]),
            4,
        );
        assert_eq!(
            p.failed.as_deref(),
            Some("0001-x.patch does not apply to this tree")
        );
    }
}

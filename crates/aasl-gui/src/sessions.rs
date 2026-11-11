//! Sessions: what each play session showed, and a closer look at one.

use std::sync::OnceLock;

use aasl_patches::community::{Crash, DipCause, HITCH_MS};
use aasl_patches::timeline::Timeline;
use aasl_service::SavedReport;
use freya::prelude::*;

use crate::i18n::{Lang, current, fill, recipe_title, t};
use crate::state::Ui;
use crate::svc;
use crate::theme::{RADIUS_CONTROL, Rgb, T, alpha, size, space};
use crate::widgets::{Tone, banner, body, chip, group, muted, row, title};

#[cfg(test)]
thread_local! {
    /// Snapshot test: open the newest session's detail.
    pub static TEST_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn first_open() -> Option<String> {
    #[cfg(test)]
    if TEST_OPEN.with(|o| o.get()) {
        return svc().sessions().first().map(|s| s.id.clone());
    }
    None
}

// ---- words and numbers -------------------------------------------------

fn dur(s: u64) -> String {
    let m = s / 60;
    if m >= 60 {
        format!("{} h {:02}", m / 60, m % 60)
    } else {
        format!("{} min", m.max(1))
    }
}

fn clock(s: f32) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn utc_offset_s() -> i64 {
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() != 5 {
                    return None;
                }
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let h: i64 = s[1..3].parse().ok()?;
                let m: i64 = s[3..5].parse().ok()?;
                Some(sign * (h * 3600 + m * 60))
            })
            .unwrap_or(0)
    })
}

/// Local date and time: "11 nov. 2026, 13:13".
pub(crate) fn when(secs: u64) -> String {
    let local = secs as i64 + utc_offset_s();
    let days = local.div_euclid(86400);
    let rem = local.rem_euclid(86400);
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    const FR: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    match current() {
        Lang::Fr => format!("{d} {} {y}, {h:02}:{m:02}", FR[(mo - 1) as usize]),
        Lang::En => format!("{} {d}, {y}, {h:02}:{m:02}", EN[(mo - 1) as usize]),
    }
}

fn worlds(n: u32) -> String {
    if n == 1 {
        t("ses.world.one").to_string()
    } else {
        format!("{n} {}", t("recipes.worlds"))
    }
}

fn proton_tag(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, tag)| tag)
}

fn recipe_name(r: &SavedReport) -> String {
    recipe_title(&r.report.recipe.id, &r.report.recipe.id)
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

// ---- drawing -----------------------------------------------------------

struct Series {
    v: Vec<f32>,
    color: Rgb,
    fill: bool,
    width: f32,
}

/// Lines over a faint grid, as SVG. Values run 0 to max.
fn plot(w: f32, h: f32, max: f32, series: &[Series], marks: &[usize]) -> String {
    let n = series.first().map_or(0, |s| s.v.len());
    let x = |i: usize| {
        if n > 1 {
            w * i as f32 / (n - 1) as f32
        } else {
            0.
        }
    };
    let y = |v: f32| (h - 3.) - (v / max).clamp(0., 1.) * (h - 6.);
    let mut s = format!(
        r#"<svg viewBox="0 0 {w} {h}" width="{w}" height="{h}" xmlns="http://www.w3.org/2000/svg">"#
    );
    for k in 0..=2 {
        let gy = y(max * k as f32 / 2.);
        s.push_str(&format!(
            r#"<line x1="0" x2="{w}" y1="{gy:.1}" y2="{gy:.1}" stroke="white" stroke-opacity="0.08"/>"#
        ));
    }
    for (k, sr) in series.iter().enumerate() {
        if sr.v.len() < 2 {
            continue;
        }
        let pts: Vec<String> =
            sr.v.iter()
                .enumerate()
                .map(|(i, v)| format!("{:.1} {:.1}", x(i), y(*v)))
                .collect();
        let line = format!("M {}", pts.join(" L "));
        let c = hex(sr.color);
        if sr.fill {
            s.push_str(&format!(
                r#"<defs><linearGradient id="g{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{c}" stop-opacity="0.28"/><stop offset="1" stop-color="{c}" stop-opacity="0"/></linearGradient></defs><path d="{line} L {w} {h} L 0 {h} Z" fill="url(#g{k})"/>"#
            ));
        }
        s.push_str(&format!(
            r#"<path d="{line}" fill="none" stroke="{c}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round"/>"#,
            sr.width
        ));
    }
    let red = hex(T.error);
    for i in marks {
        s.push_str(&format!(
            r#"<rect x="{:.1}" y="{}" width="2" height="7" rx="1" fill="{red}"/>"#,
            x(*i) - 1.,
            h - 7.
        ));
    }
    s.push_str("</svg>");
    s
}

fn svg(markup: String) -> SvgViewer {
    SvgViewer::new((markup.clone(), Bytes::from(markup.into_bytes()))).show_loader(false)
}

fn spark(tl: &Timeline) -> Element {
    let max = tl.points.iter().map(|p| p.fps).fold(1.0f32, f32::max);
    let (w, h) = (112., 30.);
    svg(plot(
        w,
        h,
        max,
        &[Series {
            v: tl.points.iter().map(|p| p.fps).collect(),
            color: T.accent,
            fill: false,
            width: 1.5,
        }],
        &[],
    ))
    .width(Size::px(w))
    .height(Size::px(h))
    .into_element()
}

fn nice_max(m: f32) -> f32 {
    let step = if m > 120. {
        40.
    } else if m > 60. {
        20.
    } else {
        10.
    };
    ((m / step).ceil() * step).max(step)
}

fn swatch(c: Rgb) -> Rect {
    rect()
        .width(Size::px(10.))
        .height(Size::px(10.))
        .corner_radius(5.)
        .background(c)
}

fn legend(items: &[(Rgb, &str)]) -> Element {
    rect()
        .horizontal()
        .spacing(space::L)
        .children(
            items
                .iter()
                .map(|(c, name)| {
                    rect()
                        .key(*name)
                        .horizontal()
                        .cross_align(Alignment::Center)
                        .spacing(space::S)
                        .child(swatch(*c))
                        .child(muted(*name))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        )
        .into_element()
}

pub(crate) fn tile(name: &str, value: impl Into<String>, tone: Option<Tone>) -> Element {
    rect()
        .width(Size::flex(1.))
        .padding(space::L)
        .spacing(space::XS)
        .corner_radius(RADIUS_CONTROL + 2.)
        .background(T.surface)
        .border(Border::new().fill(T.border).width(1.))
        .child(muted(name.to_string()))
        .child(
            label()
                .text(value.into())
                .font_size(size::TITLE)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(tone.map_or(T.text, Tone::color)),
        )
        .into_element()
}

pub(crate) fn tiles(items: Vec<Element>) -> Element {
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .spacing(space::M)
        .children(items)
        .into_element()
}

fn crash_chip(c: Option<Crash>) -> Option<Rect> {
    match c? {
        Crash::InGame => Some(chip(t("recipes.crash"), Tone::Error)),
        Crash::OnExit => Some(chip(t("recipes.crash.exit"), Tone::Neutral)),
    }
}

// ---- the tab -----------------------------------------------------------

#[derive(PartialEq)]
pub struct SessionsTab(pub Ui);

impl Component for SessionsTab {
    fn render(&self) -> impl IntoElement {
        let ui = self.0;
        let open = use_state(first_open);
        let reload = use_state(|| 0u32);
        let _ = *reload.read();
        if let Some(id) = open.read().clone() {
            return SessionDetail {
                ui,
                id,
                open,
                reload,
            }
            .into_element();
        }
        let list = svc().sessions();
        let mut on = use_state(|| svc().config().session_reports);
        let mut ui2 = ui;
        let setter = WritableSetter::new(on, move |v: bool| {
            let mut cfg = svc().config();
            cfg.session_reports = v;
            match svc().set_config(cfg) {
                Ok(()) => on.set(v),
                Err(e) => ui2.error.set(Some(e.to_string())),
            }
        });
        let recording = group(
            t("recipes.sessions.title"),
            vec![
                row(
                    t("recipes.sessions.record"),
                    Some(t("recipes.sessions.record.sub").into()),
                    Some(Switch::new(setter).into_element()),
                ),
                row(
                    t("recipes.sessions.last"),
                    Some(t("recipes.sessions.last.sub").into()),
                    Some(
                        Button::new()
                            .flat()
                            .compact()
                            .on_press(move |_| {
                                let mut reload = reload;
                                ui.run(async { svc().record_session(None) }, move |_| {
                                    reload.with_mut(|mut n| *n += 1)
                                })
                            })
                            .child(t("recipes.sessions.last.button"))
                            .into_element(),
                    ),
                ),
            ],
        );
        if list.is_empty() {
            return rect()
                .spacing(space::XL)
                .child(group(
                    t("recipes.sessions.saved"),
                    vec![row(
                        t("recipes.sessions.none"),
                        Some(t("recipes.sessions.none.sub").into()),
                        None,
                    )],
                ))
                .child(recording)
                .into_element();
        }

        let hours: f32 = list.iter().map(|s| s.report.duration_s as f32).sum::<f32>() / 3600.;
        let fps: Vec<f32> = list
            .iter()
            .filter_map(|s| s.report.perf.map(|p| p.avg_fps))
            .collect();
        let crashes = list
            .iter()
            .filter(|s| s.report.crash == Some(Crash::InGame))
            .count();
        let summary = tiles(vec![
            tile(t("ses.sum.count"), list.len().to_string(), None),
            tile(t("ses.sum.time"), format!("{hours:.1} h"), None),
            tile(
                t("ses.sum.fps"),
                if fps.is_empty() {
                    "–".to_string()
                } else {
                    format!("{:.0} fps", fps.iter().sum::<f32>() / fps.len() as f32)
                },
                None,
            ),
            tile(
                t("ses.sum.crash"),
                crashes.to_string(),
                (crashes > 0).then_some(Tone::Error),
            ),
        ]);

        let rows: Vec<Element> = list
            .iter()
            .map(|s| {
                let r = &s.report;
                let id = s.id.clone();
                let tl = svc().session_timeline(&s.id);
                let line1 = format!(
                    "{} {} {}, {}, {}",
                    recipe_name(s),
                    t("setup.on"),
                    proton_tag(&r.recipe.proton),
                    dur(r.duration_s),
                    worlds(r.worlds)
                );
                let line2 = match r.perf {
                    Some(p) => fill(
                        t("ses.avg_line"),
                        &[format!("{:.0}", p.avg_fps), format!("{:.0}", p.low1_fps)],
                    ),
                    None => t("ses.no_perf").to_string(),
                };
                let mut open = open;
                Button::new()
                    .key(s.id.clone())
                    .flat()
                    .expanded()
                    .on_press(move |_| open.set(Some(id.clone())))
                    .child(
                        rect()
                            .width(Size::fill())
                            .horizontal()
                            .content(Content::Flex)
                            .cross_align(Alignment::Center)
                            .spacing(space::M)
                            .child(
                                rect()
                                    .width(Size::flex(1.))
                                    .spacing(2.)
                                    .child(body(when(r.started_at)))
                                    .child(muted(line1))
                                    .child(muted(line2)),
                            )
                            .maybe_child(tl.as_ref().map(spark))
                            .maybe_child(r.flat.then(|| chip(t("recipes.desktop"), Tone::Neutral)))
                            .maybe_child(crash_chip(r.crash))
                            .child(label().text("›").font_size(size::LEAD).color(T.muted)),
                    )
                    .into_element()
            })
            .collect();

        rect()
            .spacing(space::XL)
            .child(summary)
            .child(group(t("recipes.sessions.saved"), rows))
            .child(recording)
            .into_element()
    }
}

// ---- one session -------------------------------------------------------

#[derive(PartialEq)]
struct SessionDetail {
    ui: Ui,
    id: String,
    open: State<Option<String>>,
    reload: State<u32>,
}

/// A chart with its axis labels, a cursor that follows the pointer and a
/// readout line under it.
#[allow(clippy::too_many_arguments)]
fn chart_card(
    heading: &str,
    sub: &str,
    key: &[(Rgb, &str)],
    width: State<f32>,
    hover: State<Option<usize>>,
    n: usize,
    step_s: f32,
    max: f32,
    unit: &str,
    series: Vec<Series>,
    marks: Vec<usize>,
    readout: impl Fn(usize) -> String,
) -> Element {
    const H: f32 = 150.;
    let w = (*width.read()).max(120.);
    let mut width_w = width;
    let mut hover_m = hover;
    let mut hover_l = hover;
    let at = *hover.read();
    let cursor_x = at.map(|i| (w * i as f32 / (n.max(2) - 1) as f32).clamp(0., w - 1.));
    let text = at.map(&readout).unwrap_or_else(|| " ".to_string());
    let total = clock(step_s * n as f32);
    rect()
        .width(Size::fill())
        .padding(space::L)
        .spacing(space::M)
        .corner_radius(RADIUS_CONTROL + 2.)
        .background(T.surface)
        .border(Border::new().fill(T.border).width(1.))
        .child(
            rect()
                .spacing(2.)
                .child(body(heading.to_string()).font_weight(FontWeight::SEMI_BOLD))
                .child(muted(sub.to_string())),
        )
        .child(legend(key))
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::Flex)
                .spacing(space::S)
                .child(
                    rect()
                        .width(Size::px(40.))
                        .height(Size::px(H))
                        .main_align(Alignment::SpaceBetween)
                        .cross_align(Alignment::End)
                        .child(muted(format!("{max:.0}")))
                        .child(muted(format!("0{unit}"))),
                )
                .child(
                    rect()
                        .width(Size::flex(1.))
                        .height(Size::px(H))
                        .on_sized(move |e: Event<SizedEventData>| {
                            let nw = e.area.width();
                            if (nw - *width_w.peek()).abs() > 1. {
                                width_w.set(nw);
                            }
                        })
                        .on_pointer_move(move |e: Event<PointerEventData>| {
                            let x = e.element_location().x as f32;
                            let i = ((x / w) * (n.max(2) - 1) as f32).round();
                            hover_m.set(Some((i.max(0.) as usize).min(n - 1)));
                        })
                        .on_pointer_leave(move |_| hover_l.set(None))
                        .child(
                            svg(plot(w, H, max, &series, &marks))
                                .width(Size::px(w))
                                .height(Size::px(H)),
                        )
                        .maybe_child(cursor_x.map(|x| {
                            rect()
                                .position(Position::new_absolute().top(0.).left(x))
                                .width(Size::px(1.))
                                .height(Size::px(H))
                                .background(alpha(T.text, 0.45))
                        })),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .padding((0., 0., 0., 48.))
                .horizontal()
                .main_align(Alignment::SpaceBetween)
                .child(muted("0:00"))
                .child(muted(clock(step_s * n as f32 / 2.)))
                .child(muted(total)),
        )
        .child(muted(text))
        .into_element()
}

impl Component for SessionDetail {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let mut open = self.open;
        let mut reload = self.reload;
        let width = use_state(|| 640.0f32);
        let hover = use_state(|| None::<usize>);
        let mut confirm = use_state(|| false);
        let Some(s) = svc().sessions().into_iter().find(|s| s.id == self.id) else {
            return rect()
                .child(
                    Button::new()
                        .flat()
                        .on_press(move |_| open.set(None))
                        .child(format!("← {}", t("ses.back"))),
                )
                .into_element();
        };
        let r = s.report.clone();
        let tl = svc().session_timeline(&s.id);
        let _ = ui;

        let crash_text = match r.crash {
            None => t("ses.crash.none"),
            Some(Crash::InGame) => t("recipes.crash"),
            Some(Crash::OnExit) => t("recipes.crash.exit"),
        };
        let head = rect()
            .width(Size::fill())
            .spacing(space::S)
            .child(
                Button::new()
                    .compact()
                    .flat()
                    .on_press(move |_| open.set(None))
                    .child(format!("← {}", t("ses.back"))),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(space::M)
                    .child(
                        rect()
                            .width(Size::flex(1.))
                            .spacing(space::XS)
                            .child(title(when(r.started_at)))
                            .child(muted(format!(
                                "{} {} {}",
                                recipe_name(&s),
                                t("setup.on"),
                                proton_tag(&r.recipe.proton)
                            ))),
                    )
                    .maybe_child(r.flat.then(|| chip(t("recipes.desktop"), Tone::Neutral)))
                    .maybe_child(crash_chip(r.crash)),
            );

        let stats = tiles(vec![
            tile(t("ses.t.duration"), dur(r.duration_s), None),
            tile(
                t("ses.t.avg"),
                r.perf
                    .map_or("–".to_string(), |p| format!("{:.0} fps", p.avg_fps)),
                None,
            ),
            tile(
                t("ses.t.low"),
                r.perf
                    .map_or("–".to_string(), |p| format!("{:.0} fps", p.low1_fps)),
                None,
            ),
            tile(
                t("ses.t.crash"),
                crash_text,
                match r.crash {
                    Some(Crash::InGame) => Some(Tone::Error),
                    _ => None,
                },
            ),
        ]);

        let charts: Element = match &tl {
            Some(tl) if tl.points.len() > 1 => {
                let n = tl.points.len();
                let max = nice_max(tl.points.iter().map(|p| p.fps).fold(1.0f32, f32::max));
                let fps_line: Vec<f32> = tl.points.iter().map(|p| p.fps).collect();
                let min_line: Vec<f32> = tl.points.iter().map(|p| p.fps_min).collect();
                let marks: Vec<usize> = tl
                    .points
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.hitches > 0)
                    .map(|(i, _)| i)
                    .collect();
                let step = tl.step_s;
                let pts = tl.points.clone();
                let pts2 = tl.points.clone();
                let fps_card = chart_card(
                    t("ses.chart.fps"),
                    t("ses.chart.fps.sub"),
                    &[
                        (T.accent, t("ses.lg.avg")),
                        (T.glow, t("ses.lg.min")),
                        (T.error, t("ses.lg.freeze")),
                    ],
                    width,
                    hover,
                    n,
                    step,
                    max,
                    "",
                    vec![
                        Series {
                            v: min_line,
                            color: T.glow,
                            fill: false,
                            width: 1.0,
                        },
                        Series {
                            v: fps_line,
                            color: T.accent,
                            fill: true,
                            width: 2.0,
                        },
                    ],
                    marks,
                    move |i| {
                        fill(
                            t("ses.read.fps"),
                            &[
                                clock(step * i as f32),
                                format!("{:.0}", pts[i].fps),
                                format!("{:.0}", pts[i].fps_min),
                            ],
                        )
                    },
                );
                let load_card = tl.has_load.then(|| {
                    chart_card(
                        t("ses.chart.load"),
                        t("ses.chart.load.sub"),
                        &[(T.accent, t("ses.lg.gpu")), (T.warn, t("ses.lg.cpu"))],
                        width,
                        hover,
                        n,
                        step,
                        100.,
                        " %",
                        vec![
                            Series {
                                v: tl.points.iter().map(|p| p.cpu).collect(),
                                color: T.warn,
                                fill: false,
                                width: 1.5,
                            },
                            Series {
                                v: tl.points.iter().map(|p| p.gpu).collect(),
                                color: T.accent,
                                fill: false,
                                width: 1.5,
                            },
                        ],
                        vec![],
                        move |i| {
                            fill(
                                t("ses.read.load"),
                                &[
                                    clock(step * i as f32),
                                    format!("{:.0}", pts2[i].gpu),
                                    format!("{:.0}", pts2[i].cpu),
                                ],
                            )
                        },
                    )
                });
                // MangoHud only sees what the game draws to its window: in
                // the headset alone the log has holes, so say how much it saw.
                let seen = tl.duration_s();
                let partial = (seen < 0.8 * r.duration_s as f32).then(|| {
                    let seen = if seen < 60. {
                        format!("{seen:.0} s")
                    } else {
                        dur(seen as u64)
                    };
                    banner(
                        Tone::Warn,
                        t("ses.partial"),
                        fill(t("ses.partial.sub"), &[seen, dur(r.duration_s)]),
                        None,
                    )
                });
                rect()
                    .width(Size::fill())
                    .spacing(space::L)
                    .maybe_child(partial)
                    .child(fps_card)
                    .maybe_child(load_card)
                    .into_element()
            }
            _ => group(
                t("ses.chart.fps"),
                vec![row(
                    t("ses.no_chart"),
                    Some(t("ses.no_chart.sub").into()),
                    None,
                )],
            )
            .into_element(),
        };

        // What slowed the game down, in words.
        let mut slow: Vec<Element> = Vec::new();
        if let Some(p) = r.perf {
            if let Some(d) = p.dips {
                slow.push(row(
                    t("ses.dips"),
                    Some(fill(
                        t("ses.dips.sub"),
                        &[
                            d.episodes.to_string(),
                            format!("{:.0}", d.seconds),
                            format!("{:.0}", d.fps),
                        ],
                    )),
                    None,
                ));
                let cause = match d.cause {
                    DipCause::GpuBound => t("ses.cause.gpu_bound"),
                    DipCause::GpuClock => t("ses.cause.gpu_clock"),
                    DipCause::GpuWaiting => t("ses.cause.gpu_waiting"),
                    DipCause::Unknown => t("ses.cause.unknown"),
                };
                let mut sub = cause.to_string();
                if let Some([normal, dip]) = d.gpu_load {
                    sub.push('\n');
                    sub.push_str(&fill(
                        t("ses.dips.gpu"),
                        &[format!("{normal:.0}"), format!("{dip:.0}")],
                    ));
                }
                slow.push(row(t("ses.cause"), Some(sub), None));
            }
            if let Some(h) = p.hitches {
                let mut sub = fill(
                    t("ses.hitch.sub"),
                    &[
                        h.count.to_string(),
                        format!("{HITCH_MS:.0}"),
                        format!("{:.0}", h.longest_ms),
                    ],
                );
                if let Some(period) = h.period_s {
                    sub.push('\n');
                    sub.push_str(&fill(t("ses.hitch.period"), &[format!("{period:.0}")]));
                }
                slow.push(row(t("ses.hitch"), Some(sub), None));
            }
            if p.frames > 0 {
                slow.push(row(
                    t("ses.stutter"),
                    Some(fill(t("ses.stutter.sub"), &[p.stutters.to_string()])),
                    None,
                ));
                slow.push(row(
                    t("ses.p99"),
                    Some(fill(
                        t("ses.p99.sub"),
                        &[format!("{:.1}", p.p99_frametime_ms)],
                    )),
                    None,
                ));
            }
        }
        if slow.is_empty() {
            slow.push(row(t("ses.no_perf"), None, None));
        }

        let played = vec![
            row(
                t("ses.g.worlds"),
                None,
                Some(body(r.worlds.to_string()).into_element()),
            ),
            row(
                t("ses.g.joins"),
                None,
                Some(body(r.joins.to_string()).into_element()),
            ),
            row(
                t("ses.g.videos"),
                Some(t("ses.g.videos.sub").into()),
                Some(
                    body(format!(
                        "{}/{}",
                        r.video.opened - r.video.failed.min(r.video.opened),
                        r.video.opened
                    ))
                    .into_element(),
                ),
            ),
            row(
                t("ses.g.errors"),
                Some(t("ses.g.errors.sub").into()),
                Some(body(r.exceptions.to_string()).into_element()),
            ),
        ];

        let hw = &r.hardware;
        let machine = vec![
            row(t("ses.hw.gpu"), Some(hw.gpu.clone()), None),
            row(t("ses.hw.driver"), Some(hw.gpu_driver.clone()), None),
            row(t("ses.hw.cpu"), Some(hw.cpu.clone()), None),
            row(
                t("ses.hw.ram"),
                Some(fill(t("ses.hw.ram.v"), &[hw.ram_gb.to_string()])),
                None,
            ),
            row(t("ses.hw.os"), Some(hw.distro.clone()), None),
            row(t("ses.hw.kernel"), Some(hw.kernel.clone()), None),
            row(t("ses.hw.vr"), Some(hw.vr_runtime.clone()), None),
        ];

        let id = s.id.clone();
        let delete = if *confirm.read() {
            Button::new()
                .filled()
                .on_press(move |_| {
                    if svc().session_delete(&id).is_ok() {
                        reload.with_mut(|mut n| *n += 1);
                        open.set(None);
                    }
                })
                .child(label().text(t("ses.delete.confirm")).color(T.accent_text))
                .into_element()
        } else {
            Button::new()
                .flat()
                .on_press(move |_| confirm.set(true))
                .child(t("ses.delete"))
                .into_element()
        };

        rect()
            .width(Size::fill())
            .spacing(space::XL)
            .child(head)
            .child(stats)
            .child(charts)
            .child(group(t("ses.slow"), slow))
            .child(group(t("ses.g.title"), played))
            .child(group(t("ses.hw"), machine))
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(space::M)
                    .child(
                        rect()
                            .width(Size::flex(1.))
                            .child(muted(t("ses.delete.sub"))),
                    )
                    .child(delete),
            )
            .into_element()
    }
}

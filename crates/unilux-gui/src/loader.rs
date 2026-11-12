//! The install loader: a breathing glow behind the logo, two gradient
//! arcs turning in opposite directions and three dots on a slow orbit.

use freya::animation::{AnimNum, Ease, Function, OnCreation, OnFinish, use_animation};
use std::sync::LazyLock;

use freya::prelude::*;

use crate::app;
use crate::theme::{T, alpha};

const SIZE: f32 = 168.;

fn hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

/// An arc of `sweep` degrees on a circle of radius `r` that fades out
/// along its length.
fn arc_svg(r: f32, width: f32, sweep: f32, color: &str) -> String {
    let c = SIZE / 2.;
    let a = sweep.to_radians();
    let (x, y) = (c + r * a.cos(), c + r * a.sin());
    let large = if sweep > 180. { 1 } else { 0 };
    format!(
        r#"<svg viewBox="0 0 {SIZE} {SIZE}" xmlns="http://www.w3.org/2000/svg">
<defs><linearGradient id="g" x1="0" y1="1" x2="1" y2="0">
<stop offset="0" stop-color="{color}" stop-opacity="0"/>
<stop offset="1" stop-color="{color}" stop-opacity="1"/></linearGradient></defs>
<path d="M {} {c} A {r} {r} 0 {large} 1 {x:.2} {y:.2}" fill="none" stroke="url(#g)" stroke-width="{width}" stroke-linecap="round"/>
</svg>"#,
        c + r
    )
}

fn dots_svg(r: f32, color: &str) -> String {
    let c = SIZE / 2.;
    let dot = |deg: f32, rad: f32, op: f32| {
        let a = deg.to_radians();
        format!(
            r#"<circle cx="{:.2}" cy="{:.2}" r="{rad}" fill="{color}" fill-opacity="{op}"/>"#,
            c + r * a.cos(),
            c + r * a.sin()
        )
    };
    format!(
        r#"<svg viewBox="0 0 {SIZE} {SIZE}" xmlns="http://www.w3.org/2000/svg">{}{}{}</svg>"#,
        dot(0., 3.5, 1.0),
        dot(120., 2.5, 0.7),
        dot(240., 2., 0.45)
    )
}

fn track_svg(r: f32) -> String {
    let c = SIZE / 2.;
    format!(
        r#"<svg viewBox="0 0 {SIZE} {SIZE}" xmlns="http://www.w3.org/2000/svg"><circle cx="{c}" cy="{c}" r="{r}" fill="none" stroke="white" stroke-opacity="0.07" stroke-width="2"/></svg>"#
    )
}

fn layer() -> Rect {
    rect()
        .position(Position::new_absolute().top(0.).left(0.))
        .width(Size::px(SIZE))
        .height(Size::px(SIZE))
        .center()
}

static TRACK_OUTER: LazyLock<String> = LazyLock::new(|| track_svg(72.));
static TRACK_INNER: LazyLock<String> = LazyLock::new(|| track_svg(58.));
static ARC_OUTER: LazyLock<String> = LazyLock::new(|| arc_svg(72., 4., 250., &hex(T.accent)));
static ARC_INNER: LazyLock<String> = LazyLock::new(|| arc_svg(58., 3., 160., &hex(T.glow)));
static DOTS: LazyLock<String> = LazyLock::new(|| dots_svg(80., &hex(T.text)));

fn svg(src: &'static LazyLock<String>) -> SvgViewer {
    let s: &'static str = src.as_str();
    SvgViewer::new(s.as_bytes())
        .show_loader(false)
        .width(Size::px(SIZE))
        .height(Size::px(SIZE))
}

#[derive(PartialEq)]
pub struct OrbitLoader;

impl Component for OrbitLoader {
    fn render(&self) -> impl IntoElement {
        let spin = |ms: u64, from: f32, to: f32| {
            move |conf: &mut freya::animation::AnimConfiguration| {
                conf.on_creation(OnCreation::Run);
                conf.on_finish(OnFinish::restart());
                AnimNum::new(from, to).time(ms).function(Function::Linear)
            }
        };
        let outer = use_animation(spin(1400, 0., 360.));
        let inner = use_animation(spin(2300, 360., 0.));
        let orbit = use_animation(spin(5200, 0., 360.));
        let breathe = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            conf.on_finish(OnFinish::reverse());
            AnimNum::new(0., 1.)
                .time(1600)
                .function(Function::Sine)
                .ease(Ease::InOut)
        });
        let b = breathe.get().value();
        rect()
            .width(Size::px(SIZE))
            .height(Size::px(SIZE))
            .child(
                layer().child(
                    rect()
                        .width(Size::px(92.))
                        .height(Size::px(92.))
                        .corner_radius(46.)
                        .background(alpha(T.accent, 0.10 + 0.08 * b))
                        .shadow(
                            Shadow::new()
                                .blur(36. + 20. * b)
                                .spread(4. + 6. * b)
                                .color(alpha(T.accent, 0.28 + 0.2 * b)),
                        ),
                ),
            )
            .child(layer().child(svg(&TRACK_OUTER)))
            .child(layer().child(svg(&TRACK_INNER)))
            .child(layer().child(svg(&ARC_OUTER).rotation(outer.get().value())))
            .child(layer().child(svg(&ARC_INNER).rotation(inner.get().value())))
            .child(layer().child(svg(&DOTS).rotation(orbit.get().value())))
            .child(layer().child(rect().opacity(0.75 + 0.25 * b).child(app::logo(52.))))
    }
}

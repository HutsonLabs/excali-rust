//! The walk every backend shares: groups flattened into absolute state.

use super::image::ImageItem;
use super::paint::{Color, Rgba, Stroke};
use super::path::{FillRule, Path};
use super::text::TextRun;
use super::transform::Transform;
use super::{Clip, DisplayItem, DisplayList};

/// The absolute drawing state of one draw call: the matrix and the
/// `globalAlpha` it is drawn with, and the context's current `fillStyle`
/// and `strokeStyle`, which a draw whose colour the canvas ignores paints
/// in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintState {
    pub transform: Transform,
    /// `0..=1`, multiplied into every pixel the draw produces.
    pub alpha: f64,
    /// The `fillStyle` in effect before the draw assigns its own.
    pub fill_style: Rgba,
    /// The `strokeStyle` in effect before the draw assigns its own.
    pub stroke_style: Rgba,
}

impl PaintState {
    /// A fresh context: identity matrix, alpha 1, black fill and stroke
    /// styles.
    pub const ROOT: PaintState = PaintState {
        transform: Transform::IDENTITY,
        alpha: 1.0,
        fill_style: Rgba::BLACK,
        stroke_style: Rgba::BLACK,
    };

    /// The root state with `transform` and `alpha`.
    pub const fn new(transform: Transform, alpha: f64) -> Self {
        Self {
            transform,
            alpha,
            ..Self::ROOT
        }
    }
}

impl Default for PaintState {
    fn default() -> Self {
        Self::ROOT
    }
}

/// What a backend implements to consume a [`DisplayList`].
///
/// [`DisplayList::replay`] calls it once per draw, in list order, with the
/// state already resolved: the matrix is the product of every enclosing
/// group's, the alpha their product, and the colour parsed as the canvas
/// parses it ([`Color::rgba`]). A colour the canvas would ignore resolves to
/// the context's current style ([`PaintState::fill_style`] for fills and
/// text, [`PaintState::stroke_style`] for strokes): black from a fresh
/// context, as upstream's element canvases are. Clips arrive as a stack:
/// every draw between `push_clip` and the matching `pop_clip` is clipped by
/// the intersection of all clips pushed and not yet popped, each in the
/// matrix given with it.
///
/// The opacity rule is the canvas's `globalAlpha`: each draw is blended on
/// its own with its alpha, so overlapping draws in one group compound. It
/// is not group compositing.
pub trait Painter {
    /// Fill `path` with the item's `color`, resolved as `rgba`, under `rule`.
    fn fill(&mut self, path: &Path, color: &Color, rgba: Rgba, rule: FillRule, state: &PaintState);
    /// Stroke `path` with `stroke` (`stroke.color` resolved as `rgba`).
    fn stroke(&mut self, path: &Path, stroke: &Stroke, rgba: Rgba, state: &PaintState);
    /// Draw a bitmap.
    fn image(&mut self, image: &ImageItem, state: &PaintState);
    /// Draw a line of text (`run.color` resolved as `rgba`).
    fn text(&mut self, run: &TextRun, rgba: Rgba, state: &PaintState);
    /// Intersect the clip with `clip.path` under `clip.rule`, in `transform`.
    fn push_clip(&mut self, clip: &Clip, transform: &Transform);
    /// Undo the matching `push_clip`.
    fn pop_clip(&mut self);
}

impl DisplayList {
    /// Replay the list into `painter` from a fresh context.
    pub fn replay(&self, painter: &mut impl Painter) {
        self.replay_from(painter, PaintState::ROOT);
    }

    /// Replay the list into `painter` from `base`, such as the device pixel
    /// ratio scale `bootstrapCanvas` sets (`renderer/helpers.ts:73-127`) or
    /// the `fillStyle` it leaves after painting the background.
    pub fn replay_from(&self, painter: &mut impl Painter, base: PaintState) {
        replay_items(&self.items, painter, &base);
    }
}

fn replay_items(items: &[DisplayItem], painter: &mut impl Painter, state: &PaintState) {
    for item in items {
        match item {
            // An assignment the canvas ignores keeps the current style.
            DisplayItem::Fill { path, color, rule } => {
                let rgba = color.rgba().unwrap_or(state.fill_style);
                painter.fill(path, color, rgba, *rule, state);
            }
            DisplayItem::Stroke { path, stroke } => {
                let rgba = stroke.color.rgba().unwrap_or(state.stroke_style);
                painter.stroke(path, stroke, rgba, state);
            }
            DisplayItem::Image(image) => painter.image(image, state),
            DisplayItem::Text(run) => {
                let rgba = run.color.rgba().unwrap_or(state.fill_style);
                painter.text(run, rgba, state);
            }
            DisplayItem::Group(group) => {
                // `transform(…)` with an infinite or NaN argument is ignored,
                // and `globalAlpha = x` unless 0 <= x <= 1.
                let t = &group.transform;
                let finite = [t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite());
                let alpha = state.alpha * group.opacity;
                let inner = PaintState {
                    fill_style: state.fill_style,
                    stroke_style: state.stroke_style,
                    transform: if finite {
                        state.transform.concat(t)
                    } else {
                        state.transform
                    },
                    alpha: if (0.0..=1.0).contains(&alpha) {
                        alpha
                    } else {
                        state.alpha
                    },
                };
                if let Some(clip) = &group.clip {
                    painter.push_clip(clip, &inner.transform);
                    replay_items(&group.items, painter, &inner);
                    painter.pop_clip();
                } else {
                    replay_items(&group.items, painter, &inner);
                }
            }
        }
    }
}

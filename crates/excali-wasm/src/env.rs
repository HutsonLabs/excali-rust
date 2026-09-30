//! The editor's environment: ids, random integers and the clock, and the
//! text metrics the leaf layouts measure with.
//!
//! [`EditorEnv`] is every environment the editor's crates ask for:
//!
//! - [`RestoreEnv`] for loading (`restoreElements`, `restoreAppState`),
//!   wrapped in `excali_editor::restore_env::RoutingEnv` by the loader;
//! - [`MutationEnv`] and [`BindingEnv`] for `scene.mutateElement` and
//!   `updateBoundElements` (keyboard nudges, dragging);
//! - [`ChangeStamp`] and [`HistoryEnv`] for the store and history, with the
//!   real leaf layouts: `redrawTextBoundingBox` is
//!   [`TextLayouter::redraw_text_bounding_box`] (ex-512), a sticky note's
//!   arrows following it through `updateBoundElements`, and
//!   `updateBoundElements` is the trait's default,
//!   [`excali_editor::binding::update_bound_elements_in_map`] (ex-510).
//!
//! Upstream draws ids from nanoid and integers from roughjs' `Random`
//! seeded per session (`packages/common/src/random.ts`); the port draws
//! both from a splitmix64 generator seeded by the host, and the time from a
//! clock the host gives (`Date.now` in the browser).

use excali_core::element::Element;
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_core::restore::{
    RestoreEnv, StickyNoteLayout as RestoredStickyNoteLayout, StickyNoteLayoutRequest,
};
use excali_editor::binding::{update_bound_elements_in_map, BindingEnv};
use excali_editor::resize_elements::{
    sticky_note_layout, StickyNoteLayout, StickyNoteLayoutOpts, TransformEnv,
};
use excali_editor::restore_env;
use excali_editor::scene::{MutationEnv, Scene};
use excali_editor::stats::StatsEnv;
use excali_editor::store::HistoryEnv;
use excali_editor::text_layout::TextLayouter;
use excali_scene::sticky_note::{utc, Clock};
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};

/// nanoid's alphabet (`nanoid/url-alphabet`), which `randomId` draws from.
const URL_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

/// The generator and the clock: `randomId`, `randomInteger` and
/// `getUpdatedTimestamp`.
#[derive(Clone, Copy, Debug)]
pub struct Rng {
    state: u64,
    clock: fn() -> f64,
}

impl Rng {
    /// splitmix64.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `randomInteger()`: an integer in `0..2^31`.
    fn integer(&mut self) -> f64 {
        (self.next() >> 33) as f64
    }

    /// `randomId()`: 21 characters of nanoid's alphabet.
    fn id(&mut self) -> String {
        (0..21)
            .map(|_| char::from(URL_ALPHABET[(self.next() >> 58) as usize]))
            .collect()
    }

    fn now(&self) -> f64 {
        (self.clock)()
    }
}

impl ChangeStamp for Rng {
    fn version_nonce(&mut self) -> f64 {
        self.integer()
    }

    fn updated(&mut self) -> f64 {
        self.now()
    }
}

/// The editor's environment over the text metrics `P`.
pub struct EditorEnv<P> {
    /// The bound text layout, its character width cache and the original
    /// container heights text editing shares.
    pub layouter: TextLayouter<P>,
    rng: Rng,
    /// Minutes east of UTC at a time: the viewer's time zone, which sticky
    /// note footers are dated in (UTC unless the host sets it).
    utc_offset_minutes: fn(f64) -> f64,
}

impl<P: TextMetricsProvider + Clone> EditorEnv<P> {
    /// An environment measuring with `provider`, its generator seeded with
    /// `seed`, its time read from `clock` (epoch milliseconds).
    pub fn new(provider: P, seed: u64, clock: fn() -> f64) -> EditorEnv<P> {
        EditorEnv {
            layouter: TextLayouter::new(provider),
            rng: Rng { state: seed, clock },
            utc_offset_minutes: utc,
        }
    }

    /// The same environment dated in the time zone `utc_offset_minutes`
    /// gives (`-new Date(t).getTimezoneOffset()` in the browser).
    pub fn with_time_zone(mut self, utc_offset_minutes: fn(f64) -> f64) -> EditorEnv<P> {
        self.utc_offset_minutes = utc_offset_minutes;
        self
    }

    /// `Date.now()` and the time zone, as the renderers read them.
    pub fn render_clock(&self) -> Clock {
        Clock {
            now: self.rng.now(),
            utc_offset_minutes: self.utc_offset_minutes,
        }
    }
}

impl<P: TextMetricsProvider + Clone> RestoreEnv for EditorEnv<P> {
    fn now(&mut self) -> f64 {
        self.rng.now()
    }

    fn random_id(&mut self) -> String {
        self.rng.id()
    }

    fn random_integer(&mut self) -> f64 {
        self.rng.integer()
    }

    /// `getStickyNoteLayout` with the editor's layouter
    /// ([`excali_editor::restore_env::sticky_note_layout`]).
    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<RestoredStickyNoteLayout> {
        self.layouter
            .with_layout(|layout, _| restore_env::sticky_note_layout(layout, &request))
    }
}

impl<P: TextMetricsProvider + Clone> MutationEnv for EditorEnv<P> {
    fn random_integer(&mut self) -> f64 {
        self.rng.integer()
    }

    fn now(&mut self) -> f64 {
        self.rng.now()
    }
}

impl<P: TextMetricsProvider + Clone> BindingEnv for EditorEnv<P> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&self.layouter.provider, &mut self.layouter.char_widths)
    }
}

impl<P: TextMetricsProvider + Clone> ChangeStamp for EditorEnv<P> {
    fn version_nonce(&mut self) -> f64 {
        self.rng.integer()
    }

    fn updated(&mut self) -> f64 {
        self.rng.now()
    }
}

/// A sticky note's arrows, laid out while the layouter is busy: the
/// layout's version stamp and a copy of its metrics.
pub(crate) struct StampBinding<'a, P> {
    pub(crate) stamp: &'a mut dyn ChangeStamp,
    pub(crate) provider: &'a P,
    pub(crate) char_widths: &'a mut CharWidthCache,
}

impl<P> MutationEnv for StampBinding<'_, P> {
    fn random_integer(&mut self) -> f64 {
        self.stamp.version_nonce()
    }

    fn now(&mut self) -> f64 {
        self.stamp.updated()
    }
}

impl<P: TextMetricsProvider> BindingEnv for StampBinding<'_, P> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (self.provider, self.char_widths)
    }
}

impl<P: TextMetricsProvider + Clone> HistoryEnv for EditorEnv<P> {
    fn random_id(&mut self) -> String {
        self.rng.id()
    }

    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: &str,
    ) -> Result<(), String> {
        self.redraw_text(elements, text_id, Some(container_id))
    }

    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&self.layouter.provider, &mut self.layouter.char_widths)
    }

    /// Debug builds fail an undo whose layout fails, as upstream's
    /// development build does; release builds carry on.
    fn dev_checks(&self) -> bool {
        cfg!(debug_assertions)
    }
}

/// Resizing and rotating: the bound arrows follow through the trait's
/// default, a sticky note's label is laid out by `getStickyNoteLayout`.
impl<P: TextMetricsProvider + Clone> TransformEnv for EditorEnv<P> {
    fn sticky_note_layout(
        &mut self,
        container: &Element,
        text: Option<&Element>,
        opts: &StickyNoteLayoutOpts,
    ) -> StickyNoteLayout {
        self.layouter
            .with_layout(|layout, _| sticky_note_layout(layout, container, text, opts))
    }
}

impl<P: TextMetricsProvider + Clone> EditorEnv<P> {
    /// `redrawTextBoundingBox(text, container, scene)` on the elements;
    /// the arrows a sticky note's layout moves follow it.
    fn redraw_text(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: Option<&str>,
    ) -> Result<(), String> {
        let provider = self.layouter.provider.clone();
        let mut arrow_widths = CharWidthCache::new();
        self.layouter.redraw_text_bounding_box(
            &mut self.rng,
            elements,
            text_id,
            container_id,
            &mut |stamp, elements, id| {
                let mut env = StampBinding {
                    stamp,
                    provider: &provider,
                    char_widths: &mut arrow_widths,
                };
                update_bound_elements_in_map(elements, id, &SceneElementsMap::new(), &mut env);
                Ok(())
            },
        )
    }
}

/// The stats panel's font size: a free text or a label laid out again.
impl<P: TextMetricsProvider + Clone> StatsEnv for EditorEnv<P> {
    fn redraw_text_bounding_box(
        &mut self,
        scene: &mut Scene,
        text_id: &str,
        container_id: Option<&str>,
    ) {
        let mut map: SceneElementsMap = scene
            .elements()
            .iter()
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect();
        if self.redraw_text(&mut map, text_id, container_id).is_ok() {
            *scene = Scene::new(map.into_values().collect());
        }
    }
}

//! A [`RestoreEnv`] that refits text for `restoreElements`.
//!
//! `restoreElements` with `refreshDimensions` refits every text but a
//! sticky label to its text and container
//! (`refreshTextDimensions`, `packages/excalidraw/data/restore.ts:1032-1045`);
//! `excali-core` asks its environment for that
//! ([`RestoreEnv::refresh_text_dimensions`]) because wrapping and
//! measurement live here. [`TextEnv`] wraps another environment, answers
//! that request with [`refresh_text_dimensions`] and hands every other
//! request to the wrapped one, so it composes with `excali-editor`'s
//! `RoutingEnv` either way round.

use excali_core::element::Element;
use excali_core::restore::{
    ElbowArrowRequest, LegacyBinding, LegacyBindingRequest, RestoreEnv, StickyNoteLayout,
    StickyNoteLayoutRequest, TextDimensionsRequest,
};
use serde_json::{Map, Value};

use crate::new_element::{refresh_text_dimensions, TextLayout};
use crate::text_element::{ArrowLabelGeometry, NoArrowGeometry};
use crate::text_measurements::{CharWidthCache, TextMetricsProvider};

/// `inner` with text measurement: `provider` measures lines, `geometry`
/// places arrow labels, and `char_widths` is the per-character width cache
/// wrapping fills (upstream's module-global `charWidth`).
#[derive(Debug, Clone, Default)]
pub struct TextEnv<E, P, G = NoArrowGeometry> {
    pub inner: E,
    pub provider: P,
    pub geometry: G,
    pub char_widths: CharWidthCache,
}

impl<E: RestoreEnv, P: TextMetricsProvider> TextEnv<E, P> {
    /// Without arrow geometry: an arrow label is left as restored.
    pub fn new(inner: E, provider: P) -> TextEnv<E, P> {
        TextEnv::with_geometry(inner, provider, NoArrowGeometry)
    }
}

impl<E: RestoreEnv, P: TextMetricsProvider, G: ArrowLabelGeometry> TextEnv<E, P, G> {
    pub fn with_geometry(inner: E, provider: P, geometry: G) -> TextEnv<E, P, G> {
        TextEnv {
            inner,
            provider,
            geometry,
            char_widths: CharWidthCache::new(),
        }
    }
}

fn read(element: &Map<String, Value>) -> Option<Element> {
    Element::from_restored(element.clone()).ok()
}

impl<E: RestoreEnv, P: TextMetricsProvider, G: ArrowLabelGeometry> RestoreEnv for TextEnv<E, P, G> {
    fn now(&mut self) -> f64 {
        self.inner.now()
    }

    fn random_id(&mut self) -> String {
        self.inner.random_id()
    }

    fn random_integer(&mut self) -> f64 {
        self.inner.random_integer()
    }

    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        self.inner.migrate_legacy_binding(request)
    }

    /// `refreshTextDimensions(text, container, restoredElementsMap)`: the
    /// text, its container and the restored elements read into the element
    /// model, refitted by [`refresh_text_dimensions`].
    ///
    /// `None`, which leaves the text as restored, for a deleted text (as
    /// upstream), a text or container the model cannot read, and an arrow
    /// label the geometry cannot place.
    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let text = read(request.text)?;
        let container = match request.container {
            Some(container) => Some(read(container)?),
            None => None,
        };
        let elements: Vec<Element> = request.elements.iter().filter_map(read).collect();
        let mut layout = TextLayout {
            provider: &self.provider,
            char_widths: &mut self.char_widths,
            geometry: &mut self.geometry,
        };
        let refreshed = refresh_text_dimensions(
            &mut layout,
            &text,
            container.as_ref(),
            &elements,
            None,
            None,
        )?;
        Some(refreshed.to_map())
    }

    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        self.inner.sticky_note_layout(request)
    }

    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.inner.update_elbow_arrow_points(request)
    }
}

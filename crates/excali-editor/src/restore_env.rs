//! A [`RestoreEnv`] that re-routes elbow arrows for `restoreElements`.
//!
//! `restoreElements` with `repairBindings` re-routes an unbound elbow arrow
//! whose segments are not all axis-aligned from `[0, 0]` to its last point
//! (`restore.ts:1076-1093`); `excali-core` asks its environment for that
//! route ([`RestoreEnv::update_elbow_arrow_points`]) because the router
//! lives here. [`RoutingEnv`] wraps another environment, answers that
//! request with [`update_elbow_arrow_points`] and hands every other
//! request to the wrapped one.

use excali_core::element::Element;
use excali_core::restore::{
    ElbowArrowRequest, LegacyBinding, LegacyBindingRequest, RestoreEnv, StickyNoteLayout,
    StickyNoteLayoutRequest, TextDimensionsRequest,
};
use serde_json::{Map, Value};

use crate::elbow_arrow::{update_elbow_arrow_points, ElbowArrowUpdates, ElementsMap};

/// `inner` with the elbow arrow router.
#[derive(Debug, Clone, Default)]
pub struct RoutingEnv<E> {
    pub inner: E,
}

impl<E: RestoreEnv> RoutingEnv<E> {
    pub fn new(inner: E) -> RoutingEnv<E> {
        RoutingEnv { inner }
    }
}

fn point(value: &Value) -> Option<[f64; 2]> {
    let items = value.as_array()?;
    Some([items.first()?.as_f64()?, items.get(1)?.as_f64()?])
}

impl<E: RestoreEnv> RestoreEnv for RoutingEnv<E> {
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

    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.inner.refresh_text_dimensions(request)
    }

    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        self.inner.sticky_note_layout(request)
    }

    /// `updateElbowArrowPoints(arrow, restoredElementsMap, {points})`: the
    /// arrow and the restored elements read into the element model (an
    /// element it cannot read still counts towards the map's size but is
    /// no binding target), routed by [`update_elbow_arrow_points`].
    ///
    /// `None`, which keeps the arrow as restored, when the arrow or its
    /// points cannot be read, or where upstream's router would throw (and
    /// `restoreElements` with it).
    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let arrow = Element::from_map(request.arrow.clone()).ok()?;
        let points = request
            .points
            .iter()
            .map(point)
            .collect::<Option<Vec<[f64; 2]>>>()?;
        let elements: Vec<Element> = request
            .elements
            .iter()
            .filter_map(|e| Element::from_map(e.clone()).ok())
            .collect();
        let mut ids: Vec<&Value> = request
            .elements
            .iter()
            .filter_map(|e| e.get("id"))
            .collect();
        ids.sort_by_key(|id| id.to_string());
        ids.dedup();
        let map = ElementsMap::new(&elements).with_size(ids.len());
        let update = update_elbow_arrow_points(
            &arrow,
            &map,
            &ElbowArrowUpdates {
                points: Some(points),
                ..ElbowArrowUpdates::default()
            },
        )
        .ok()?;
        Some(update.to_map())
    }
}

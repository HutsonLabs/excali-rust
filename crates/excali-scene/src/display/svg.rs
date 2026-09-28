//! The drawing of an SVG export: the nodes upstream's `renderSceneToSvg`
//! (`packages/excalidraw/renderer/staticSvgScene.ts:850-933`) adds to the
//! document `exportToSvg` builds.
//!
//! The canvas backends paint a [`DisplayList`](super::DisplayList); an SVG
//! export is markup whose structure upstream's own consumers read back
//! (one `<g>` per element, `<text>` per line, `<symbol>` and `<use>` per
//! image, `<mask>` per arrow label), so the scene hands the SVG backend
//! that structure: tags with their attributes in the order upstream sets
//! them, and text. The backend prints each value ([`SvgValue`]) and
//! serializes the tree as `outerHTML` does. Nothing here names an element.

use super::path::Path;

/// A node of the drawing: a tag or a text node.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgNode {
    Tag(SvgTag),
    /// A text node's data (`textContent`), unescaped.
    Text(String),
}

impl From<SvgTag> for SvgNode {
    fn from(tag: SvgTag) -> SvgNode {
        SvgNode::Tag(tag)
    }
}

/// A tag: `createElementNS(SVG_NS, name)` (or, for the one HTML element an
/// export makes, the `<iframe>` of an embeddable, `createElement`), its
/// attributes in the order they were first set, and its children.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgTag {
    pub name: &'static str,
    pub attributes: Vec<(&'static str, SvgValue)>,
    pub children: Vec<SvgNode>,
}

impl SvgTag {
    /// A tag with no attributes and no children.
    pub fn new(name: &'static str) -> SvgTag {
        SvgTag {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// `setAttribute(name, value)`: a new attribute goes last, an existing
    /// one keeps its place and takes the new value.
    pub fn set(&mut self, name: &'static str, value: impl Into<SvgValue>) {
        let value = value.into();
        match self.attributes.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => *v = value,
            None => self.attributes.push((name, value)),
        }
    }

    /// The attribute's value, if set.
    pub fn get(&self, name: &str) -> Option<&SvgValue> {
        self.attributes
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v)
    }

    /// `appendChild(node)`.
    pub fn append(&mut self, node: impl Into<SvgNode>) {
        self.children.push(node.into());
    }

    /// The tag with an attribute set, for building trees.
    pub fn with(mut self, name: &'static str, value: impl Into<SvgValue>) -> SvgTag {
        self.set(name, value);
        self
    }

    /// The tag with a child appended.
    pub fn child(mut self, node: impl Into<SvgNode>) -> SvgTag {
        self.append(node);
        self
    }
}

/// An attribute value, as upstream computes it before it becomes a string.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgValue {
    /// A string, set as it is.
    Text(String),
    /// `${number}`: printed as JavaScript's `Number::toString` prints it.
    Number(f64),
    /// rough.js path data (`RoughGenerator.opsToPath(drawing,
    /// fixedDecimals)`): `move`, `lineTo` and `bcurveTo` ops as `M`, `L` and
    /// `C` commands, each number through `+n.toFixed(decimals)` when
    /// `decimals` is given. The path holds only `MoveTo`, `LineTo` and
    /// `CubicTo`.
    RoughPath { path: Path, decimals: Option<usize> },
}

impl From<String> for SvgValue {
    fn from(s: String) -> SvgValue {
        SvgValue::Text(s)
    }
}

impl From<&str> for SvgValue {
    fn from(s: &str) -> SvgValue {
        SvgValue::Text(s.to_owned())
    }
}

impl From<f64> for SvgValue {
    fn from(n: f64) -> SvgValue {
        SvgValue::Number(n)
    }
}

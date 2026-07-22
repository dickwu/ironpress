//! Shared block-flow geometry carried by layout elements.
//!
//! Concrete box types implement [`MarginHolder`]; metadata nodes do not.

/// Physical block-axis margins for the horizontal writing-mode layout tree.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct BlockMargins {
    pub(crate) start: f32,
    pub(crate) end: f32,
}

impl BlockMargins {
    pub(crate) const ZERO: Self = Self::new(0.0, 0.0);

    pub(crate) const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    pub(crate) const fn total(self) -> f32 {
        self.start + self.end
    }
}

/// Common margin access for every concrete [`LayoutElement`] box kind.
///
/// Consumers operate on the semantic pair and no longer destructure every
/// element variant merely to rediscover the same two scalar fields.
pub(crate) trait MarginHolder {
    fn margins(&self) -> &BlockMargins;
    fn margins_mut(&mut self) -> &mut BlockMargins;
}

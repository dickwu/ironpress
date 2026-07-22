//! Ordered filter-list evaluation over one composited source graphic.

mod color;
mod surface;

pub(crate) use color::apply_operations_to_color;
pub(crate) use surface::apply_operations_to_surface;

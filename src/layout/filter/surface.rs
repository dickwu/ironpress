//! Raster `SourceGraphic` construction for CSS filters.

mod canvas;
mod cells;
mod dispatch;
mod geometry;
mod gradient;
mod overflow;
mod painter;
mod source_borders;
mod source_graphic;
mod text;

pub(crate) use cells::{
    flex_cell_source_frames, grid_cell_source_frames, paint_flex_cell_source,
    paint_grid_cell_source,
};
pub(crate) use geometry::{
    SourceGeometry, SourceGraphic, SourceRasterAnchor, block_child_frames, source_geometry,
};
pub(crate) use source_graphic::paint_source_graphic;

#[cfg(test)]
mod tests;

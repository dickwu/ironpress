use super::*;

#[derive(Debug, Clone)]
struct RadialMaskShading {
    geometry: RadialGradientGeometry,
    terminal: f32,
    terminal_coverage: f32,
    stops: PdfGradientStops,
}

impl RadialMaskShading {
    fn resolve(layer: &MaskLayer, tile: PdfRect, invert_coverage: bool) -> Option<Self> {
        let MaskLayerSource::Radial(gradient) = &layer.source else {
            return None;
        };
        if gradient.shape != RadialShape::Circle {
            return None;
        }
        let geometry =
            RadialGradientGeometry::resolve(gradient, PdfVector::new(tile.width, tile.height))?;
        let resolved = gradient.ramp.resolve(geometry.stop_basis())?;
        if resolved.repeat().is_repeating() {
            return None;
        }
        let terminal = resolved.stops().last()?.position.max(1.0);
        if !terminal.is_finite()
            || terminal <= 0.0
            || resolved.stops().iter().any(|stop| {
                !stop.position.is_finite() || stop.position < 0.0 || stop.position > terminal
            })
        {
            return None;
        }
        let coverage = |color: crate::types::Color| {
            let coverage = coverage_fraction(color.to_f32_rgba(), layer.mode);
            if invert_coverage {
                1.0 - coverage
            } else {
                coverage
            }
        };
        let terminal_coverage = coverage(resolved.stops().last()?.color.color);
        let stops = PdfGradientStops::unit(resolved.stops().iter().map(|stop| {
            let gray = coverage(stop.color.color);
            (stop.position / terminal, (gray, gray, gray))
        }))
        .ok()?;
        Some(Self {
            geometry,
            terminal,
            terminal_coverage,
            stops,
        })
    }

    fn pattern(self, writer: &mut PdfWriter, tile: PdfRect) -> String {
        writer.add_shading_pattern(PdfShadingPattern::radial(
            self.geometry.center,
            self.geometry.radii.x * self.terminal,
            PdfMatrix::new(
                PdfVector::new(crate::fonts::PT_PER_CSS_PX, 0.0),
                PdfVector::new(0.0, -crate::fonts::PT_PER_CSS_PX),
                PdfPoint::new(tile.left, tile.top()),
            ),
            self.stops,
            PdfPatternGeometryFormat::SixDecimals,
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct BinaryLinearMask {
    boundary: f32,
    opaque_at_start: bool,
    reverse_axis: bool,
}

impl BinaryLinearMask {
    fn resolve(layer: &MaskLayer, tile: PdfRect) -> Option<Self> {
        let MaskLayerSource::Linear(gradient) = &layer.source else {
            return None;
        };
        let angle = gradient.angle.rem_euclid(360.0);
        let reverse_axis = if (angle - 90.0).abs() <= 0.000_1 {
            false
        } else if (angle - 270.0).abs() <= 0.000_1 {
            true
        } else {
            return None;
        };
        let basis = linear_gradient_line_length(gradient.angle, tile.width, tile.height);
        let resolved = gradient.ramp.resolve(basis)?;
        if resolved.repeat().is_repeating() {
            return None;
        }
        let stops = resolved.stops();
        let binary_coverage = |stop: &crate::style::computed::ResolvedGradientStop| {
            let coverage = coverage_fraction(stop.color.color.to_f32_rgba(), layer.mode);
            if coverage <= f32::EPSILON {
                Some(false)
            } else if (coverage - 1.0).abs() <= f32::EPSILON {
                Some(true)
            } else {
                None
            }
        };
        let opaque_at_start = binary_coverage(stops.first()?)?;
        let opaque_at_end = binary_coverage(stops.last()?)?;
        if opaque_at_start == opaque_at_end {
            return None;
        }
        let mut boundary = None;
        for pair in stops.windows(2) {
            let lower = binary_coverage(&pair[0])?;
            let upper = binary_coverage(&pair[1])?;
            if lower == upper {
                continue;
            }
            if pair[0].position != pair[1].position || boundary.replace(pair[0].position).is_some()
            {
                return None;
            }
        }
        let boundary = boundary?;
        ((0.0..=1.0).contains(&boundary)).then_some(Self {
            boundary,
            opaque_at_start,
            reverse_axis,
        })
    }

    fn coverage_rect(self, tile: PdfRect, invert: bool) -> Option<PdfRect> {
        let start_is_covered = self.opaque_at_start != invert;
        let boundary_x = if self.reverse_axis {
            tile.right() - tile.width * self.boundary
        } else {
            tile.left + tile.width * self.boundary
        };
        let start_is_left = !self.reverse_axis;
        let covered_is_left = start_is_covered == start_is_left;
        let rect = if covered_is_left {
            PdfRect::new(tile.left, tile.bottom, boundary_x - tile.left, tile.height)
        } else {
            PdfRect::new(
                boundary_x,
                tile.bottom,
                tile.right() - boundary_x,
                tile.height,
            )
        };
        (!rect.is_empty()).then_some(rect)
    }
}

/// Re-express an alpha-interpreted conic image as an opaque grayscale conic
/// gradient. In a DeviceRGB luminosity group, an equal RGB triple produces the
/// same coverage value. This keeps the native PDF function shading exact for
/// both `mask-mode: alpha` and the `match-source` default for CSS images.
///
/// Luminance masks deliberately stay on the general path: their coverage also
/// depends on the interpolated colour channels, whereas alpha coverage is the
/// gradient alpha at every point.
fn alpha_mask_conic_gradient(gradient: &ConicGradient, mode: MaskMode) -> Option<ConicGradient> {
    if !matches!(mode, MaskMode::Alpha | MaskMode::MatchSource) {
        return None;
    }

    let stops = gradient
        .ramp
        .stops
        .iter()
        .map(|stop| {
            let alpha = stop.color.color.to_f32_rgba().3;
            let coverage = crate::types::Color::from_srgb(alpha, alpha, alpha, 1.0);
            crate::style::computed::GradientStop {
                color: crate::style::computed::GradientColor::new(
                    coverage,
                    crate::style::computed::GradientColorProvenance::LegacySrgb,
                ),
                position: stop.position,
                hint_after: stop.hint_after,
            }
        })
        .collect();

    Some(ConicGradient {
        from_angle: gradient.from_angle,
        center: gradient.center,
        ramp: GradientRamp {
            stops,
            interpolation: GradientInterpolation::Srgb,
            repeat: gradient.ramp.repeat,
        },
        layer_box: gradient.layer_box,
    })
}

impl PdfWriter {
    /// Build a CSS `mask-image` soft mask (css-masking-1 §3) for a box of size
    /// `w` × `h` points whose top-left sits at PDF coordinate (`x`, `top_y`).
    ///
    /// The mask source is rasterised to a DeviceGray coverage buffer (alpha for
    /// `mask-mode: alpha`/`match-source` on a CSS image, luminance for
    /// `luminance`), wrapped in a `/Luminosity` transparency-group form XObject
    /// positioned over the box, and registered as an `/SMask` ExtGState. Returns
    /// the graphics-state name to emit with `gs` (the caller wraps the masked
    /// paint in `q /name gs ... Q`), or `None` if the source can't be rasterised.
    pub(super) fn add_mask_soft_mask(
        &mut self,
        source: &MaskSource,
        mode: MaskMode,
        geometry: BoxGeometry,
    ) -> Option<String> {
        let border_box = geometry.border_box;
        if border_box.is_empty() {
            return None;
        }
        // VECTOR path for SVG masks (resolution-independent, like Chrome, which
        // emits the mask as transparency-group bezier paths — NOT a fixed-DPI
        // bitmap). Render the SVG as PDF vector ops into the luminosity group.
        // Falls through to the raster path below for gradient masks, or any SVG
        // that can't be vectorised self-contained (needs shading/image/font
        // resources, or fails to parse).
        // VECTOR paths for gradient masks paint grayscale coverage into a
        // DeviceRGB luminosity group. They remain resolution-independent and
        // avoid changing an authored mask's edges through a bitmap soft mask.
        if let MaskSource::Linear(lg) = source {
            if let Some(gs) = self.try_linear_mask_vector_shading(lg, mode, geometry) {
                return Some(gs);
            }
        }
        if let MaskSource::Conic(cg) = source {
            if let Some(gs) = self.try_conic_mask_vector_shading(cg, mode, geometry) {
                return Some(gs);
            }
        }
        if let MaskSource::Radial(rg) = source {
            if let Some(gs) = self.try_repeating_radial_mask_function(rg, mode, geometry.border_box)
            {
                return Some(gs);
            }
        }
        if let MaskSource::Layers(layers) = source {
            if let Some(gs) = self.try_single_linear_mask_layer_shading(layers, geometry) {
                return Some(gs);
            }
            if let Some(gs) = self.try_single_conic_mask_layer_shading(layers, geometry) {
                return Some(gs);
            }
            if let Some(gs) = self.try_radial_binary_composite_mask(layers, geometry) {
                return Some(gs);
            }
            if let Some(gs) = self.try_two_layer_add_mask_alpha(layers, geometry) {
                return Some(gs);
            }
            if let Some(gs) = self.try_opaque_linear_exclude_radial_mask(layers, geometry) {
                return Some(gs);
            }
            if let Some(gs) = self.try_single_repeating_radial_mask_layer_function(layers, geometry)
            {
                return Some(gs);
            }
            if let Some(gs) = self.try_single_radial_mask_layer_shading(layers, geometry) {
                return Some(gs);
            }
        }
        // Raster masks use their own physical-resolution policy. Only bounded
        // DeviceGray windows exist in memory or in individual PDF image
        // XObjects; their transforms place them on exact adjacent grid edges.
        let grid = MaskRasterGrid::new(
            crate::style::raster_quality::mask_raster_dimensions(
                border_box.width,
                border_box.height,
                self.opts.raster_quality.mask_dpi,
            )?,
            border_box.width,
            border_box.height,
        )?;
        let mut xobjects = String::new();
        let mut group_stream = String::new();
        for tile in grid.pixels.tiles(MAX_RASTER_TILE_EDGE)? {
            let coverage =
                rasterize_mask_source(source, mode, grid.window(tile)?, geometry, &self.svg_defs)?;
            let img_id = self.add_flate_image_stream(
                flate_compress(&coverage)?,
                tile.width,
                tile.height,
                "/DeviceGray",
                None,
                PdfImageInterpolation::Default,
            );
            let image_name = format!("MaskImg{img_id}");
            xobjects.push_str(&format!("/{image_name} {img_id} 0 R "));
            let rect = border_box.raster_tile(grid.pixels, tile);
            group_stream.push_str(&format!(
                "q\n{} 0 0 {} {} {} cm\n/{image_name} Do\nQ\n",
                rect.width, rect.height, rect.left, rect.bottom,
            ));
        }

        // Transparency-group form XObject that draws the coverage tiles over
        // the box. The luminosity group backdrop is black, so pixels outside
        // those exact tile rectangles have zero coverage.
        let group_bytes = group_stream.into_bytes();
        let form_id = self.next_id();
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{x} {y} {right} {top}] /Group << /Type /Group /S /Transparency /CS /DeviceGray >> /Resources << /XObject << {xobjects}>> >> /Length {len} >>\nstream\n",
            x = border_box.left,
            y = border_box.bottom,
            right = border_box.right(),
            top = border_box.top(),
            len = group_bytes.len(),
        ));
        self.binary_objects.insert(form_id, group_bytes);

        let gs_name = format!("GSmask{form_id}");
        self.register_soft_mask(gs_name.clone(), form_id);
        Some(gs_name)
    }

    /// Try to build a CSS `mask-image: url(svg)` soft mask as resolution-
    /// independent VECTOR paths (matching Chrome), rendering the SVG into the
    /// luminosity transparency group instead of a fixed-DPI coverage bitmap.
    /// Returns `None` (so the caller falls back to raster) when the SVG can't be
    /// parsed, has zero size, renders nothing, or needs gradient/image/font
    /// resources the self-contained mask form can't carry.
    #[allow(dead_code)]
    pub(super) fn try_svg_vector_soft_mask(
        &mut self,
        svg_bytes: &[u8],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let border_box = geometry.border_box;
        let svg_text = std::str::from_utf8(svg_bytes).ok()?;
        let tree = crate::parser::svg::parse_svg_from_string(svg_text)?;
        // The SVG user-coordinate extent (viewBox if present, else width/height).
        let (sw, sh) = tree
            .view_box
            .map(|vb| (vb.width, vb.height))
            .unwrap_or((tree.width, tree.height));
        if !(sw > 0.0 && sh > 0.0) {
            return None;
        }
        let mut svg_content = String::new();
        let mut shadings = Vec::new();
        let mut shading_counter = 0usize;
        crate::render::svg_to_pdf::render_svg_tree_with_shadings(
            &tree,
            &mut svg_content,
            &mut shadings,
            &mut shading_counter,
        );
        // The mask form is self-contained (empty /Resources): bail to raster if
        // the SVG produced gradient shadings (would need /Shading resources) or
        // drew nothing.
        if !shadings.is_empty() || svg_content.trim().is_empty() {
            return None;
        }
        // Map the SVG user space (y-down, 0..sw × 0..sh) onto the box (PDF y-up):
        // scale to the box and flip Y so SVG (0,0) lands at the box top-left.
        let group = format!(
            "q\n{a} 0 0 {d} {e} {f} cm\n{svg_content}Q\n",
            a = format_pdf_number(border_box.width / sw),
            d = format_pdf_number(-border_box.height / sh),
            e = format_pdf_number(border_box.left),
            f = format_pdf_number(border_box.top()),
        );
        let group_bytes = group.into_bytes();
        let form_id = self.next_id();
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{x} {bottom} {right} {top}] /Group << /Type /Group /S /Transparency /CS /DeviceRGB >> /Resources << >> /Length {len} >>\nstream\n",
            x = border_box.left,
            bottom = border_box.bottom,
            right = border_box.right(),
            top = border_box.top(),
            len = group_bytes.len(),
        ));
        self.binary_objects.insert(form_id, group_bytes);
        let gs_name = format!("GSmask{form_id}");
        self.register_soft_mask(gs_name.clone(), form_id);
        Some(gs_name)
    }

    /// Render the initial one-layer linear-gradient mask as a native PDF
    /// shading. Any size, position, repeat, origin, clip, or composite override
    /// stays on the general tiled path below.
    fn try_single_linear_mask_layer_shading(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [layer] = layers else {
            return None;
        };
        let MaskLayerSource::Linear(gradient) = &layer.source else {
            return None;
        };
        if !layer.uses_initial_paint_area() {
            return None;
        }
        self.try_linear_mask_vector_shading(gradient, layer.mode, geometry)
    }

    /// Render an initial one-layer conic-gradient mask through the same native
    /// path as a directly stored conic source. Longhand application retains
    /// layer metadata even when it has its initial values, so this keeps the
    /// two equivalent computed forms represented identically in PDF.
    fn try_single_conic_mask_layer_shading(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [layer] = layers else {
            return None;
        };
        let MaskLayerSource::Conic(gradient) = &layer.source else {
            return None;
        };
        if !layer.uses_initial_paint_area() {
            return None;
        }
        self.try_conic_mask_vector_shading(gradient, layer.mode, geometry)
    }

    /// Render an initial one-layer repeating radial mask as a type-1 function
    /// in a luminosity group. Its implicit `mask-repeat: repeat` does not add a
    /// second tile here: an initial CSS gradient tile already spans the whole
    /// border box, while the radial function itself supplies the ring period.
    fn try_single_repeating_radial_mask_layer_function(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [layer] = layers else {
            return None;
        };
        let MaskLayerSource::Radial(gradient) = &layer.source else {
            return None;
        };
        layer.uses_initial_paint_area().then(|| {
            self.try_repeating_radial_mask_function(gradient, layer.mode, geometry.border_box)
        })?
    }

    /// Build a vector soft mask for an alpha-interpreted repeating radial
    /// gradient. A PDF function emits equal RGB coverage inside a luminosity
    /// transparency group, so the result has no source-raster resolution or
    /// tiling seam.
    fn try_repeating_radial_mask_function(
        &mut self,
        gradient: &RadialGradient,
        mode: MaskMode,
        tile: PdfRect,
    ) -> Option<String> {
        if gradient.shape != RadialShape::Circle
            || !matches!(mode, MaskMode::Alpha | MaskMode::MatchSource)
        {
            return None;
        }
        let radial =
            RadialGradientGeometry::resolve(gradient, PdfVector::new(tile.width, tile.height))?;
        let function = radial_alpha_function_gradient(&gradient.ramp, radial.stop_basis())?;
        let period_radii = radial.point_radii() * function.period();
        if !period_radii.is_positive() {
            return None;
        }
        let transform = PdfMatrix::new(
            PdfVector::new(period_radii.x, 0.0),
            PdfVector::new(0.0, -period_radii.y),
            radial.page_center(tile),
        );
        let mask_bounds = self.page_content_transform.enclosing_device_bounds(tile);
        // Type-1 shading domains are clipped at their upper/left boundary by
        // Poppler's sample grid. Keep one physical device pixel of function
        // domain outside the form's paint and BBox: the form itself remains
        // exactly `mask_bounds`, while its boundary samples receive the same
        // gradient coverage as the interior.
        let function_bounds =
            mask_bounds.outset_uniform(PageContentTransform::DEVICE_TO_PAGE as f32);
        let inverse = transform.inverse()?;
        let domain = function_bounds.transformed_bounds(inverse);
        let pattern_name = self.add_function_pattern(PdfFunctionPattern::new(
            transform,
            domain,
            function.calculator()?,
        )?);
        let pattern_id = self
            .pdf_patterns
            .last()
            .filter(|entry| entry.name == pattern_name)?
            .object_id;
        let group = format!(
            "q\n/Pattern cs\n/{pattern_name} scn\n{}f\nQ\n",
            mask_bounds.rect_path(),
        );
        let form_id = self.next_id();
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{left} {bottom} {right} {top}] /Group << /Type /Group /S /Transparency /CS /DeviceRGB /I true >> /Resources << /Pattern << /{pattern_name} {pattern_id} 0 R >> >> /Length {len} >>\nstream\n",
            left = mask_bounds.left,
            bottom = mask_bounds.bottom,
            right = mask_bounds.right(),
            top = mask_bounds.top(),
            len = group.len(),
        ));
        self.binary_objects.insert(form_id, group.into_bytes());
        let state_name = format!("GSmask{form_id}");
        self.register_soft_mask(state_name.clone(), form_id);
        Some(state_name)
    }

    /// Build a `mask-image: linear-gradient(...)` soft mask as a native PDF axial
    /// shading (vector, resolution-independent) instead of a coverage bitmap. The
    /// shading paints `(g, g, g)` where `g` is the mask coverage the gradient
    /// asks for (alpha for image masks, luminance for luminance mode), into a
    /// DeviceRGB `/Luminosity` transparency group — whose luminosity of an equal
    /// RGB triple is exactly `g`. Returns `None` for a degenerate (<2 stop)
    /// gradient so the caller falls back to raster.
    pub(super) fn try_linear_mask_vector_shading(
        &mut self,
        lg: &crate::style::computed::LinearGradient,
        mode: crate::style::computed::MaskMode,
        geometry: BoxGeometry,
    ) -> Option<String> {
        self.try_linear_gradient_luminosity_mask(lg, geometry.border_box, |color| {
            f32::from(coverage_byte(color.to_f32_rgba(), mode)) / 255.0
        })
    }

    /// Build an alpha conic-gradient mask as a PDF type-1 function shading in
    /// an isolated luminosity group. This is the same resolution-independent
    /// PDF representation Chrome uses for the repeating spoke fixture.
    fn try_conic_mask_vector_shading(
        &mut self,
        gradient: &ConicGradient,
        mode: MaskMode,
        geometry: BoxGeometry,
    ) -> Option<String> {
        let gradient = alpha_mask_conic_gradient(gradient, mode)?;
        let border_box = geometry.border_box;
        let center = PdfPoint::new(
            border_box.left + gradient.center.x.resolve(border_box.width),
            border_box.bottom + border_box.height - gradient.center.y.resolve(border_box.height),
        );
        let function = build_conic_shading_function(&gradient, center)?;
        if function.opacity != 1.0 {
            return None;
        }

        let mask_bounds = self
            .page_content_transform
            .enclosing_device_bounds(border_box);
        let shading_name = self.add_conic_shading(mask_bounds, function);
        let stream = format!(
            "q\n{clip} W n\n/{shading_name} sh\nQ\n",
            clip = border_box.rect_path(),
        );
        let form = self.add_transparency_group_form(stream, mask_bounds);
        let gs_name = format!("GSmask{}", form.obj_id);
        self.register_soft_mask(gs_name.clone(), form.obj_id);
        Some(gs_name)
    }

    /// Build the alpha channel of a translucent CSS linear gradient as a
    /// resolution-independent luminosity soft mask.
    pub(super) fn try_linear_gradient_alpha_mask(
        &mut self,
        gradient: &crate::style::computed::LinearGradient,
        tile: PdfRect,
    ) -> Option<String> {
        let basis = linear_gradient_line_length(gradient.angle, tile.width, tile.height);
        if !basis.is_finite() || basis <= 0.0 {
            return None;
        }
        let resolved = gradient.ramp.resolve(basis)?;
        let stops = PdfGradientStops::unit(resolved.fixed_unit_interval_stops()?.into_iter().map(
            |stop| {
                let gray = stop.color.color.to_f32_rgba().3;
                (stop.position, (gray, gray, gray))
            },
        ))
        .ok()?;
        let css_size = PdfVector::new(
            tile.width / crate::fonts::PT_PER_CSS_PX,
            tile.height / crate::fonts::PT_PER_CSS_PX,
        );
        let (sin, cos) = sin_cos_degrees(gradient.angle);
        let half = (css_size.x * sin.abs() + css_size.y * cos.abs()) / 2.0;
        let center = PdfPoint::new(css_size.x / 2.0, css_size.y / 2.0);
        let axis = PdfVector::new(sin * half, cos * half);
        let (start, end) = if cos == 0.0 {
            (
                PdfPoint::new(center.x - axis.x, 0.0),
                PdfPoint::new(center.x + axis.x, 0.0),
            )
        } else {
            (center - axis, center + axis)
        };
        let pattern_name = self.add_shading_pattern(PdfShadingPattern::axial(
            start,
            end,
            PdfMatrix::new(
                PdfVector::new(crate::fonts::PT_PER_CSS_PX, 0.0),
                PdfVector::new(0.0, -crate::fonts::PT_PER_CSS_PX),
                PdfPoint::new(tile.left, tile.top()),
            ),
            stops,
            PdfPatternGeometryFormat::SixDecimals,
        ));
        let pattern_id = self
            .pdf_patterns
            .last()
            .filter(|entry| entry.name == pattern_name)?
            .object_id;
        let mask_bounds = self.page_content_transform.page_bounds().unwrap_or(tile);
        let group = format!(
            "/Pattern CS/Pattern cs\n/{pattern_name} SCN/{pattern_name} scn\n{}f*\n",
            mask_bounds.rect_path(),
        );
        let form_id = self.next_id();
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{left} {bottom} {right} {top}] /Group << /Type /Group /S /Transparency /CS /DeviceRGB /I true >> /Resources << /Pattern << /{pattern_name} {pattern_id} 0 R >> >> /Length {len} >>\nstream\n",
            left = mask_bounds.left,
            bottom = mask_bounds.bottom,
            right = mask_bounds.right(),
            top = mask_bounds.top(),
            len = group.len(),
        ));
        self.binary_objects.insert(form_id, group.into_bytes());
        let state_name = format!("GSmask{form_id}");
        self.register_soft_mask(state_name.clone(), form_id);
        Some(state_name)
    }

    fn try_linear_gradient_luminosity_mask(
        &mut self,
        gradient: &crate::style::computed::LinearGradient,
        tile: PdfRect,
        coverage: impl Fn(crate::types::Color) -> f32,
    ) -> Option<String> {
        let basis = linear_gradient_line_length(gradient.angle, tile.width, tile.height);
        if !basis.is_finite() || basis <= 0.0 {
            return None;
        }
        let resolved = gradient.ramp.resolve(basis)?;
        // Coverage as a gray level per stop, replicated into an RGB triple.
        let stops = PdfGradientStops::unit(resolved.fixed_unit_interval_stops()?.into_iter().map(
            |stop| {
                let gray = coverage(stop.color.color).clamp(0.0, 1.0);
                (stop.position, (gray, gray, gray))
            },
        ))
        .ok()?;
        // Chrome allocates the mask form in integer print-device pixels. Keep
        // the CSS gradient's original coordinates, but enclose its paint bound
        // so the final partially covered device edge participates in the soft
        // mask instead of being clipped away in layout-point space.
        let paint_bounds = self.page_content_transform.enclosing_device_bounds(tile);
        let mut shadings = Vec::new();
        let mut counter = 0usize;
        let mut group = String::new();
        render_linear_gradient_tile_clipped(
            &mut group,
            gradient.angle,
            tile,
            paint_bounds,
            NativePdfGradient::unit(stops),
            self.page_content_transform,
            &mut shadings,
            &mut counter,
        );
        let entry = shadings.into_iter().next()?;
        let function_str = build_shading_function(&entry.stops);
        let sh_id = self.next_id();
        self.objects.push(format!(
            "{sh_id} 0 obj\n<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] /Function {function_str} /Extend [true true] >>\nendobj",
            entry.coords[0], entry.coords[1], entry.coords[2], entry.coords[3],
        ));
        let group_bytes = group.into_bytes();
        let form_id = self.next_id();
        self.objects.push(format!(
            "{form_id} 0 obj\n<< /Type /XObject /Subtype /Form /FormType 1 /BBox [{x} {bottom} {right} {top}] /Group << /Type /Group /S /Transparency /CS /DeviceRGB >> /Resources << /Shading << /{name} {sh_id} 0 R >> >> /Length {len} >>\nstream\n",
            x = paint_bounds.left,
            bottom = paint_bounds.bottom,
            right = paint_bounds.right(),
            top = paint_bounds.top(),
            name = entry.name,
            len = group_bytes.len(),
        ));
        self.binary_objects.insert(form_id, group_bytes);
        let gs_name = format!("GSmask{form_id}");
        self.register_soft_mask(gs_name.clone(), form_id);
        Some(gs_name)
    }

    /// Build one full-size circular radial layer as a vector luminosity mask.
    /// The resulting state is deliberately kept separate from composition: a
    /// later isolated group can combine several such alpha-painted layers with
    /// PDF's normal source-over rule.
    fn try_radial_layer_luminosity_mask(
        &mut self,
        layer: &MaskLayer,
        geometry: BoxGeometry,
        invert_coverage: bool,
    ) -> Option<String> {
        let MaskLayerSource::Radial(gradient) = &layer.source else {
            return None;
        };
        if gradient.shape != RadialShape::Circle {
            return None;
        }
        let MaskLayerPaint { tile, clip } = MaskLayerPaint::resolve(layer, geometry)?;
        if tile != geometry.border_box || clip != geometry.border_box {
            return None;
        }
        let shading = RadialMaskShading::resolve(layer, tile, invert_coverage)?;
        let expected_tail = if invert_coverage { 1.0 } else { 0.0 };
        if shading.terminal_coverage != expected_tail {
            return None;
        }
        let pattern = shading.pattern(self, tile);
        let stream = format!("/Pattern cs\n/{pattern} scn\n{}f\n", tile.rect_path(),);
        let form = self.add_transparency_group_form(stream, tile);
        let state = format!("GSmask{}", form.obj_id);
        self.register_soft_mask(state.clone(), form.obj_id);
        Some(state)
    }

    /// Vector source-over composition for two full-size circular alpha masks.
    /// CSS `mask-composite: add` is the union `s + d * (1 - s)`, which is the
    /// alpha produced by painting the two source masks normally into an
    /// isolated transparency group.
    fn try_two_layer_add_mask_alpha(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [top, bottom] = layers else {
            return None;
        };
        if top.composite != MaskComposite::Add || bottom.composite != MaskComposite::Add {
            return None;
        }
        if layers.iter().any(|layer| {
            !matches!(
                layer.layer_box.repeat.unwrap_or(BackgroundRepeat::Repeat),
                BackgroundRepeat::NoRepeat
            )
        }) {
            return None;
        }

        let bottom_mask = self.try_radial_layer_luminosity_mask(bottom, geometry, false)?;
        let bottom_pattern =
            self.add_masked_solid_page_pattern(geometry.border_box, &bottom_mask, (1.0, 1.0, 1.0))?;
        let top_mask = self.try_radial_layer_luminosity_mask(top, geometry, false)?;
        let top_pattern =
            self.add_masked_solid_page_pattern(geometry.border_box, &top_mask, (1.0, 1.0, 1.0))?;

        let mut stream = String::new();
        patterns::paint_page_tiling_pattern(&mut stream, &bottom_pattern, geometry.border_box);
        patterns::paint_page_tiling_pattern(&mut stream, &top_pattern, geometry.border_box);
        let form = self.add_transparency_group_form(stream, geometry.border_box);
        let state = format!("GSmask{}", form.obj_id);
        self.register_alpha_soft_mask(state.clone(), form.obj_id);
        Some(state)
    }

    /// Intersect a circular radial layer with one binary horizontal linear
    /// layer without reducing either source to a bitmap. `subtract` is the
    /// same radial shading clipped to the inverse binary region.
    fn try_radial_binary_composite_mask(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [radial, linear] = layers else {
            return None;
        };
        let invert_linear = match radial.composite {
            MaskComposite::Intersect => false,
            MaskComposite::Subtract => true,
            _ => return None,
        };
        if layers.iter().any(|layer| {
            !matches!(
                layer.layer_box.repeat.unwrap_or(BackgroundRepeat::Repeat),
                BackgroundRepeat::NoRepeat
            )
        }) {
            return None;
        }
        let radial_paint = MaskLayerPaint::resolve(radial, geometry)?;
        let linear_paint = MaskLayerPaint::resolve(linear, geometry)?;
        if radial_paint != linear_paint
            || radial_paint.tile != geometry.border_box
            || radial_paint.clip != geometry.border_box
        {
            return None;
        }
        let paint = BinaryLinearMask::resolve(linear, linear_paint.tile)?
            .coverage_rect(linear_paint.tile, invert_linear)?
            .intersection(radial_paint.tile)?
            .intersection(radial_paint.clip)?;
        let pattern = RadialMaskShading::resolve(radial, radial_paint.tile, false)?
            .pattern(self, radial_paint.tile);
        let stream = format!("/Pattern cs\n/{pattern} scn\n{}f\n", paint.rect_path());
        let form = self.add_transparency_group_form(stream, geometry.border_box);
        let state = format!("GSmask{}", form.obj_id);
        self.register_soft_mask(state.clone(), form.obj_id);
        Some(state)
    }

    /// Reduce an opaque linear source excluding a radial destination to the
    /// inverse radial coverage. This is the CSS alpha-composite result
    /// `1 * (1 - d) + d * (1 - 1)`, represented directly in one native PDF
    /// soft mask. Other layered exclusions remain on the general raster path:
    /// PDF's `/Exclusion` colour blend mode is not CSS mask alpha composition.
    pub(super) fn try_opaque_linear_exclude_radial_mask(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [top, bottom] = layers else {
            return None;
        };
        if top.composite != MaskComposite::Exclude {
            return None;
        }
        if layers.iter().any(|layer| {
            !matches!(
                layer.layer_box.repeat.unwrap_or(BackgroundRepeat::Repeat),
                BackgroundRepeat::NoRepeat
            )
        }) {
            return None;
        }
        let MaskLayerSource::Linear(linear) = &top.source else {
            return None;
        };
        let MaskLayerPaint { tile, clip } = MaskLayerPaint::resolve(top, geometry)?;
        if tile != geometry.border_box || clip != geometry.border_box {
            return None;
        }
        let basis = linear_gradient_line_length(linear.angle, tile.width, tile.height);
        let resolved = linear.ramp.resolve(basis)?;
        if !resolved
            .stops()
            .iter()
            .all(|stop| coverage_fraction(stop.color.color.to_f32_rgba(), top.mode) == 1.0)
        {
            return None;
        }
        self.try_radial_layer_luminosity_mask(bottom, geometry, true)
    }

    pub(super) fn try_single_radial_mask_layer_shading(
        &mut self,
        layers: &[MaskLayer],
        geometry: BoxGeometry,
    ) -> Option<String> {
        let [layer] = layers else {
            return None;
        };
        if layer.composite != MaskComposite::Add
            || !matches!(
                layer.layer_box.repeat.unwrap_or(BackgroundRepeat::Repeat),
                BackgroundRepeat::NoRepeat
            )
        {
            return None;
        }
        let MaskLayerSource::Radial(_) = &layer.source else {
            return None;
        };
        if !matches!(
            layer.layer_box.size,
            None | Some(BackgroundSize::Auto | BackgroundSize::Explicit { .. })
        ) {
            return None;
        }
        let MaskLayerPaint { tile, clip } = MaskLayerPaint::resolve(layer, geometry)?;
        let paint = tile.intersection(clip)?;
        let pattern = RadialMaskShading::resolve(layer, tile, false)?.pattern(self, tile);
        let stream = format!("/Pattern cs\n/{pattern} scn\n{}f\n", paint.rect_path());
        let form = self.add_transparency_group_form(stream, geometry.border_box);
        let state = format!("GSmask{}", form.obj_id);
        self.register_soft_mask(state.clone(), form.obj_id);
        Some(state)
    }
}

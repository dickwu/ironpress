use super::*;

#[derive(Clone, Copy)]
pub(super) enum PdfImageInterpolation {
    Default,
    Smooth,
    Crisp,
}

impl PdfImageInterpolation {
    pub(super) fn for_css_image_rendering(
        image_rendering: crate::style::computed::ImageRendering,
    ) -> Self {
        if image_rendering.preserves_source_edges() {
            Self::Crisp
        } else if image_rendering.requests_smooth_pdf_interpolation() {
            Self::Smooth
        } else {
            Self::Default
        }
    }

    const fn pdf_dictionary_entry(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Smooth => " /Interpolate true",
            Self::Crisp => " /Interpolate false",
        }
    }

    const fn preserves_source_edges(self) -> bool {
        matches!(self, Self::Crisp)
    }
}

impl PdfWriter {
    /// Embed a layout image at its display size.
    ///
    /// Document source images participate in the configurable source-image
    /// downscaling policy. Renderer-owned rasters keep their native dimensions:
    /// their pixels were deliberately generated at a filter or background DPI.
    pub(crate) fn add_layout_image_object(
        &mut self,
        image: &crate::layout::engine::RasterImageAsset,
        display_w_pt: f32,
        display_h_pt: f32,
        image_rendering: crate::style::computed::ImageRendering,
    ) -> usize {
        let interpolation = PdfImageInterpolation::for_css_image_rendering(image_rendering);
        if image.origin.preserves_native_resolution() {
            self.add_image_object_with_interpolation(
                &image.data,
                image.source_width,
                image.source_height,
                image.format,
                image.png_metadata.as_ref(),
                interpolation,
            )
        } else if image_rendering.is_pixelated() {
            self.add_pixelated_source_image_object(
                &image.data,
                image.source_width,
                image.source_height,
                display_w_pt,
                display_h_pt,
            )
            .unwrap_or_else(|| {
                self.add_image_object_with_interpolation(
                    &image.data,
                    image.source_width,
                    image.source_height,
                    image.format,
                    image.png_metadata.as_ref(),
                    interpolation,
                )
            })
        } else {
            self.add_source_image_object_with_interpolation(
                &image.data,
                image.source_width,
                image.source_height,
                image.format,
                image.png_metadata.as_ref(),
                display_w_pt,
                display_h_pt,
                interpolation,
            )
        }
    }

    /// Add an image as a PDF XObject and return its object ID.
    pub(super) fn add_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
    ) -> usize {
        self.add_image_object_with_interpolation(
            data,
            width,
            height,
            format,
            png_metadata,
            PdfImageInterpolation::Default,
        )
    }

    /// Embed an image with the requested final PDF sampling behavior.
    pub(super) fn add_image_object_with_interpolation(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        if matches!(format, ImageFormat::Png | ImageFormat::PngAlpha)
            && !interpolation.preserves_source_edges()
            && should_try_lossy_png_reencode(width, height, data.len())
            && let Some(decoded) = try_decode_png_as_opaque_rgb(data)
            && let Some(jpeg) = encode_rgb_as_jpeg(
                &decoded.color_data,
                decoded.width,
                decoded.height,
                self.opts.jpeg_quality,
            )
            && jpeg.len() < data.len()
        {
            return self.add_image_object_with_interpolation(
                &jpeg,
                decoded.width,
                decoded.height,
                ImageFormat::Jpeg,
                None,
                interpolation,
            );
        }
        // An alpha PNG carries the complete original PNG file; decode it into a
        // colour stream plus an `/SMask`, preserving transparency. Fall back to
        // an opaque RGB embedding if decoding fails for any reason.
        if format == ImageFormat::PngAlpha {
            if let Some(obj_id) =
                self.add_raw_png_image_object_with_interpolation(data, interpolation)
            {
                return obj_id;
            }
        }
        let id = self.next_id();
        let interpolation = interpolation.pdf_dictionary_entry();
        let header = match format {
            ImageFormat::Jpeg => {
                format!(
                    "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode{interpolation} /Length {len} >>\nstream\n",
                    len = data.len(),
                )
            }
            ImageFormat::PngAlpha | ImageFormat::Png => {
                // Reaching here for a PngAlpha means the SMask decode above
                // failed (corrupt PNG); recover its metadata from the IHDR so the
                // passthrough header is still well-formed rather than panicking.
                let parsed_png = crate::parser::png::parse_png(data);
                let recovered = parsed_png.as_ref().map(|info| PngMetadata {
                    channels: info.channels,
                    bit_depth: info.bit_depth,
                });
                let Some(meta) = png_metadata.or(recovered.as_ref()) else {
                    return self.add_blank_image_object();
                };
                let color_space = match meta.channels {
                    1 | 2 => "/DeviceGray",
                    _ => "/DeviceRGB",
                };
                let stream_data = parsed_png
                    .as_ref()
                    .map_or(data, |info| info.idat_data.as_slice());
                format!(
                    "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {color_space} /BitsPerComponent {bpc} /Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns {width} /Colors {channels} /BitsPerComponent {bpc} >>{interpolation} /Length {len} >>\nstream\n",
                    bpc = meta.bit_depth,
                    channels = meta.channels,
                    len = stream_data.len(),
                )
            }
        };
        self.objects.push(header);
        let stream_data = match format {
            ImageFormat::Png | ImageFormat::PngAlpha => crate::parser::png::parse_png(data)
                .map_or_else(|| data.to_vec(), |info| info.idat_data),
            ImageFormat::Jpeg => data.to_vec(),
        };
        self.binary_objects.insert(id, stream_data);
        id
    }

    pub(super) fn add_blank_image_object(&mut self) -> usize {
        let id = self.next_id();
        self.objects.push(format!(
            "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n"
        ));
        self.binary_objects.insert(id, vec![255]);
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> usize {
        self.add_source_image_object_with_interpolation(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
            PdfImageInterpolation::Default,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_source_image_object_with_interpolation(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        if let Some(resized) = self.maybe_resize_image(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
        ) {
            self.add_image_object_with_interpolation(
                &resized.data,
                resized.width,
                resized.height,
                resized.format,
                resized.png_metadata.as_ref(),
                interpolation,
            )
        } else {
            self.add_image_object_with_interpolation(
                data,
                width,
                height,
                format,
                png_metadata,
                interpolation,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_pixelated_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<usize> {
        if width == 0 || height == 0 || display_w_pt <= 0.0 || display_h_pt <= 0.0 {
            return None;
        }
        let raster = crate::render::blur::pixelated_image_at_css_size(
            &image::load_from_memory(data).ok()?.to_rgba8(),
            display_w_pt,
            display_h_pt,
        )?;
        if (raster.width(), raster.height()) == (width, height) {
            return None;
        }
        let width = raster.width();
        let height = raster.height();
        let opaque = raster.pixels().all(|pixel| pixel[3] == 255);
        let mut data = Vec::new();
        let (format, png_metadata) = if opaque {
            image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(raster).to_rgb8())
                .write_to(
                    &mut std::io::Cursor::new(&mut data),
                    image::ImageFormat::Png,
                )
                .ok()?;
            let png_metadata = crate::parser::png::parse_png(&data).map(|info| PngMetadata {
                channels: info.channels,
                bit_depth: info.bit_depth,
            });
            (ImageFormat::Png, png_metadata)
        } else {
            image::DynamicImage::ImageRgba8(raster)
                .write_to(
                    &mut std::io::Cursor::new(&mut data),
                    image::ImageFormat::Png,
                )
                .ok()?;
            let png_metadata = crate::parser::png::parse_png(&data).map(|info| PngMetadata {
                channels: info.channels,
                bit_depth: info.bit_depth,
            });
            (ImageFormat::PngAlpha, png_metadata)
        };
        Some(self.add_image_object_with_interpolation(
            &data,
            width,
            height,
            format,
            png_metadata.as_ref(),
            PdfImageInterpolation::Crisp,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_decodable_source_image_object(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<usize> {
        if matches!(format, ImageFormat::Png | ImageFormat::PngAlpha)
            && decode_png_for_pdf(data).is_none()
        {
            return None;
        }
        Some(self.add_source_image_object(
            data,
            width,
            height,
            format,
            png_metadata,
            display_w_pt,
            display_h_pt,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn maybe_resize_image(
        &self,
        data: &[u8],
        source_width: u32,
        source_height: u32,
        format: ImageFormat,
        png_metadata: Option<&PngMetadata>,
        display_w_pt: f32,
        display_h_pt: f32,
    ) -> Option<ResizedImage> {
        if !self.opts.auto_resize_images
            || source_width == 0
            || source_height == 0
            || display_w_pt <= 0.0
            || display_h_pt <= 0.0
        {
            return None;
        }

        let target_w = (display_w_pt * self.opts.raster_quality.source_image_dpi / 72.0)
            .round()
            .max(1.0) as u32;
        let target_h = (display_h_pt * self.opts.raster_quality.source_image_dpi / 72.0)
            .round()
            .max(1.0) as u32;
        let scale = ((target_w as f32 / source_width as f32)
            .min(target_h as f32 / source_height as f32))
        .min(1.0);
        if scale >= 1.0 {
            return None;
        }
        let new_w = ((source_width as f32 * scale).round().max(1.0) as u32).min(source_width);
        let new_h = ((source_height as f32 * scale).round().max(1.0) as u32).min(source_height);
        if new_w >= source_width && new_h >= source_height {
            return None;
        }

        match format {
            ImageFormat::Jpeg => {
                let decoded = image::load_from_memory(data).ok()?.to_rgb8();
                let resized = image::imageops::resize(
                    &decoded,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Lanczos3,
                );
                let encoded =
                    encode_rgb_as_jpeg(resized.as_raw(), new_w, new_h, self.opts.jpeg_quality)?;
                Some(ResizedImage {
                    data: encoded,
                    width: new_w,
                    height: new_h,
                    format: ImageFormat::Jpeg,
                    png_metadata: None,
                })
            }
            ImageFormat::Png | ImageFormat::PngAlpha => {
                let png = crate::layout::images::png_bytes_for_decoding(
                    data,
                    source_width,
                    source_height,
                    png_metadata,
                )?;
                let decoded = image::load_from_memory(&png).ok()?;
                let has_alpha = matches!(
                    decoded.color(),
                    image::ColorType::La8
                        | image::ColorType::La16
                        | image::ColorType::Rgba8
                        | image::ColorType::Rgba16
                        | image::ColorType::Rgba32F
                );
                let mut encoded = Vec::new();
                let output_format = if has_alpha {
                    let rgba = decoded.to_rgba8();
                    let resized = image::imageops::resize(
                        &rgba,
                        new_w,
                        new_h,
                        image::imageops::FilterType::Lanczos3,
                    );
                    image::DynamicImage::ImageRgba8(resized)
                        .write_to(
                            &mut std::io::Cursor::new(&mut encoded),
                            image::ImageFormat::Png,
                        )
                        .ok()?;
                    ImageFormat::PngAlpha
                } else {
                    let rgb = decoded.to_rgb8();
                    let resized = image::imageops::resize(
                        &rgb,
                        new_w,
                        new_h,
                        image::imageops::FilterType::Lanczos3,
                    );
                    image::DynamicImage::ImageRgb8(resized)
                        .write_to(
                            &mut std::io::Cursor::new(&mut encoded),
                            image::ImageFormat::Png,
                        )
                        .ok()?;
                    ImageFormat::Png
                };
                let png_metadata =
                    crate::parser::png::parse_png(&encoded).map(|info| PngMetadata {
                        channels: info.channels,
                        bit_depth: info.bit_depth,
                    });
                Some(ResizedImage {
                    data: encoded,
                    width: new_w,
                    height: new_h,
                    format: output_format,
                    png_metadata,
                })
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn add_icc_profile_object(&mut self, icc_profile: &[u8]) -> Option<usize> {
        let id = self.next_id();
        self.objects.push(format!(
            "{id} 0 obj\n<< /N 3 /Alternate /DeviceRGB /Length {} >>\nstream\n",
            icc_profile.len(),
        ));
        self.binary_objects.insert(id, icc_profile.to_vec());
        Some(id)
    }

    #[allow(dead_code)]
    pub(crate) fn add_raw_rgb_image_object(
        &mut self,
        rgb_data: &[u8],
        width: u32,
        height: u32,
        icc_profile: Option<&[u8]>,
    ) -> Option<usize> {
        let color_stream = flate_compress(rgb_data)?;
        let color_space = if let Some(icc_profile) = icc_profile {
            let icc_id = self.add_icc_profile_object(icc_profile)?;
            format!("[/ICCBased {icc_id} 0 R]")
        } else {
            "/DeviceRGB".to_string()
        };

        Some(self.add_flate_image_stream(
            color_stream,
            width,
            height,
            &color_space,
            None,
            PdfImageInterpolation::Default,
        ))
    }

    pub(super) fn add_raw_rgba_image_object(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Option<usize> {
        let pixels = usize::try_from(width.checked_mul(height)?).ok()?;
        if rgba.len() != pixels.checked_mul(4)? {
            return None;
        }
        let has_alpha = rgba.chunks_exact(4).any(|pixel| pixel[3] != 255);
        let mut rgb = Vec::with_capacity(pixels.checked_mul(3)?);
        let mut alpha = has_alpha.then(|| Vec::with_capacity(pixels));
        for pixel in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
            if let Some(alpha) = &mut alpha {
                alpha.push(pixel[3]);
            }
        }

        let color_stream = flate_compress(&rgb)?;
        let alpha_stream = match alpha {
            Some(alpha) => Some(flate_compress(&alpha)?),
            None => None,
        };
        let alpha_id = alpha_stream.map(|stream| {
            self.add_flate_image_stream(
                stream,
                width,
                height,
                "/DeviceGray",
                None,
                PdfImageInterpolation::Default,
            )
        });
        Some(self.add_flate_image_stream(
            color_stream,
            width,
            height,
            "/DeviceRGB",
            alpha_id,
            PdfImageInterpolation::Default,
        ))
    }

    pub(super) fn add_flate_image_stream(
        &mut self,
        stream: Vec<u8>,
        width: u32,
        height: u32,
        color_space: &str,
        alpha_id: Option<usize>,
        interpolation: PdfImageInterpolation,
    ) -> usize {
        let id = self.next_id();
        let soft_mask = alpha_id.map_or_else(String::new, |id| format!(" /SMask {id} 0 R"));
        let interpolation = interpolation.pdf_dictionary_entry();
        self.objects.push(format!(
            "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {color_space} /BitsPerComponent 8 /Filter /FlateDecode{soft_mask}{interpolation} /Length {len} >>\nstream\n",
            len = stream.len(),
        ));
        self.binary_objects.insert(id, stream);
        id
    }

    pub(crate) fn add_raw_png_image_object(&mut self, raw_png: &[u8]) -> Option<usize> {
        self.add_raw_png_image_object_with_interpolation(raw_png, PdfImageInterpolation::Default)
    }

    fn add_raw_png_image_object_with_interpolation(
        &mut self,
        raw_png: &[u8],
        interpolation: PdfImageInterpolation,
    ) -> Option<usize> {
        let decoded = decode_png_for_pdf(raw_png)?;
        let alpha_id = match decoded.alpha_data.as_deref() {
            Some(alpha) => Some(self.add_flate_image_stream(
                flate_compress(alpha)?,
                decoded.width,
                decoded.height,
                "/DeviceGray",
                None,
                PdfImageInterpolation::Default,
            )),
            None => None,
        };

        // Only large opaque photographic DeviceRGB PNGs may use JPEG. Alpha,
        // DeviceGray, and small images stay lossless Flate.
        let jpeg_color = (alpha_id.is_none()
            && decoded.color_space == "/DeviceRGB"
            && !interpolation.preserves_source_edges()
            && should_try_lossy_png_reencode(decoded.width, decoded.height, raw_png.len()))
        .then(|| {
            encode_rgb_as_jpeg(
                &decoded.color_data,
                decoded.width,
                decoded.height,
                self.opts.jpeg_quality,
            )
        })
        .flatten();
        if let Some(jpeg) = jpeg_color {
            let id = self.next_id();
            let interpolation = interpolation.pdf_dictionary_entry();
            self.objects.push(format!(
                "{id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode{interpolation} /Length {len} >>\nstream\n",
                width = decoded.width,
                height = decoded.height,
                len = jpeg.len(),
            ));
            self.binary_objects.insert(id, jpeg);
            return Some(id);
        }
        Some(self.add_flate_image_stream(
            flate_compress(&decoded.color_data)?,
            decoded.width,
            decoded.height,
            decoded.color_space,
            alpha_id,
            interpolation,
        ))
    }
}

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Convert an HTML string to a PDF document.
///
/// Args:
///     html: HTML string to convert.
///
/// Returns:
///     PDF document as bytes.
///
/// Example:
///     >>> import ironpress
///     >>> pdf = ironpress.html_to_pdf("<h1>Hello</h1>")
///     >>> with open("output.pdf", "wb") as f:
///     ...     f.write(pdf)
#[pyfunction]
fn html_to_pdf(html: &str) -> PyResult<Vec<u8>> {
    ironpress_core::html_to_pdf(html).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Convert a Markdown string to a PDF document.
///
/// Args:
///     markdown: Markdown string to convert.
///
/// Returns:
///     PDF document as bytes.
#[pyfunction]
fn markdown_to_pdf(markdown: &str) -> PyResult<Vec<u8>> {
    ironpress_core::markdown_to_pdf(markdown).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// A configurable HTML-to-PDF converter.
///
/// Example:
///     >>> from ironpress import HtmlConverter
///     >>> converter = HtmlConverter()
///     >>> converter.page_size("Letter")
///     >>> converter.margin(36.0)
///     >>> converter.margin_sides(72.0, 36.0, 72.0, 36.0)
///     >>> converter.compress(False)
///     >>> pdf = converter.convert("<h1>Hello</h1>")
#[pyclass]
struct HtmlConverter {
    page_size: ironpress_core::types::PageSize,
    margin: ironpress_core::types::Margin,
    compress: Option<bool>,
    jpeg_quality: Option<u8>,
    auto_resize_images: Option<bool>,
    source_image_dpi: Option<f32>,
    filter_dpi: Option<f32>,
    mask_dpi: Option<f32>,
    background_dpi: Option<f32>,
    occlusion_cull: Option<bool>,
    sanitize: Option<bool>,
    custom_fonts: Vec<(String, Vec<u8>)>,
    base_path: Option<String>,
    resource_root: Option<String>,
    header: Option<String>,
    footer: Option<String>,
}

impl HtmlConverter {
    /// Build the Rust `HtmlConverter` from stored fields.
    fn build_converter(&self) -> ironpress_core::HtmlConverter {
        let mut converter = ironpress_core::HtmlConverter::new()
            .page_size(self.page_size)
            .margin(self.margin);

        if let Some(compress) = self.compress {
            converter = converter.compress(compress);
        }
        if let Some(quality) = self.jpeg_quality {
            converter = converter.jpeg_quality(quality);
        }
        if let Some(enabled) = self.auto_resize_images {
            converter = converter.auto_resize_images(enabled);
        }
        if let Some(dpi) = self.source_image_dpi {
            converter = converter.image_dpi(dpi);
        }
        if let Some(dpi) = self.filter_dpi {
            converter = converter.filter_dpi(dpi);
        }
        if let Some(dpi) = self.mask_dpi {
            converter = converter.mask_dpi(dpi);
        }
        if let Some(dpi) = self.background_dpi {
            converter = converter.background_raster_dpi(dpi);
        }
        if let Some(enabled) = self.occlusion_cull {
            converter = converter.occlusion_cull(enabled);
        }
        if let Some(enabled) = self.sanitize {
            converter = converter.sanitize(enabled);
        }
        for (name, data) in &self.custom_fonts {
            converter = converter.add_font(name, data.clone());
        }
        if let Some(ref path) = self.base_path {
            converter = converter.base_path(std::path::Path::new(path));
        }
        if let Some(ref root) = self.resource_root {
            converter = converter.resource_root(std::path::Path::new(root));
        }
        if let Some(ref text) = self.header {
            converter = converter.header(text.clone());
        }
        if let Some(ref text) = self.footer {
            converter = converter.footer(text.clone());
        }

        converter
    }
}

#[pymethods]
impl HtmlConverter {
    #[new]
    fn new() -> Self {
        HtmlConverter {
            page_size: ironpress_core::types::PageSize::A4,
            margin: ironpress_core::types::Margin::default(),
            compress: None,
            jpeg_quality: None,
            auto_resize_images: None,
            source_image_dpi: None,
            filter_dpi: None,
            mask_dpi: None,
            background_dpi: None,
            occlusion_cull: None,
            sanitize: None,
            custom_fonts: Vec::new(),
            base_path: None,
            resource_root: None,
            header: None,
            footer: None,
        }
    }

    /// Set uniform page margin in points (72 points = 1 inch).
    ///
    /// Args:
    ///     points: Margin size applied to all four sides.
    fn margin(&mut self, points: f32) {
        self.margin = ironpress_core::types::Margin::uniform(points);
    }

    /// Set individual page margins in points (72 points = 1 inch).
    ///
    /// Args:
    ///     top: Top margin in points.
    ///     right: Right margin in points.
    ///     bottom: Bottom margin in points.
    ///     left: Left margin in points.
    fn margin_sides(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.margin = ironpress_core::types::Margin::new(top, right, bottom, left);
    }

    /// Set page size by name or custom dimensions.
    ///
    /// Supported names: "A4" (210x297mm), "Letter" (8.5x11in), "Legal" (8.5x14in).
    /// For custom sizes, use `page_size_custom()`.
    ///
    /// Args:
    ///     name: Page size name (case-insensitive).
    ///
    /// Raises:
    ///     ValueError: If the name is not recognized.
    fn page_size(&mut self, name: &str) -> PyResult<()> {
        self.page_size = match name.to_lowercase().as_str() {
            "a4" => ironpress_core::types::PageSize::A4,
            "letter" => ironpress_core::types::PageSize::LETTER,
            "legal" => ironpress_core::types::PageSize::LEGAL,
            _ => return Err(PyValueError::new_err(format!("Unknown page size: {name}"))),
        };
        Ok(())
    }

    /// Set a custom page size in points (72 points = 1 inch).
    ///
    /// Args:
    ///     width: Page width in points.
    ///     height: Page height in points.
    fn page_size_custom(&mut self, width: f32, height: f32) {
        self.page_size = ironpress_core::types::PageSize::new(width, height);
    }

    /// Enable or disable FlateDecode compression of page content streams.
    ///
    /// Compression is enabled by default. Disabling produces larger but
    /// human-readable PDFs, useful for debugging.
    ///
    /// Args:
    ///     enabled: Whether to compress content streams.
    fn compress(&mut self, enabled: bool) {
        self.compress = Some(enabled);
    }

    /// Set JPEG quality for optimized image embedding (0-100).
    ///
    /// The default is 95. Lower values produce smaller files at the cost
    /// of image quality.
    ///
    /// Args:
    ///     quality: JPEG quality level (0-100, clamped).
    fn jpeg_quality(&mut self, quality: u8) {
        self.jpeg_quality = Some(quality);
    }

    /// Enable or disable automatic downscaling of oversized source images.
    ///
    /// Enabled by default. When enabled, images larger than needed at the
    /// target DPI are downscaled to reduce PDF file size.
    ///
    /// Args:
    ///     enabled: Whether to auto-resize images.
    fn auto_resize_images(&mut self, enabled: bool) {
        self.auto_resize_images = Some(enabled);
    }

    /// Set the target source-image resolution in DPI (minimum 72, default 300).
    ///
    /// Controls the resolution at which embedded images are stored in the PDF.
    /// Higher values produce sharper images at the cost of file size.
    ///
    /// Args:
    ///     dpi: Target resolution in dots per inch.
    fn image_dpi(&mut self, dpi: f32) {
        self.source_image_dpi = Some(dpi);
    }

    /// Set the rasterization DPI for render-time blur/filter bitmaps.
    ///
    /// Controls the resolution of CSS filter effects like blur. The default
    /// is 300 DPI.
    ///
    /// Args:
    ///     dpi: Target resolution in dots per inch.
    fn filter_dpi(&mut self, dpi: f32) {
        self.filter_dpi = Some(dpi);
    }

    /// Set the rasterization DPI for CSS mask-image coverage bitmaps.
    ///
    /// The default 300 DPI preserves high-contrast coverage edges. It is
    /// independent from blur/filter quality because mask coverage has
    /// different sampling and compression characteristics.
    ///
    /// Args:
    ///     dpi: Target resolution in dots per inch.
    fn mask_dpi(&mut self, dpi: f32) {
        self.mask_dpi = Some(dpi);
    }

    /// Set the target resolution for flattened background bitmaps.
    ///
    /// The default 192 DPI is the lowest tested physical-resolution baseline.
    /// This setting affects only synthetic background images (e.g. gradients
    /// with blend modes), never PDF geometry.
    ///
    /// Args:
    ///     dpi: Target resolution in dots per inch.
    fn background_raster_dpi(&mut self, dpi: f32) {
        self.background_dpi = Some(dpi);
    }

    /// Enable or disable occlusion culling of raster images.
    ///
    /// When enabled, images that are fully covered by a later fully-opaque
    /// rectangular element are skipped. Disabled by default. Conservative
    /// and safe: only skips images that are guaranteed invisible.
    ///
    /// Args:
    ///     enabled: Whether to enable occlusion culling.
    fn occlusion_cull(&mut self, enabled: bool) {
        self.occlusion_cull = Some(enabled);
    }

    /// Enable or disable HTML sanitization (enabled by default).
    ///
    /// When enabled, potentially dangerous HTML elements and attributes
    /// (e.g. `<script>`, event handlers) are stripped before conversion.
    ///
    /// Args:
    ///     enabled: Whether to sanitize input HTML.
    fn sanitize(&mut self, enabled: bool) {
        self.sanitize = Some(enabled);
    }

    /// Register a custom TrueType font.
    ///
    /// The name should match the `font-family` value used in CSS. The
    /// font data must be the raw contents of a `.ttf` file.
    ///
    /// Args:
    ///     name: Font family name (matched case-insensitively).
    ///     ttf_data: Raw TrueType font file contents.
    ///
    /// Example:
    ///     >>> converter = HtmlConverter()
    ///     >>> with open("MyFont.ttf", "rb") as f:
    ///     ...     converter.add_font("MyFont", f.read())
    ///     >>> pdf = converter.convert('<p style="font-family: MyFont">Hello</p>')
    fn add_font(&mut self, name: &str, ttf_data: Vec<u8>) {
        self.custom_fonts.push((name.to_string(), ttf_data));
    }

    /// Set the base directory for resolving relative paths.
    ///
    /// When set, CSS `@import "styles.css"` and `@font-face src: url(...)`
    /// paths are resolved relative to this directory. Only local file paths
    /// are supported; remote URLs (http/https) are rejected for security.
    ///
    /// Args:
    ///     path: Absolute path to the base directory.
    ///
    /// Example:
    ///     >>> converter = HtmlConverter()
    ///     >>> converter.base_path("/path/to/project")
    ///     >>> pdf = converter.convert('<style>@import "styles.css";</style><p>Hi</p>')
    fn base_path(&mut self, path: &str) {
        self.base_path = Some(path.to_string());
    }

    /// Set the directory boundary authorized for document-local resources.
    ///
    /// By default, the base_path is both the URL base and the authorization
    /// boundary. Set a broader root when a document legitimately references
    /// shared assets in an ancestor directory. The base path must remain
    /// inside this canonical root; traversal and symlink escapes are denied.
    ///
    /// Args:
    ///     path: Absolute path to the authorized resource root.
    fn resource_root(&mut self, path: &str) {
        self.resource_root = Some(path.to_string());
    }

    /// Set a header text rendered at the top of each page.
    ///
    /// The header is placed in the top margin area of every page.
    ///
    /// Args:
    ///     text: Header text string.
    fn header(&mut self, text: &str) {
        self.header = Some(text.to_string());
    }

    /// Set a footer text rendered at the bottom of each page.
    ///
    /// Use `{page}` for the current page number and `{pages}` for the
    /// total page count. For example: `"Page {page} of {pages}"`.
    ///
    /// Args:
    ///     text: Footer text string with optional placeholders.
    ///
    /// Example:
    ///     >>> converter = HtmlConverter()
    ///     >>> converter.footer("Page {page} of {pages}")
    fn footer(&mut self, text: &str) {
        self.footer = Some(text.to_string());
    }

    /// Convert HTML to PDF bytes.
    ///
    /// Args:
    ///     html: HTML string to convert.
    ///
    /// Returns:
    ///     PDF document as bytes.
    ///
    /// Raises:
    ///     ValueError: If conversion fails.
    fn convert(&self, html: &str) -> PyResult<Vec<u8>> {
        self.build_converter()
            .convert(html)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Convert Markdown to PDF bytes.
    ///
    /// Args:
    ///     markdown: Markdown string to convert.
    ///
    /// Returns:
    ///     PDF document as bytes.
    ///
    /// Raises:
    ///     ValueError: If conversion fails.
    fn convert_markdown(&self, markdown: &str) -> PyResult<Vec<u8>> {
        self.build_converter()
            .convert_markdown(markdown)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Convert HTML to PDF and write the result to a file.
    ///
    /// Args:
    ///     html: HTML string to convert.
    ///     path: Output file path.
    ///
    /// Raises:
    ///     ValueError: If conversion or file writing fails.
    fn convert_to_file(&self, html: &str, path: &str) -> PyResult<()> {
        let pdf = self
            .build_converter()
            .convert(html)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        std::fs::write(path, pdf).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Convert Markdown to PDF and write the result to a file.
    ///
    /// Args:
    ///     markdown: Markdown string to convert.
    ///     path: Output file path.
    ///
    /// Raises:
    ///     ValueError: If conversion or file writing fails.
    fn convert_markdown_to_file(&self, markdown: &str, path: &str) -> PyResult<()> {
        let pdf = self
            .build_converter()
            .convert_markdown(markdown)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        std::fs::write(path, pdf).map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

#[pymodule]
fn ironpress(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(html_to_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(markdown_to_pdf, m)?)?;
    m.add_class::<HtmlConverter>()?;
    Ok(())
}

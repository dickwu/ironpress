use std::cell::RefCell;
use std::path::Path;

use magnus::{
    Error, IntoValue, RString, Ruby, Value,
    class, define_module, function, method,
    prelude::*,
    typed_data,
};

fn html_to_pdf(ruby: &Ruby, html: String) -> Result<RString, Error> {
    let pdf = ironpress_core::html_to_pdf(&html)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
    Ok(ruby.str_from_slice(&pdf))
}

fn markdown_to_pdf(ruby: &Ruby, markdown: String) -> Result<RString, Error> {
    let pdf = ironpress_core::markdown_to_pdf(&markdown)
        .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
    Ok(ruby.str_from_slice(&pdf))
}

// ---------------------------------------------------------------------------
// Inner state for the builder -- stored inside a RefCell so magnus method
// receivers (&self) can mutate it.
// ---------------------------------------------------------------------------

struct ConverterInner {
    page_size: ironpress_core::types::PageSize,
    margin: ironpress_core::types::Margin,
    compress: Option<bool>,
    jpeg_quality: Option<u8>,
    auto_resize_images: Option<bool>,
    image_dpi: Option<f32>,
    filter_dpi: Option<f32>,
    mask_dpi: Option<f32>,
    background_raster_dpi: Option<f32>,
    occlusion_cull: Option<bool>,
    sanitize: Option<bool>,
    custom_fonts: Vec<(String, Vec<u8>)>,
    base_path: Option<String>,
    resource_root: Option<String>,
    header: Option<String>,
    footer: Option<String>,
}

impl Default for ConverterInner {
    fn default() -> Self {
        Self {
            page_size: ironpress_core::types::PageSize::default(),
            margin: ironpress_core::types::Margin::default(),
            compress: None,
            jpeg_quality: None,
            auto_resize_images: None,
            image_dpi: None,
            filter_dpi: None,
            mask_dpi: None,
            background_raster_dpi: None,
            occlusion_cull: None,
            sanitize: None,
            custom_fonts: Vec::new(),
            base_path: None,
            resource_root: None,
            header: None,
            footer: None,
        }
    }
}

impl ConverterInner {
    /// Build an `ironpress_core::HtmlConverter` from the accumulated state.
    fn build(&self) -> ironpress_core::HtmlConverter {
        let mut c = ironpress_core::HtmlConverter::new()
            .page_size(self.page_size)
            .margin(self.margin);

        if let Some(v) = self.compress {
            c = c.compress(v);
        }
        if let Some(v) = self.jpeg_quality {
            c = c.jpeg_quality(v);
        }
        if let Some(v) = self.auto_resize_images {
            c = c.auto_resize_images(v);
        }
        if let Some(v) = self.image_dpi {
            c = c.image_dpi(v);
        }
        if let Some(v) = self.filter_dpi {
            c = c.filter_dpi(v);
        }
        if let Some(v) = self.mask_dpi {
            c = c.mask_dpi(v);
        }
        if let Some(v) = self.background_raster_dpi {
            c = c.background_raster_dpi(v);
        }
        if let Some(v) = self.occlusion_cull {
            c = c.occlusion_cull(v);
        }
        if let Some(v) = self.sanitize {
            c = c.sanitize(v);
        }
        for (name, data) in &self.custom_fonts {
            c = c.add_font(name, data.clone());
        }
        if let Some(ref p) = self.base_path {
            c = c.base_path(Path::new(p));
        }
        if let Some(ref p) = self.resource_root {
            c = c.resource_root(Path::new(p));
        }
        if let Some(ref t) = self.header {
            c = c.header(t.clone());
        }
        if let Some(ref t) = self.footer {
            c = c.footer(t.clone());
        }
        c
    }
}

// ---------------------------------------------------------------------------
// Ruby-visible class: Ironpress::HtmlConverter
//
// All setter methods mutate the receiver and return `self` so calls can be
// chained:
//
//   converter = Ironpress::HtmlConverter.new
//                 .page_size("Letter")
//                 .margin(36.0)
//                 .compress(true)
//   pdf = converter.convert("<h1>Hello</h1>")
// ---------------------------------------------------------------------------

#[magnus::wrap(class = "Ironpress::HtmlConverter", free_immediately, size)]
struct RbHtmlConverter {
    inner: RefCell<ConverterInner>,
}

// Type alias for the typed Ruby object handle that can be both dereferenced
// to &RbHtmlConverter and converted back to a Ruby Value.
type RbSelf = typed_data::Obj<RbHtmlConverter>;

// --- helpers ---------------------------------------------------------------

fn parse_page_size(ruby: &Ruby, name: &str) -> Result<ironpress_core::types::PageSize, Error> {
    match name.to_lowercase().as_str() {
        "a4" => Ok(ironpress_core::types::PageSize::A4),
        "letter" => Ok(ironpress_core::types::PageSize::LETTER),
        "legal" => Ok(ironpress_core::types::PageSize::LEGAL),
        _ => Err(Error::new(
            ruby.exception_arg_error(),
            format!("Unknown page size: {name}"),
        )),
    }
}

// --- methods ---------------------------------------------------------------

impl RbHtmlConverter {
    /// `Ironpress::HtmlConverter.new` -- create with defaults.
    fn new() -> Self {
        Self {
            inner: RefCell::new(ConverterInner::default()),
        }
    }

    /// `page_size(name)` -- set page size by name ("A4", "Letter", "Legal").
    fn page_size(ruby: &Ruby, rb_self: RbSelf, name: String) -> Result<Value, Error> {
        let size = parse_page_size(ruby, &name)?;
        rb_self.inner.borrow_mut().page_size = size;
        Ok(rb_self.into_value_with(ruby))
    }

    /// `margin(points)` -- set uniform margin on all sides.
    fn margin(ruby: &Ruby, rb_self: RbSelf, points: f64) -> Value {
        rb_self.inner.borrow_mut().margin =
            ironpress_core::types::Margin::uniform(points as f32);
        rb_self.into_value_with(ruby)
    }

    /// `margin_sides(top, right, bottom, left)` -- set per-side margins.
    fn margin_sides(
        ruby: &Ruby,
        rb_self: RbSelf,
        top: f64,
        right: f64,
        bottom: f64,
        left: f64,
    ) -> Value {
        rb_self.inner.borrow_mut().margin = ironpress_core::types::Margin::new(
            top as f32,
            right as f32,
            bottom as f32,
            left as f32,
        );
        rb_self.into_value_with(ruby)
    }

    /// `compress(enabled)` -- enable/disable FlateDecode compression.
    fn compress(ruby: &Ruby, rb_self: RbSelf, enabled: bool) -> Value {
        rb_self.inner.borrow_mut().compress = Some(enabled);
        rb_self.into_value_with(ruby)
    }

    /// `jpeg_quality(quality)` -- JPEG quality 0-100.
    fn jpeg_quality(ruby: &Ruby, rb_self: RbSelf, quality: u8) -> Value {
        rb_self.inner.borrow_mut().jpeg_quality = Some(quality);
        rb_self.into_value_with(ruby)
    }

    /// `auto_resize_images(enabled)` -- enable/disable automatic image downscaling.
    fn auto_resize_images(ruby: &Ruby, rb_self: RbSelf, enabled: bool) -> Value {
        rb_self.inner.borrow_mut().auto_resize_images = Some(enabled);
        rb_self.into_value_with(ruby)
    }

    /// `image_dpi(dpi)` -- source image resolution (min 72, default 300).
    fn image_dpi(ruby: &Ruby, rb_self: RbSelf, dpi: f64) -> Value {
        rb_self.inner.borrow_mut().image_dpi = Some(dpi as f32);
        rb_self.into_value_with(ruby)
    }

    /// `filter_dpi(dpi)` -- blur/filter bitmap DPI.
    fn filter_dpi(ruby: &Ruby, rb_self: RbSelf, dpi: f64) -> Value {
        rb_self.inner.borrow_mut().filter_dpi = Some(dpi as f32);
        rb_self.into_value_with(ruby)
    }

    /// `mask_dpi(dpi)` -- CSS mask-image coverage DPI.
    fn mask_dpi(ruby: &Ruby, rb_self: RbSelf, dpi: f64) -> Value {
        rb_self.inner.borrow_mut().mask_dpi = Some(dpi as f32);
        rb_self.into_value_with(ruby)
    }

    /// `background_raster_dpi(dpi)` -- background bitmap DPI.
    fn background_raster_dpi(ruby: &Ruby, rb_self: RbSelf, dpi: f64) -> Value {
        rb_self.inner.borrow_mut().background_raster_dpi = Some(dpi as f32);
        rb_self.into_value_with(ruby)
    }

    /// `occlusion_cull(enabled)` -- enable/disable occlusion culling.
    fn occlusion_cull(ruby: &Ruby, rb_self: RbSelf, enabled: bool) -> Value {
        rb_self.inner.borrow_mut().occlusion_cull = Some(enabled);
        rb_self.into_value_with(ruby)
    }

    /// `sanitize(enabled)` -- enable/disable HTML sanitization.
    fn sanitize(ruby: &Ruby, rb_self: RbSelf, enabled: bool) -> Value {
        rb_self.inner.borrow_mut().sanitize = Some(enabled);
        rb_self.into_value_with(ruby)
    }

    /// `add_font(name, ttf_data)` -- register a custom TrueType font.
    fn add_font(ruby: &Ruby, rb_self: RbSelf, name: String, ttf_data: RString) -> Value {
        let data = unsafe { ttf_data.as_slice() }.to_vec();
        rb_self.inner.borrow_mut().custom_fonts.push((name, data));
        rb_self.into_value_with(ruby)
    }

    /// `base_path(path)` -- base directory for resolving relative paths.
    fn base_path(ruby: &Ruby, rb_self: RbSelf, path: String) -> Value {
        rb_self.inner.borrow_mut().base_path = Some(path);
        rb_self.into_value_with(ruby)
    }

    /// `resource_root(path)` -- authorized resource boundary directory.
    fn resource_root(ruby: &Ruby, rb_self: RbSelf, path: String) -> Value {
        rb_self.inner.borrow_mut().resource_root = Some(path);
        rb_self.into_value_with(ruby)
    }

    /// `header(text)` -- header text rendered at the top of each page.
    fn header(ruby: &Ruby, rb_self: RbSelf, text: String) -> Value {
        rb_self.inner.borrow_mut().header = Some(text);
        rb_self.into_value_with(ruby)
    }

    /// `footer(text)` -- footer text rendered at the bottom of each page.
    /// Use `{page}` for current page number and `{pages}` for total count.
    fn footer(ruby: &Ruby, rb_self: RbSelf, text: String) -> Value {
        rb_self.inner.borrow_mut().footer = Some(text);
        rb_self.into_value_with(ruby)
    }

    /// `convert(html)` -- convert HTML to PDF bytes.
    fn convert(ruby: &Ruby, rb_self: RbSelf, html: String) -> Result<RString, Error> {
        let converter = rb_self.inner.borrow().build();
        let pdf = converter
            .convert(&html)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        Ok(ruby.str_from_slice(&pdf))
    }

    /// `convert_markdown(markdown)` -- convert Markdown to PDF bytes.
    fn convert_markdown(ruby: &Ruby, rb_self: RbSelf, markdown: String) -> Result<RString, Error> {
        let converter = rb_self.inner.borrow().build();
        let pdf = converter
            .convert_markdown(&markdown)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        Ok(ruby.str_from_slice(&pdf))
    }

    /// `convert_to_file(html, path)` -- convert HTML and write PDF to a file.
    fn convert_to_file(
        ruby: &Ruby,
        rb_self: RbSelf,
        html: String,
        path: String,
    ) -> Result<Value, Error> {
        let converter = rb_self.inner.borrow().build();
        let pdf = converter
            .convert(&html)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        std::fs::write(&path, &pdf)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        Ok(rb_self.into_value_with(ruby))
    }

    /// `convert_markdown_to_file(markdown, path)` -- convert Markdown and write PDF to a file.
    fn convert_markdown_to_file(
        ruby: &Ruby,
        rb_self: RbSelf,
        markdown: String,
        path: String,
    ) -> Result<Value, Error> {
        let converter = rb_self.inner.borrow().build();
        let pdf = converter
            .convert_markdown(&markdown)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        std::fs::write(&path, &pdf)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), e.to_string()))?;
        Ok(rb_self.into_value_with(ruby))
    }
}

#[magnus::init]
fn init(_ruby: &Ruby) -> Result<(), Error> {
    let module = define_module("Ironpress")?;

    // Module-level convenience functions (existing API).
    module.define_singleton_method("html_to_pdf", function!(html_to_pdf, 1))?;
    module.define_singleton_method("markdown_to_pdf", function!(markdown_to_pdf, 1))?;

    // HtmlConverter class under the Ironpress module.
    let klass = module.define_class("HtmlConverter", class::object())?;
    klass.define_singleton_method("new", function!(RbHtmlConverter::new, 0))?;
    klass.define_method("page_size", method!(RbHtmlConverter::page_size, 1))?;
    klass.define_method("margin", method!(RbHtmlConverter::margin, 1))?;
    klass.define_method("margin_sides", method!(RbHtmlConverter::margin_sides, 4))?;
    klass.define_method("compress", method!(RbHtmlConverter::compress, 1))?;
    klass.define_method("jpeg_quality", method!(RbHtmlConverter::jpeg_quality, 1))?;
    klass.define_method(
        "auto_resize_images",
        method!(RbHtmlConverter::auto_resize_images, 1),
    )?;
    klass.define_method("image_dpi", method!(RbHtmlConverter::image_dpi, 1))?;
    klass.define_method("filter_dpi", method!(RbHtmlConverter::filter_dpi, 1))?;
    klass.define_method("mask_dpi", method!(RbHtmlConverter::mask_dpi, 1))?;
    klass.define_method(
        "background_raster_dpi",
        method!(RbHtmlConverter::background_raster_dpi, 1),
    )?;
    klass.define_method("occlusion_cull", method!(RbHtmlConverter::occlusion_cull, 1))?;
    klass.define_method("sanitize", method!(RbHtmlConverter::sanitize, 1))?;
    klass.define_method("add_font", method!(RbHtmlConverter::add_font, 2))?;
    klass.define_method("base_path", method!(RbHtmlConverter::base_path, 1))?;
    klass.define_method("resource_root", method!(RbHtmlConverter::resource_root, 1))?;
    klass.define_method("header", method!(RbHtmlConverter::header, 1))?;
    klass.define_method("footer", method!(RbHtmlConverter::footer, 1))?;
    klass.define_method("convert", method!(RbHtmlConverter::convert, 1))?;
    klass.define_method(
        "convert_markdown",
        method!(RbHtmlConverter::convert_markdown, 1),
    )?;
    klass.define_method(
        "convert_to_file",
        method!(RbHtmlConverter::convert_to_file, 2),
    )?;
    klass.define_method(
        "convert_markdown_to_file",
        method!(RbHtmlConverter::convert_markdown_to_file, 2),
    )?;

    Ok(())
}

# frozen_string_literal: true

require_relative "ironpress/version"
require_relative "ironpress/ironpress_ruby"

module Ironpress
  # Mutable Ruby facade over immutable native converter values.
  class HtmlConverter
    CONFIGURATION_METHODS = %i[
      page_size
      page_size_custom
      margin
      margin_sides
      compress
      jpeg_quality
      auto_resize_images
      image_dpi
      filter_dpi
      mask_dpi
      background_raster_dpi
      occlusion_cull
      sanitize
      add_font
      add_font_pack
      base_path
      resource_root
      header
      footer
    ].freeze

    def initialize
      @native = NativeConverter.new
    end

    CONFIGURATION_METHODS.each do |name|
      define_method(name) do |*arguments|
        @native = @native.public_send(name, *arguments)
        self
      end
    end

    def convert(html)
      @native.convert(html)
    end

    def convert_markdown(markdown)
      @native.convert_markdown(markdown)
    end

    def convert_to_file(html, path)
      @native.convert_to_file(html, path)
      self
    end

    def convert_markdown_to_file(markdown, path)
      @native.convert_markdown_to_file(markdown, path)
      self
    end
  end

  private_constant :NativeConverter
end

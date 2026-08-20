# frozen_string_literal: true

require_relative "lib/ironpress/version"

Gem::Specification.new do |spec|
  spec.name = "ironpress"
  spec.version = Ironpress::VERSION
  spec.authors = ["Paul Gaston Gouron"]
  spec.summary = "Pure Rust HTML/CSS/Markdown to PDF converter"
  spec.description = <<~DESCRIPTION.strip
    Convert HTML, CSS, and Markdown to PDF with no browser or system
    dependencies. Native Rust extension for maximum performance.
  DESCRIPTION
  spec.homepage = "https://github.com/gastongouron/ironpress"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.0"

  spec.files = Dir[
    "lib/**/*.rb",
    "ext/**/*.{rb,rs,toml}",
    "README.md"
  ]
  spec.extensions = ["ext/ironpress/extconf.rb"]
  spec.require_paths = ["lib"]

  spec.add_dependency "rb_sys", "~> 0.9"
end

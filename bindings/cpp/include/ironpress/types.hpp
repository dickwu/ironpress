#pragma once

#include "ironpress.h"

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace ironpress {

namespace detail {
class NativeResult;
}

/// Stable machine-readable Ironpress failure category.
enum class Status : IronpressStatus {
    unknown = -1,
    invalid_argument = IRONPRESS_STATUS_INVALID_ARGUMENT,
    invalid_utf8 = IRONPRESS_STATUS_INVALID_UTF8,
    invalid_enum = IRONPRESS_STATUS_INVALID_ENUM,
    invalid_handle = IRONPRESS_STATUS_INVALID_HANDLE,
    output_not_empty = IRONPRESS_STATUS_OUTPUT_NOT_EMPTY,
    parse = IRONPRESS_STATUS_PARSE,
    css = IRONPRESS_STATUS_CSS,
    layout = IRONPRESS_STATUS_LAYOUT,
    render = IRONPRESS_STATUS_RENDER,
    font = IRONPRESS_STATUS_FONT,
    io = IRONPRESS_STATUS_IO,
    security = IRONPRESS_STATUS_SECURITY,
    internal = IRONPRESS_STATUS_INTERNAL,
};

/// A categorized Ironpress failure copied from the native error owner.
class Error final : public std::runtime_error {
public:
    /// Return the stable category, or `unknown` for a newer native status.
    [[nodiscard]] Status status() const noexcept { return status_; }

    /// Return the exact status emitted by the linked native library.
    [[nodiscard]] IronpressStatus native_status() const noexcept {
        return native_status_;
    }

private:
    Error(Status status, IronpressStatus native_status, std::string message)
        : std::runtime_error(std::move(message)),
          status_(status),
          native_status_(native_status) {}

    Status status_;
    IronpressStatus native_status_;

    friend class detail::NativeResult;
};

/// Named physical page sizes understood by Ironpress.
enum class PageSize : std::uint32_t {
    a4 = IRONPRESS_PAGE_SIZE_A4,
    letter = IRONPRESS_PAGE_SIZE_LETTER,
    legal = IRONPRESS_PAGE_SIZE_LEGAL,
};

/// The role of an optional Ironpress fallback-font pack.
enum class FontPackKind : std::uint32_t {
    cjk_japanese = IRONPRESS_FONT_PACK_CJK_JAPANESE,
    cjk_korean = IRONPRESS_FONT_PACK_CJK_KOREAN,
    cjk_simplified_chinese = IRONPRESS_FONT_PACK_CJK_SIMPLIFIED_CHINESE,
    cjk_traditional_chinese = IRONPRESS_FONT_PACK_CJK_TRADITIONAL_CHINESE,
    emoji = IRONPRESS_FONT_PACK_EMOJI,
};

/// A borrowed byte range that remains owned by the caller.
class BytesView final {
public:
    /// Borrow one pointer-plus-size range for the duration of a native call.
    BytesView(const std::uint8_t* data, std::size_t size)
        : value_{data, size} {
        if (data == nullptr && size != 0) {
            throw std::invalid_argument(
                "byte data must not be null when its size is non-zero");
        }
    }

    /// Borrow a byte vector without copying it.
    explicit BytesView(const std::vector<std::uint8_t>& bytes) noexcept
        : value_{bytes.data(), bytes.size()} {}

    /// Return the borrowed data pointer.
    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return value_.data;
    }

    /// Return the borrowed byte count.
    [[nodiscard]] std::size_t size() const noexcept { return value_.len; }

private:
    [[nodiscard]] IronpressBytes native() const noexcept { return value_; }

    IronpressBytes value_;

    friend class Converter;
};

/// A positive finite custom page size measured in points.
class PageDimensions final {
public:
    /// Parse physical point dimensions into a valid custom page size.
    [[nodiscard]] static PageDimensions from_points(float width, float height) {
        if (!std::isfinite(width) || width <= 0.0F) {
            throw std::invalid_argument("page width must be positive and finite");
        }
        if (!std::isfinite(height) || height <= 0.0F) {
            throw std::invalid_argument("page height must be positive and finite");
        }
        return PageDimensions(width, height);
    }

    /// Return the physical width in points.
    [[nodiscard]] float width() const noexcept { return width_; }

    /// Return the physical height in points.
    [[nodiscard]] float height() const noexcept { return height_; }

private:
    PageDimensions(float width, float height) noexcept
        : width_(width), height_(height) {}

    float width_;
    float height_;
};

/// Finite physical page margins in CSS clockwise order.
class PageMargins final {
public:
    /// Parse four equal physical margins.
    [[nodiscard]] static PageMargins uniform(float points) {
        return from_points(points, points, points, points);
    }

    /// Parse physical margins in top, right, bottom, left order.
    [[nodiscard]] static PageMargins from_points(float top,
                                                 float right,
                                                 float bottom,
                                                 float left) {
        if (!std::isfinite(top) || !std::isfinite(right) ||
            !std::isfinite(bottom) || !std::isfinite(left)) {
            throw std::invalid_argument("page margins must be finite");
        }
        return PageMargins(top, right, bottom, left);
    }

    /// Return the top margin in points.
    [[nodiscard]] float top() const noexcept { return top_; }

    /// Return the right margin in points.
    [[nodiscard]] float right() const noexcept { return right_; }

    /// Return the bottom margin in points.
    [[nodiscard]] float bottom() const noexcept { return bottom_; }

    /// Return the left margin in points.
    [[nodiscard]] float left() const noexcept { return left_; }

private:
    PageMargins(float top, float right, float bottom, float left) noexcept
        : top_(top), right_(right), bottom_(bottom), left_(left) {}

    float top_;
    float right_;
    float bottom_;
    float left_;
};

}

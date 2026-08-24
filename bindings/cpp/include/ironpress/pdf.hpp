#pragma once

#include "ironpress/detail/native.hpp"

#include <cstddef>
#include <cstdint>
#include <string_view>
#include <utility>

namespace ironpress {

/// One uniquely owned PDF allocation returned by Ironpress.
class Pdf final {
public:
    Pdf() = delete;
    Pdf(const Pdf&) = delete;
    Pdf& operator=(const Pdf&) = delete;
    Pdf(Pdf&&) noexcept = default;
    Pdf& operator=(Pdf&&) noexcept = default;
    ~Pdf() noexcept = default;

    /// Borrow the first PDF byte for the lifetime of this owner.
    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return ironpress_buffer_data(owner_.get());
    }

    /// Return the PDF byte count.
    [[nodiscard]] std::size_t size() const noexcept {
        return ironpress_buffer_len(owner_.get());
    }

    /// Report whether this object still owns a native PDF buffer.
    [[nodiscard]] explicit operator bool() const noexcept {
        return static_cast<bool>(owner_);
    }

private:
    explicit Pdf(detail::BufferOwner owner) noexcept : owner_(std::move(owner)) {}

    [[nodiscard]] static Pdf take(IronpressStatus status,
                                  IronpressBuffer* raw_buffer,
                                  IronpressError* raw_error) {
        detail::BufferOwner buffer(raw_buffer);
        detail::NativeResult::require_success(status, raw_error);
        if (!buffer) {
            throw detail::NativeResult::contract_violation(
                "Native conversion returned no PDF owner.");
        }
        return Pdf(std::move(buffer));
    }

    detail::BufferOwner owner_;

    friend class Converter;
    friend Pdf html_to_pdf(std::string_view html);
    friend Pdf markdown_to_pdf(std::string_view markdown);
};

}

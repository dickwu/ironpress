#pragma once

#include "ironpress/types.hpp"

#include <memory>
#include <string>
#include <string_view>
#include <utility>

namespace ironpress::detail {

struct BufferDeleter final {
    void operator()(IronpressBuffer* buffer) const noexcept {
        if (buffer != nullptr) {
            (void)ironpress_buffer_free(&buffer);
        }
    }
};

struct ConverterDeleter final {
    void operator()(IronpressConverter* converter) const noexcept {
        if (converter != nullptr) {
            (void)ironpress_converter_free(&converter);
        }
    }
};

struct ErrorDeleter final {
    void operator()(IronpressError* error) const noexcept {
        if (error != nullptr) {
            (void)ironpress_error_free(&error);
        }
    }
};

using BufferOwner = std::unique_ptr<IronpressBuffer, BufferDeleter>;
using ConverterOwner = std::unique_ptr<IronpressConverter, ConverterDeleter>;
using ErrorOwner = std::unique_ptr<IronpressError, ErrorDeleter>;

/// Parses native results once and releases every native error owner.
class NativeResult final {
public:
    static void require_compatible_abi() {
        const auto linked_version = ironpress_abi_version();
        if (linked_version != IRONPRESS_ABI_VERSION) {
            throw contract_violation(
                "Ironpress C++ requires ABI " +
                std::to_string(IRONPRESS_ABI_VERSION) + ", but ABI " +
                std::to_string(linked_version) + " was loaded.");
        }
    }

    static void require_success(IronpressStatus status,
                                IronpressError* raw_error) {
        ErrorOwner error(raw_error);
        if (status == IRONPRESS_STATUS_OK && !error) {
            return;
        }
        if (status == IRONPRESS_STATUS_OK) {
            throw contract_violation(
                "A successful native call returned an error owner.");
        }
        throw Error(category(status), status, message(error.get(), status));
    }

    [[nodiscard]] static Error contract_violation(std::string message) {
        return Error(Status::internal, IRONPRESS_STATUS_INTERNAL,
                     std::move(message));
    }

private:
    [[nodiscard]] static Status category(IronpressStatus status) noexcept {
        switch (status) {
            case IRONPRESS_STATUS_INVALID_ARGUMENT:
                return Status::invalid_argument;
            case IRONPRESS_STATUS_INVALID_UTF8:
                return Status::invalid_utf8;
            case IRONPRESS_STATUS_INVALID_ENUM:
                return Status::invalid_enum;
            case IRONPRESS_STATUS_INVALID_HANDLE:
                return Status::invalid_handle;
            case IRONPRESS_STATUS_OUTPUT_NOT_EMPTY:
                return Status::output_not_empty;
            case IRONPRESS_STATUS_PARSE:
                return Status::parse;
            case IRONPRESS_STATUS_CSS:
                return Status::css;
            case IRONPRESS_STATUS_LAYOUT:
                return Status::layout;
            case IRONPRESS_STATUS_RENDER:
                return Status::render;
            case IRONPRESS_STATUS_FONT:
                return Status::font;
            case IRONPRESS_STATUS_IO:
                return Status::io;
            case IRONPRESS_STATUS_SECURITY:
                return Status::security;
            case IRONPRESS_STATUS_INTERNAL:
                return Status::internal;
            default:
                return Status::unknown;
        }
    }

    [[nodiscard]] static std::string message(const IronpressError* error,
                                             IronpressStatus status) {
        if (error == nullptr) {
            return "Ironpress failed with native status " +
                   std::to_string(status) + ".";
        }
        const auto size = ironpress_error_message_len(error);
        const auto* data = ironpress_error_message_data(error);
        if (data == nullptr || size == 0) {
            return "Ironpress returned an empty native diagnostic.";
        }
        return {reinterpret_cast<const char*>(data), size};
    }
};

[[nodiscard]] inline IronpressBytes text(std::string_view value) noexcept {
    return {reinterpret_cast<const std::uint8_t*>(value.data()), value.size()};
}

}

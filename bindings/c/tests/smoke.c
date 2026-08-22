#include "ironpress.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void require(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "%s\n", message);
        exit(EXIT_FAILURE);
    }
}

static IronpressBytes text(const char *value) {
    IronpressBytes bytes = {
        .data = (const uint8_t *)value,
        .len = strlen(value),
    };
    return bytes;
}

int main(void) {
    IronpressConverter *converter = NULL;
    IronpressBuffer *pdf = NULL;
    IronpressError *error = NULL;

    require(ironpress_abi_version() == IRONPRESS_ABI_VERSION,
            "ABI version query disagrees with the header");
    require(strlen(ironpress_version()) > 0, "package version is empty");

    require(ironpress_converter_new(&converter, &error) == IRONPRESS_STATUS_OK,
            "converter allocation failed");
    require(converter != NULL && error == NULL, "converter ownership is invalid");

    require(ironpress_converter_set_page_size(
                converter, IRONPRESS_PAGE_SIZE_LETTER, &error) == IRONPRESS_STATUS_OK,
            "named page size failed");
    require(ironpress_converter_set_margins(converter, 36.0f, 36.0f, 36.0f,
                                            36.0f, &error) == IRONPRESS_STATUS_OK,
            "page margins failed");
    require(ironpress_converter_set_header(converter, text("C ABI"), &error) ==
                IRONPRESS_STATUS_OK,
            "header configuration failed");
    require(ironpress_converter_set_footer(
                converter, text("Page {page} / {pages}"), &error) ==
                IRONPRESS_STATUS_OK,
            "footer configuration failed");

    require(ironpress_converter_convert_html(
                converter, text("<h1>Hello from C</h1>"), &pdf, &error) ==
                IRONPRESS_STATUS_OK,
            "HTML conversion failed");
    require(pdf != NULL && error == NULL, "conversion ownership is invalid");
    require(ironpress_buffer_len(pdf) > 4, "PDF output is too short");
    require(memcmp(ironpress_buffer_data(pdf), "%PDF", 4) == 0,
            "conversion did not return a PDF");

    require(ironpress_buffer_free(&pdf) == IRONPRESS_STATUS_OK,
            "PDF release failed");
    require(pdf == NULL, "PDF release did not clear the handle");
    require(ironpress_buffer_free(&pdf) == IRONPRESS_STATUS_OK,
            "repeated PDF release was not safe");

    const uint8_t invalid_utf8[] = {0xff};
    IronpressBytes invalid = {.data = invalid_utf8, .len = sizeof(invalid_utf8)};
    require(ironpress_converter_convert_html(converter, invalid, &pdf, &error) ==
                IRONPRESS_STATUS_INVALID_UTF8,
            "invalid UTF-8 returned the wrong status");
    require(pdf == NULL && error != NULL, "invalid UTF-8 did not return an error");
    require(ironpress_error_status(error) == IRONPRESS_STATUS_INVALID_UTF8,
            "error handle returned the wrong status");
    require(ironpress_error_message_len(error) > 0, "error message is empty");
    require(ironpress_error_free(&error) == IRONPRESS_STATUS_OK,
            "error release failed");
    require(ironpress_error_free(&error) == IRONPRESS_STATUS_OK,
            "repeated error release was not safe");

    require(ironpress_converter_set_page_size(converter, UINT32_MAX, &error) ==
                IRONPRESS_STATUS_INVALID_ENUM,
            "invalid page-size discriminant returned the wrong status");
    require(ironpress_error_free(&error) == IRONPRESS_STATUS_OK,
            "enum error release failed");

    require(ironpress_converter_free(&converter) == IRONPRESS_STATUS_OK,
            "converter release failed");
    require(converter == NULL, "converter release did not clear the handle");
    require(ironpress_converter_free(&converter) == IRONPRESS_STATUS_OK,
            "repeated converter release was not safe");

    return EXIT_SUCCESS;
}

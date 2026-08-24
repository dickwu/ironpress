#include "ironpress.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    static const char source[] = "<h1>Installed C package</h1>";
    const IronpressBytes html = {
        .data = (const uint8_t *)source,
        .len = sizeof(source) - 1,
    };
    IronpressBuffer *pdf = NULL;
    IronpressError *error = NULL;

    if (ironpress_html_to_pdf(html, &pdf, &error) != IRONPRESS_STATUS_OK) {
        return EXIT_FAILURE;
    }
    if (ironpress_buffer_len(pdf) < 4 ||
        memcmp(ironpress_buffer_data(pdf), "%PDF", 4) != 0) {
        return EXIT_FAILURE;
    }
    if (ironpress_buffer_free(&pdf) != IRONPRESS_STATUS_OK) {
        return EXIT_FAILURE;
    }
    return EXIT_SUCCESS;
}

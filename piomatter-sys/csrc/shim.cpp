// C interface over Piomatter's C++ core: create, show, fps, destroy.
// Every exception is caught at the boundary and returned as an error
// string; unwinding into Rust would be undefined behaviour. piolib
// still calls exit(1) if /dev/pio0 cannot be opened, so the caller
// checks the device exists before calling piomatter_create.

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <iterator>
#include <map>
#include <memory>
#include <span>
#include <stdexcept>
#include <string>
#include <vector>

#include "piomatter/piomatter.h"

struct piomatter_config {
    uint32_t width;
    uint32_t height;
    uint32_t n_addr_lines;
    uint32_t n_planes;
    uint32_t n_temporal_planes;
    uint32_t n_lanes;
    uint32_t pinout;  // 0 bonnet, 1 bonnet BGR, 2 active3, 3 active3 BGR
    const int32_t *map;
    size_t map_len;
};

struct piomatter_handle {
    std::unique_ptr<piomatter::piomatter_base> driver;
};

static void set_error(char *err, size_t err_len, const char *message) {
    if (err != nullptr && err_len > 0) {
        std::strncpy(err, message, err_len - 1);
        err[err_len - 1] = '\0';
    }
}

template <class pinout>
static piomatter::piomatter_base *make(const piomatter_config &cfg,
                                       std::span<const uint32_t> framebuffer,
                                       const piomatter::matrix_geometry &geometry) {
    return new piomatter::piomatter<pinout, piomatter::colorspace_rgb888>(framebuffer, geometry);
}

extern "C" piomatter_handle *piomatter_create(const piomatter_config *cfg, const uint32_t *framebuffer,
                                              size_t framebuffer_len, char *err, size_t err_len) {
    try {
        if (cfg == nullptr || framebuffer == nullptr || cfg->map == nullptr) {
            throw std::invalid_argument("null config, framebuffer or map");
        }
        if (framebuffer_len != size_t(cfg->width) * cfg->height) {
            throw std::invalid_argument("framebuffer length does not match width * height");
        }
        size_t pixels_down = size_t(cfg->n_lanes) << cfg->n_addr_lines;
        if (pixels_down == 0 || (size_t(cfg->width) * cfg->height) % pixels_down != 0) {
            throw std::invalid_argument("width * height is not a multiple of lanes << address lines");
        }
        size_t pixels_across = size_t(cfg->width) * cfg->height / pixels_down;
        piomatter::matrix_map map(cfg->map, cfg->map + cfg->map_len);
        piomatter::matrix_geometry geometry(pixels_across, cfg->n_addr_lines, int(cfg->n_planes),
                                            int(cfg->n_temporal_planes), cfg->width, cfg->height,
                                            map, cfg->n_lanes);
        std::span<const uint32_t> span(framebuffer, framebuffer_len);
        piomatter::piomatter_base *driver = nullptr;
        switch (cfg->pinout) {
        case 0: driver = make<piomatter::adafruit_matrix_bonnet_pinout>(*cfg, span, geometry); break;
        case 1: driver = make<piomatter::adafruit_matrix_bonnet_pinout_bgr>(*cfg, span, geometry); break;
        case 2: driver = make<piomatter::active3_pinout>(*cfg, span, geometry); break;
        case 3: driver = make<piomatter::active3_pinout_bgr>(*cfg, span, geometry); break;
        default: throw std::invalid_argument("unknown pinout");
        }
        auto *handle = new piomatter_handle;
        handle->driver.reset(driver);
        return handle;
    } catch (const std::exception &e) {
        set_error(err, err_len, e.what());
    } catch (...) {
        set_error(err, err_len, "unknown C++ exception");
    }
    return nullptr;
}

extern "C" int piomatter_show(piomatter_handle *h) {
    try {
        return h->driver->show();
    } catch (...) {
        return -1;
    }
}

extern "C" double piomatter_fps(const piomatter_handle *h) {
    return h->driver->fps;
}

extern "C" void piomatter_destroy(piomatter_handle *h) {
    delete h;
}

/* Smoke test for the pathlight C ABI: builds a path, validates arguments and
 * calls every draw entry point. Exits non-zero if the ABI misbehaves.
 * Drawing calls currently report PATHLIGHT_STATUS_UNIMPLEMENTED. */

#include <math.h>
#include <pathlight.h>
#include <stdio.h>
#include <stdlib.h>

#define W 64
#define H 64

static int failures = 0;

static void expect(const char *what, PathlightStatus got, PathlightStatus want) {
    if (got != want) {
        fprintf(stderr, "FAIL %s: got status %d, want %d\n", what, got, want);
        failures++;
    }
}

int main(void) {
    printf("pathlight %s\n", pathlight_version());

    static uint8_t pixels[W * H * 4];
    PathlightSurface surface = {pixels, W, H, W * 4, PATHLIGHT_FORMAT_RGBA8};
    PathlightPaint red = {PATHLIGHT_PAINT_SOLID, {1.0f, 0.0f, 0.0f, 1.0f}, NULL};

    PathlightPath *path = pathlight_path_new();
    expect("move_to", pathlight_path_move_to(path, 8, 8), PATHLIGHT_STATUS_OK);
    expect("line_to", pathlight_path_line_to(path, 56, 8), PATHLIGHT_STATUS_OK);
    expect("quad_to", pathlight_path_quad_to(path, 56, 56, 8, 56), PATHLIGHT_STATUS_OK);
    expect("close", pathlight_path_close(path), PATHLIGHT_STATUS_OK);

    PathlightPoint spline[] = {{10, 40}, {20, 10}, {40, 10}, {50, 40}, {30, 60}};
    expect("bspline", pathlight_path_bspline(path, spline, 5, true), PATHLIGHT_STATUS_OK);
    expect("bspline too short", pathlight_path_bspline(path, spline, 3, false), PATHLIGHT_STATUS_INVALID_ARGUMENT);

    expect("fill", pathlight_fill(&surface, path, PATHLIGHT_FILL_NONZERO, &red, NULL, PATHLIGHT_BLEND_MULTIPLY, NULL),
           PATHLIGHT_STATUS_UNIMPLEMENTED);
    expect("fill bad blend", pathlight_fill(&surface, path, PATHLIGHT_FILL_NONZERO, &red, NULL, 99, NULL),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);

    PathlightSurface short_surface = surface;
    short_surface.stride = 8;
    expect("fill bad stride", pathlight_fill(&short_surface, path, PATHLIGHT_FILL_NONZERO, &red, NULL, 0, NULL),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);

    static uint8_t mask_pixels[W * H];
    PathlightSurface mask = {mask_pixels, W, H, W, PATHLIGHT_FORMAT_A8};
    float dashes[] = {4.0f, 2.0f};
    PathlightStrokeStyle style = {2.0f, PATHLIGHT_CAP_ROUND, PATHLIGHT_JOIN_BEVEL, 4.0f, dashes, 2, 0.0f};
    expect("stroke", pathlight_stroke(&surface, path, &style, &red, NULL, PATHLIGHT_BLEND_NORMAL, &mask),
           PATHLIGHT_STATUS_UNIMPLEMENTED);
    expect("stroke rgba clip", pathlight_stroke(&surface, path, &style, &red, NULL, 0, &surface),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);

    /* Last row only needs width * 4 bytes, not a full stride: a tightly sized
     * buffer must be accepted and never read past. */
    uint8_t *tight = malloc(16 * 1 + 2 * 4);
    PathlightSurface tight_surface = {tight, 2, 2, 16, PATHLIGHT_FORMAT_RGBA8};
    expect("fill tight last row", pathlight_fill(&tight_surface, path, PATHLIGHT_FILL_NONZERO, &red, NULL, 0, NULL),
           PATHLIGHT_STATUS_UNIMPLEMENTED);
    free(tight);

    /* A clip may not share bytes with the target it clips. */
    expect("fill clip aliases target", pathlight_fill(&mask, path, PATHLIGHT_FILL_NONZERO, &red, NULL, 0, &mask),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);
    PathlightSurface half_mask = {mask_pixels + (W * H) / 2, W, H / 2, W, PATHLIGHT_FORMAT_A8};
    expect("fill clip overlaps target", pathlight_fill(&mask, path, PATHLIGHT_FILL_NONZERO, &red, NULL, 0, &half_mask),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);

    PathlightPaint nan_paint = {PATHLIGHT_PAINT_SOLID, {NAN, 0.0f, 0.0f, 1.0f}, NULL};
    expect("fill NaN color", pathlight_fill(&surface, path, PATHLIGHT_FILL_NONZERO, &nan_paint, NULL, 0, NULL),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);
    PathlightPath *inf_path = pathlight_path_new();
    pathlight_path_move_to(inf_path, 0, 0);
    pathlight_path_line_to(inf_path, INFINITY, 4);
    expect("fill infinite point", pathlight_fill(&surface, inf_path, PATHLIGHT_FILL_NONZERO, &red, NULL, 0, NULL),
           PATHLIGHT_STATUS_INVALID_ARGUMENT);
    pathlight_path_free(inf_path);

    PathlightFont *font = NULL;
    const uint8_t junk[] = "not a font";
    expect("font junk", pathlight_font_new(junk, sizeof junk, 0, &font), PATHLIGHT_STATUS_INVALID_FONT);

    pathlight_path_free(path);

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    printf("all checks passed\n");
    return EXIT_SUCCESS;
}

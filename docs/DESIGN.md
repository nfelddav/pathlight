# pathlight design

pathlight is a lightweight 2D path renderer. "Lightweight" means minimal
memory and CPU use and a small API, while keeping SIMD acceleration and GPU
offloading possible later. It supports a small set of operations well rather
than everything a browser canvas can do.

These decisions were settled before any code was written. Changing one is a
design change, not a refactor; record the new decision here.

## Scope of v1

| Area | Decision | Why |
| --- | --- | --- |
| Rasterizer | Written from scratch, not wrapping tiny-skia, Blend2D or vello_cpu | Owning the rasterizer is the point of the project |
| Algorithm | Sparse strips: per-tile coverage only where edges touch, solid spans elsewhere | Memory scales with outline length, not area; fixed tile layout maps to SIMD now and GPU compute later |
| Geometry | Lines, quadratic and cubic Béziers. Uniform cubic B-splines are converted to cubics when added to a path | Real B-spline input while the rasterizer only sees Béziers. No knot vectors or NURBS |
| Fill rules | Non-zero and even-odd | |
| Strokes | Width, butt/round/square caps, miter/round/bevel joins with a miter limit, dashing | "Non-filled shapes" means strokes; dashing is painful for every binding to redo. No hairline special case, no correction for non-uniform transforms |
| Output | CPU only, into caller-owned buffers: RGBA8 and BGRA8 premultiplied, and A8 | RGBA/BGRA cover common surfaces; A8 is nearly free and serves masks and glyph caches. No float formats yet |
| Clipping | Arbitrary path clips as an A8 mask passed to any draw call | Reuses A8 surfaces; one optional argument |
| Paint | Solid colors only. `Paint` is an enum in Rust and a tagged struct in C | Gradients and images can be added without breaking the ABI |
| Blending | Source-over plus every separable CSS blend mode (multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion) | Matches CSS `mix-blend-mode`. Non-separable modes (hue, saturation, color, luminosity) need per-pixel HSL math and are out |
| Color space | Blending on sRGB-encoded values | Same results as CSS and browsers |
| Text | One line of shaped text with bidi and script itemization | Mixed-direction text renders correctly. No line breaking, paragraph layout or font fallback |
| Text stack | harfrust (shaping), skrifa (outlines), unicode-bidi, unicode-script | Pure Rust, so `cargo build` alone builds everything, including for WASM. Glyphs go through the same rasterizer as paths |
| Fonts | Caller passes font bytes. No filesystem access, no system font lookup | Avoids three platform backends and environment-dependent bugs |

## Architecture

```
PathBuilder ──► Path (lines, quads, cubics)
                  │  transform, flatten to lines (device space)
                  ▼
           sparse strips (tile coverage + solid spans)
                  │  × clip mask coverage
                  ▼
           composite: paint × coverage, blend mode, source-over
                  │
                  ▼
           caller's RGBA8 / BGRA8 / A8 buffer
```

Details of the current implementation (`crates/pathlight/src/raster/`):

- Curves are flattened in device space to within 0.1 px, so tolerance does
  not depend on the transform. A curve whose control points are all off the
  surface is replaced by its chord.
- Lines are clipped against the left and right edges: parts left of the
  surface become vertical lines at x = 0 (they still change winding to their
  right), parts to the right are dropped.
- Each line adds signed area contributions ("cells") only for the pixels it
  crosses. Cells are sorted by (band, x, row), with bands 4 rows tall.
- A sweep over each band turns runs of 4×4 tiles that contain cells into
  strips with per-pixel alpha, and the gaps between them into fills with one
  coverage value per row. A wide shape costs two strips and a fill per band,
  not one alpha value per pixel.
- Coverage maps to alpha by fill rule: non-zero clamps |winding area| to 1,
  even-odd folds it modulo 2. Overlapping antialiased edges are approximate,
  as in FreeType and font-rs.
- The compositor writes opaque source-over spans directly as bytes and sends
  everything else through the blend formulas.
- Strokes (`crates/pathlight/src/stroker.rs`) are flattened in user space at
  the fill tolerance divided by the transform's largest scale, dashed, and
  emitted as a union of same-orientation convex pieces: a quad per segment,
  a wedge per join, a fan or square per cap. Filling that non-zero handles
  cusps and self-intersections without offset-curve logic; the cost is more
  edges and slightly heavier antialiasing where pieces overlap along an
  edge. Vertices inside a flattened curve always get round joins. Arcs place
  their inner vertices just outside the circle so the area is preserved.
- Dashing restarts the pattern on every subpath, repeats odd patterns twice
  (as SVG does), and joins a dash that runs over the start of a closed
  subpath into one. A pattern that would make more than a million dashes is
  rejected, and so is an outline over 2²³ points (or 64 per input point).
  Zero-length dashes keep the path's direction, so their square and round
  caps line up with the path.
- Text (`crates/pathlight/src/text.rs`): unicode-bidi splits each paragraph
  into runs in visual order, unicode-script splits those by script (common
  and inherited characters join the run around them), and harfrust shapes
  each piece with its direction, script and surrounding text as context.
  skrifa draws the glyphs unhinted in font units into one path, which is
  filled non-zero like any other path, so text gets the same transforms,
  blend modes and clips. `measure_text` returns the shaped advance so
  callers can align text themselves. Fonts are parsed once when loaded and
  shared by clones.

Each stage is independent of pixel format until the final store, so a SIMD
or GPU backend can replace one stage without touching the API.
`pathlight::composite` is the scalar reference compositor: optimized paths
must match it within rounding.

## Language and boundary

- The core is Rust (`crates/pathlight`).
- `crates/pathlight-capi` exposes a stateless C ABI, built as `libpathlight`
  (shared and static). Its header `include/pathlight.h` is generated by
  cbindgen and checked in; CI fails if it is stale.
- Every draw call takes all of its state explicitly:
  `pathlight_fill(surface, path, fill_rule, paint, transform, blend, clip)`.
  There is no hidden context. Bindings can wrap this in a canvas-style
  object if they want one.
- Enum-like parameters cross the ABI as `uint32_t` and are validated, so a
  bad value from C returns `PATHLIGHT_STATUS_INVALID_ARGUMENT` instead of
  causing undefined behavior. Panics never cross the boundary.
- `crates/pathlight-py` is the Python binding (pyo3, abi3, built with
  maturin). Its `Surface` owns its pixels.
- WASM/JS bindings are planned but not in v1.

## License

MIT OR Apache-2.0.

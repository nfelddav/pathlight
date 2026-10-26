# pathlight

A lightweight 2D path renderer in Rust with a stable C ABI.

- Fill and stroke paths made of lines, quadratic and cubic Béziers, and uniform cubic B-splines
- Strokes with caps, joins, miter limit and dashing
- Source-over plus every separable CSS blend mode, blended in sRGB like browsers do
- Single-line shaped text with bidi, via harfrust and skrifa
- Renders into your own RGBA8, BGRA8 or A8 buffer; clip with any A8 mask
- Sparse-strip rasterizer designed for low memory use, SIMD and later GPU offload

> **Status:** early. Filling, stroking and text work end to end: paths and
> B-splines, both fill rules, caps, joins and dashes, shaped bidi text, all
> blend modes, all three pixel formats and clip masks. See [docs/DESIGN.md](docs/DESIGN.md)
> for what is planned and why.

## Layout

| Path | What |
| --- | --- |
| `crates/pathlight` | Core library |
| `crates/pathlight-capi` | C ABI (`libpathlight`), header in `include/pathlight.h` |
| `crates/pathlight-py` | Python bindings |
| `examples/c` | C example, run in CI as an ABI smoke test |

## Building

```sh
cargo test                                   # core + C ABI
cargo build --release -p pathlight-capi      # libpathlight.{a,so}
make -C examples/c run                       # C example
cargo run -p pathlight --example demo        # writes demo.ppm
cd crates/pathlight-py && maturin develop    # Python module
```

After changing the C ABI, regenerate the header:

```sh
cbindgen --config crates/pathlight-capi/cbindgen.toml --crate pathlight-capi \
  --output crates/pathlight-capi/include/pathlight.h
```

## C example

```c
PathlightSurface surface = {pixels, 64, 64, 64 * 4, PATHLIGHT_FORMAT_RGBA8};
PathlightPaint red = {PATHLIGHT_PAINT_SOLID, {1, 0, 0, 1}, NULL};

PathlightPath *path = pathlight_path_new();
PathlightPoint spline[] = {{10, 40}, {20, 10}, {40, 10}, {50, 40}, {30, 60}};
pathlight_path_bspline(path, spline, 5, true);

pathlight_fill(&surface, path, PATHLIGHT_FILL_NONZERO, &red, NULL, PATHLIGHT_BLEND_MULTIPLY, NULL);
pathlight_path_free(path);
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

import pytest

import pathlight


def test_bspline_builds_cubics():
    path = pathlight.Path()
    path.bspline([(0, 0), (10, 20), (30, 20), (40, 0)])
    assert len(path) == 2  # move_to + one cubic


def test_bspline_needs_enough_points():
    with pytest.raises(ValueError):
        pathlight.Path().bspline([(0, 0), (1, 1)])


def test_surface_starts_transparent():
    s = pathlight.Surface(4, 2)
    assert s.data() == bytes(4 * 2 * 4)


def test_fill_draws_pixels():
    path = pathlight.Path()
    path.move_to(0, 0)
    path.line_to(4, 0)
    path.line_to(4, 2)
    path.line_to(0, 2)
    s = pathlight.Surface(4, 4)
    pathlight.fill(s, path, (1, 0, 0, 1))
    data = s.data()
    assert data[:4] == bytes([255, 0, 0, 255])
    assert data[3 * 16 :] == bytes(16)


def test_fill_respects_clip():
    clip = pathlight.Surface(4, 1, pathlight.Format.A8)
    s = pathlight.Surface(4, 1)
    path = pathlight.Path()
    path.move_to(0, 0)
    path.line_to(4, 0)
    path.line_to(4, 1)
    path.line_to(0, 1)
    pathlight.fill(s, path, (1, 0, 0, 1), clip=clip)
    assert s.data() == bytes(16)


def _triangle():
    path = pathlight.Path()
    path.move_to(0, 0)
    path.line_to(4, 0)
    path.line_to(0, 4)
    path.close()
    return path


def test_stroke_with_dashes_and_clip():
    path = pathlight.Path()
    path.move_to(0, 1)
    path.line_to(8, 1)
    clip = pathlight.Surface(8, 2, pathlight.Format.A8)
    s = pathlight.Surface(8, 2)
    # An empty clip hides everything.
    pathlight.stroke(s, path, (0, 0, 1, 1), width=2, dashes=[2, 2], clip=clip)
    assert s.data() == bytes(8 * 2 * 4)
    pathlight.stroke(s, path, (0, 0, 1, 1), width=2, cap=pathlight.LineCap.BUTT, dashes=[2, 2])
    row = s.data()[: 8 * 4]
    alphas = [row[i * 4 + 3] for i in range(8)]
    assert alphas == [255, 255, 0, 0, 255, 255, 0, 0]


def test_invalid_stroke_width_is_rejected():
    with pytest.raises(ValueError):
        pathlight.stroke(pathlight.Surface(8, 8), _triangle(), (0, 0, 1, 1), width=float("nan"))


def test_clip_must_be_a8():
    with pytest.raises(ValueError):
        pathlight.fill(pathlight.Surface(8, 8), _triangle(), (1, 0, 0, 1), clip=pathlight.Surface(8, 8))


def test_surface_cannot_clip_itself():
    s = pathlight.Surface(8, 8, pathlight.Format.A8)
    with pytest.raises(ValueError):
        pathlight.fill(s, _triangle(), (1, 0, 0, 1), clip=s)


def test_non_finite_color_is_rejected():
    with pytest.raises(ValueError):
        pathlight.fill(pathlight.Surface(8, 8), _triangle(), (float("inf"), 0, 0, 1))


def test_font_rejects_garbage():
    with pytest.raises(ValueError):
        pathlight.Font(b"not a font")


FONT = (
    __import__("pathlib").Path(__file__).resolve().parents[2]
    / "pathlight"
    / "tests"
    / "fonts"
    / "DejaVuSans-subset.ttf"
)


def test_text_measure_and_draw():
    font = pathlight.Font(FONT.read_bytes())
    advance = pathlight.measure_text(font, "Hi", 20)
    assert 15 < advance < 40
    s = pathlight.Surface(48, 32, pathlight.Format.A8)
    pathlight.draw_text(s, font, "Hi", 20, 2, 24, (0, 0, 0, 1))
    assert sum(1 for a in s.data() if a) > 30

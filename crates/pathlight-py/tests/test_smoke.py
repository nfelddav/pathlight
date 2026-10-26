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


def test_fill_not_implemented_yet():
    path = pathlight.Path()
    path.move_to(0, 0)
    path.line_to(4, 0)
    path.line_to(0, 4)
    with pytest.raises(NotImplementedError):
        pathlight.fill(pathlight.Surface(8, 8), path, (1, 0, 0, 1), blend=pathlight.BlendMode.MULTIPLY)


def _triangle():
    path = pathlight.Path()
    path.move_to(0, 0)
    path.line_to(4, 0)
    path.line_to(0, 4)
    path.close()
    return path


def test_stroke_with_dashes_and_clip_not_implemented_yet():
    clip = pathlight.Surface(8, 8, pathlight.Format.A8)
    with pytest.raises(NotImplementedError):
        pathlight.stroke(
            pathlight.Surface(8, 8),
            _triangle(),
            (0, 0, 1, 1),
            width=2,
            cap=pathlight.LineCap.ROUND,
            join=pathlight.LineJoin.BEVEL,
            dashes=[3, 1],
            clip=clip,
        )


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

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

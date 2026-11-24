"""The interval rules the arena compares versions with (ADR 0109). Standard library only.

- `wilson(k, n, level)`: the Wilson score interval of a share.
- `newcombe_paired(n11, n10, n01, n00, level)`: Newcombe (1998) method 10, the interval of the
  difference of two paired shares; the first share is the challenger's, (n11 + n10) / n.
- `mover(d1, ci1, d2, ci2)`: the MOVER interval of d1 - d2 from two independent estimates and
  their intervals (Zou and Donner 2008); conservative when the two are positively correlated.
- `distance_interval(c, ci, target)`: the interval of |c - target| from an interval of c.
- `verdict(lo, hi, better)`: better, worse or no clear difference, from an interval of the
  challenger minus the champion.
"""

from __future__ import annotations

import math
from statistics import NormalDist

BETTER = "better"
WORSE = "worse"
NO_CLEAR = "no clear difference"


def z_of(level: float) -> float:
    """The two-sided normal quantile of a confidence level."""
    return NormalDist().inv_cdf(1 - (1 - level) / 2)


def wilson(k: float, n: float, level: float) -> tuple[float, float]:
    if n <= 0:
        raise ValueError("a share needs n > 0")
    z = z_of(level)
    p = k / n
    den = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / den
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return max(0.0, centre - half), min(1.0, centre + half)


def newcombe_paired(
    n11: int, n10: int, n01: int, n00: int, level: float
) -> tuple[float, float, float]:
    n = n11 + n10 + n01 + n00
    p1 = (n11 + n10) / n
    p2 = (n11 + n01) / n
    l1, u1 = wilson(n11 + n10, n, level)
    l2, u2 = wilson(n11 + n01, n, level)
    den = math.sqrt((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00))
    phi = (n11 * n00 - n10 * n01) / den if den > 0 else 0.0
    d = p1 - p2
    lo = d - math.sqrt(max((p1 - l1) ** 2 - 2 * phi * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))
    hi = d + math.sqrt(max((u1 - p1) ** 2 - 2 * phi * (u1 - p1) * (p2 - l2) + (p2 - l2) ** 2, 0))
    return d, lo, hi


def mover(
    d1: float, ci1: tuple[float, float], d2: float, ci2: tuple[float, float]
) -> tuple[float, float, float]:
    (l1, u1), (l2, u2) = ci1, ci2
    d = d1 - d2
    lo = d - math.sqrt((d1 - l1) ** 2 + (u2 - d2) ** 2)
    hi = d + math.sqrt((u1 - d1) ** 2 + (d2 - l2) ** 2)
    return d, lo, hi


def distance_interval(
    c: float, ci: tuple[float, float], target: float
) -> tuple[float, tuple[float, float]]:
    lo, hi = ci
    d = abs(c - target)
    if lo >= target:
        return d, (lo - target, hi - target)
    if hi <= target:
        return d, (target - hi, target - lo)
    return d, (0.0, max(target - lo, hi - target))


def verdict(lo: float, hi: float, better: str) -> str:
    """`better` is "higher" or "lower": the direction in which the challenger's change is good."""
    if better not in ("higher", "lower"):
        raise ValueError(f"better is higher or lower, not {better}")
    if lo > 0:
        return BETTER if better == "higher" else WORSE
    if hi < 0:
        return WORSE if better == "higher" else BETTER
    return NO_CLEAR

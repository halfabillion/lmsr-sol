#!/usr/bin/env python3
"""Generate exact N-outcome LMSR reference vectors for lmsr-math.

Ported from lmsr-fe/scripts/vectors.py (github.com/halfabillion/lmsr-fe).
Python decimal at 80 significant digits, floored to wei (1e-18).
Writes math/tests/vectors.json. Re-run with:

    python3 scripts/vectors.py && cargo test -p lmsr-math
"""
from decimal import Decimal, getcontext, ROUND_FLOOR
from pathlib import Path
import json
import random

getcontext().prec = 80
WAD = Decimal(10) ** 18
W = 10**18
SEED = 20261119
CASES = 300


def cost(q, b):
    m = max(q)
    return m + b * sum(((x - m) / b).exp() for x in q).ln()


def price(q, b, i):
    m = max(q)
    terms = [((x - m) / b).exp() for x in q]
    return terms[i] / sum(terms)


def wei(x):
    return int((x * WAD).to_integral_value(rounding=ROUND_FLOOR))


def dec(w):
    return Decimal(w) / WAD


def case(q, b, i, buy_amt, sell_amt):
    qd = [dec(x) for x in q]
    bd = dec(b)
    c0 = cost(qd, bd)
    up = list(qd); up[i] += dec(buy_amt)
    down = list(qd); down[i] -= dec(sell_amt)
    n = len(q)
    return {
        "q": [str(x) for x in q],
        "b": str(b),
        "i": i,
        "buy_amt": str(buy_amt),
        "sell_amt": str(sell_amt),
        "cost": str(wei(c0)),
        "prices": [str(wei(price(qd, bd, k))) for k in range(n)],
        "buy_exact": str(wei(cost(up, bd) - c0)),
        "sell_exact": str(wei(c0 - cost(down, bd))),
        "b_ln_n": str(wei(bd * Decimal(n).ln())),
    }


def main():
    out = []
    # Hand-picked cases carried over from lmsr-fe (whole tokens).
    for q, b, i, amt in [
        ([0, 0, 0], 100, 0, 10),
        ([120, 15, 0], 100, 2, 40),
        ([0, 0, 0, 0], 50, 3, 5),
        ([300, 280, 10, 0], 100, 1, 25),
        ([5000, 0, 0, 0], 100, 1, 100),
        ([7, 3, 0, 42, 11], 25, 3, 9),
    ]:
        qw = [x * W for x in q]
        out.append(case(qw, b * W, i, amt * W, min(amt * W, qw[i])))
    rng = random.Random(SEED)
    for _ in range(CASES):
        n = rng.randint(2, 8)
        b = rng.randint(1, 500) * W + rng.randint(0, W - 1)
        spread = b * rng.choice([1, 3, 8, 40])
        q = [rng.randint(0, spread) for _ in range(n)]
        i = rng.randrange(n)
        buy_amt = rng.randint(1, 3 * b)
        sell_amt = rng.randint(0, q[i]) if q[i] > 0 else 0
        out.append(case(q, b, i, buy_amt, sell_amt))
    p = Path(__file__).resolve().parent.parent / "math" / "tests" / "vectors.json"
    p.write_text(json.dumps(out, indent=1) + "\n")
    print(f"wrote {len(out)} vectors to {p}")


if __name__ == "__main__":
    main()

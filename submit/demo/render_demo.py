#!/usr/bin/env python3
"""Render the silent, captioned lmsr-sol devnet demo (1920x1080, H.264).

Inputs (all real, nothing invented):
  docs/SMOKE_devnet.json        signatures, slots, amounts from the devnet run
  submit/demo-stills/*.png      Solana Explorer screenshots of those transactions
Output:
  submit/demo/lmsr-sol-devnet-demo.mp4
Re-run:  python3 submit/demo/render_demo.py
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
SMOKE = json.loads((ROOT / "docs" / "SMOKE_devnet.json").read_text())
STILLS = ROOT / "submit" / "demo-stills"
OUT = ROOT / "submit" / "demo" / "lmsr-sol-devnet-demo.mp4"

W, H, FPS = 1920, 1080, 30
FADE = 0.4
CAP_H = 150

INTER = "/usr/share/fonts/truetype/sand-box/google/Inter/Inter-VariableFont_opsz,wght.ttf"
MONO = "/usr/share/fonts/truetype/sand-box/google/IBM Plex Mono/IBMPlexMono-Regular.ttf"
MONO_MED = "/usr/share/fonts/truetype/sand-box/google/IBM Plex Mono/IBMPlexMono-Medium.ttf"
MONO_BOLD = "/usr/share/fonts/truetype/sand-box/google/IBM Plex Mono/IBMPlexMono-Bold.ttf"

BG = (10, 12, 20)
PANEL = (18, 22, 34)
BORDER = (44, 52, 74)
FG = (236, 240, 248)
DIM = (150, 160, 182)
GREEN = (20, 241, 149)     # Solana green
PURPLE = (153, 69, 255)    # Solana purple
AMBER = (255, 196, 92)
RED = (255, 120, 120)
CYAN = (110, 210, 230)
CAP_BG = (6, 8, 14)

_font_cache: dict = {}


def inter(size, weight="Regular"):
    key = ("inter", size, weight)
    if key not in _font_cache:
        f = ImageFont.truetype(INTER, size)
        try:
            f.set_variation_by_name(weight)
        except Exception:
            pass
        _font_cache[key] = f
    return _font_cache[key]


def mono(size, path=MONO):
    key = (path, size)
    if key not in _font_cache:
        _font_cache[key] = ImageFont.truetype(path, size)
    return _font_cache[key]


def wrap(draw, text, fnt, width):
    out = []
    for para in text.split("\n"):
        words, cur = [], ""
        for w in para.split():
            while draw.textlength(w, font=fnt) > width:  # break very long tokens
                k = len(w)
                while k > 1 and draw.textlength(w[:k], font=fnt) > width:
                    k -= 1
                words.append(w[:k])
                w = w[k:]
            words.append(w)
        for w in words:
            t = w if not cur else cur + " " + w
            if draw.textlength(t, font=fnt) <= width:
                cur = t
            else:
                out.append(cur)
                cur = w
        out.append(cur)
    return out


def ex(kind, sig):
    return f"https://explorer.solana.com/{kind}/{sig}?cluster=devnet"


SIG = SMOKE["signatures"]
SLOT = SMOKE["confirmations"]["slots"]
PROGRAM = SMOKE["program_id"]
DEPLOY = SMOKE["deploy"]["signature"]
TB = SMOKE["balances"]["token_base_units"]
RT = SMOKE["round_trip"]
paid = int(RT["paid"])
back = int(RT["got_back"])
buy2_cost = int(TB["after_sell"]) - int(TB["before_redeem"])
redeemed = int(SMOKE["redeemed"])
subsidy = int(SMOKE["subsidy_base_units"])
assert paid - back == 1 and redeemed == 25_000_000 and subsidy == 109_861_229


def fmt(n):
    return f"{n:,}"


# ---------------------------------------------------------------- base layers
def base_frame(title, step=None):
    im = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(im)
    # top bar
    d.rectangle([0, 0, W, 76], fill=(14, 17, 28))
    d.rectangle([0, 74, W, 76], fill=PURPLE)
    d.text((48, 18), "lmsr-sol", font=inter(34, "Bold"), fill=GREEN)
    d.text((48 + d.textlength("lmsr-sol", font=inter(34, "Bold")) + 18, 26), title,
           font=inter(26, "Medium"), fill=FG)
    tag = "Solana devnet"
    tw = d.textlength(tag, font=inter(22, "SemiBold"))
    d.rounded_rectangle([W - 48 - tw - 32, 20, W - 48, 58], radius=19, outline=GREEN, width=2)
    d.text((W - 48 - tw - 16, 25), tag, font=inter(22, "SemiBold"), fill=GREEN)
    if step:
        sw = d.textlength(step, font=inter(22, "Medium"))
        d.text((W - 48 - tw - 32 - sw - 24, 27), step, font=inter(22, "Medium"), fill=DIM)
    return im


def caption(im, text):
    d = ImageDraw.Draw(im)
    d.rectangle([0, H - CAP_H, W, H], fill=CAP_BG)
    d.rectangle([0, H - CAP_H, W, H - CAP_H + 2], fill=BORDER)
    f = inter(36, "Medium")
    lines = wrap(d, text, f, W - 200)
    y = H - CAP_H + (CAP_H - len(lines) * 48) // 2
    for ln in lines:
        lw = d.textlength(ln, font=f)
        d.text(((W - lw) // 2, y), ln, font=f, fill=FG)
        y += 48


def url_bar(d, url, x, y, w):
    d.rounded_rectangle([x, y, x + w, y + 44], radius=10, fill=(24, 29, 44), outline=BORDER)
    f = mono(17)
    s = url
    while d.textlength(s, font=f) > w - 30:
        f = mono(f.size - 1)
    d.text((x + 14, y + 11), s, font=f, fill=CYAN)


def fact_panel(d, x, y, w, heading, rows, note=None):
    d.rounded_rectangle([x, y, x + w, 900], radius=16, fill=PANEL, outline=BORDER)
    d.text((x + 28, y + 24), heading, font=inter(32, "Bold"), fill=FG)
    yy = y + 84
    for label, value, color in rows:
        d.text((x + 28, yy), label, font=inter(21, "Medium"), fill=DIM)
        yy += 30
        fv = mono(26 if len(value) <= 26 else 21, MONO_MED)
        for ln in wrap(d, value, fv, w - 56):
            d.text((x + 28, yy), ln, font=fv, fill=color)
            yy += 34
        yy += 16
    if note:
        fn = inter(22, "Regular")
        for ln in wrap(d, note, fn, w - 56):
            d.text((x + 28, yy), ln, font=fn, fill=AMBER)
            yy += 30


# ---------------------------------------------------------------- stills
def load_still(name, cut=None):
    im = Image.open(STILLS / name).convert("RGB")
    if cut:  # remove a horizontal band (y0, y1)
        y0, y1 = cut
        top = im.crop((0, 0, im.width, y0))
        bot = im.crop((0, y1, im.width, im.height))
        new = Image.new("RGB", (im.width, top.height + bot.height))
        new.paste(top, (0, 0))
        new.paste(bot, (0, top.height))
        im = new
    # crop the explorer page to its content column (drop empty side margins)
    x0, x1 = 120, im.width - 120
    im = im.crop((x0, 0, x1, im.height))
    return im


PX, PY, PW, PH = 48, 156, 1240, 744  # still viewport (below URL bar)
COL = (440, 1480)      # explorer content column (summary)
LOGS_X = (960, 1905)   # explorer "Logs" panel (right half of the Programs/Logs row)


def _bg_rows(im):
    """First y (below the summary) where the full-width Programs/Logs row starts."""
    px = im.load()
    bg = px[5, im.height // 2]
    for y in range(300, im.height):
        for x in range(20, 60, 2):
            c = px[x, y]
            if abs(c[0] - bg[0]) + abs(c[1] - bg[1]) + abs(c[2] - bg[2]) > 30:
                return y
    return None


def _cut(im, cut):
    y0, y1 = cut
    top = im.crop((0, 0, im.width, y0))
    bot = im.crop((0, y1, im.width, im.height))
    new = Image.new("RGB", (im.width, top.height + bot.height))
    new.paste(top, (0, 0))
    new.paste(bot, (0, top.height))
    return new


def _view(src, x0, x1, y0, y1):
    """Crop src[x0:x1, y0:y1] and scale to the viewport width."""
    crop = src.crop((x0, y0, x1, min(y1, src.height)))
    s = PW / crop.width
    return crop.resize((PW, int(crop.height * s)), Image.LANCZOS)


def smooth(q):
    q = max(0.0, min(1.0, q))
    return q * q * (3 - 2 * q)


class StillScene:
    """Phase A: zoomed summary column (slow scroll). Phase B (tx pages): zoomed Logs panel."""

    def __init__(self, dur, title, step, still, url, heading, rows, caps, note=None,
                 cut=None, split=0.55, logs=True):
        self.dur, self.caps, self.split = dur, caps, split
        src = Image.open(STILLS / still).convert("RGB")
        if cut:
            src = _cut(src, cut)
        sec = _bg_rows(src) if logs else None
        col_end = sec if sec else src.height
        self.a = _view(src, COL[0], COL[1], 90, col_end)
        self.a_scroll = max(0, self.a.height - PH)
        if logs and sec:
            vh = int((LOGS_X[1] - LOGS_X[0]) * PH / PW)
            self.b = _view(src, LOGS_X[0], LOGS_X[1], sec - 12, sec - 12 + vh)
            self.a_scroll = min(self.a_scroll, int(0.55 * PH))
        else:
            self.b = None
            self.split = 1.0
        base = base_frame(title, step)
        d = ImageDraw.Draw(base)
        url_bar(d, url, PX, 96, PW)
        d.rounded_rectangle([PX - 2, PY - 2, PX + PW + 2, PY + PH + 2], radius=6, outline=BORDER, width=2)
        fact_panel(d, PX + PW + 32, 96, W - (PX + PW + 32) - 48, heading, rows, note)
        self.base = base
        self.view_a_end = None

    def _a(self, p):
        q = smooth((p / self.split - 0.25) / 0.65) if self.split < 1 else smooth((p - 0.3) / 0.62)
        off = int(self.a_scroll * q)
        frame = Image.new("RGB", (PW, PH), (22, 27, 25))
        frame.paste(self.a.crop((0, off, PW, off + min(PH, self.a.height - off))), (0, 0))
        return frame

    def _b(self):
        frame = Image.new("RGB", (PW, PH), (22, 27, 25))
        frame.paste(self.b.crop((0, 0, PW, min(PH, self.b.height))), (0, 0))
        return frame

    def frame(self, t):
        im = self.base.copy()
        p = t / self.dur
        if self.b is None or p < self.split - 0.03:
            view = self._a(min(p, self.split))
        elif p < self.split + 0.03:
            k = (p - (self.split - 0.03)) / 0.06
            view = Image.blend(self._a(self.split), self._b(), smooth(k))
        else:
            view = self._b()
        im.paste(view, (PX, PY))
        if self.b is not None and p >= self.split:
            d = ImageDraw.Draw(im)
            lab = "Program logs"
            f = inter(20, "SemiBold")
            lw = d.textlength(lab, font=f)
            d.rounded_rectangle([PX + PW - lw - 36, PY + PH - 46, PX + PW - 12, PY + PH - 12], radius=8, fill=PURPLE)
            d.text((PX + PW - lw - 24, PY + PH - 42), lab, font=f, fill=(255, 255, 255))
        caption(im, pick(self.caps, p))
        return im


def pick(caps, p):
    cur = caps[0][1]
    for start, txt in caps:
        if p >= start:
            cur = txt
    return cur


class CardScene:
    def __init__(self, dur, title, step, draw_fn, caps):
        self.dur, self.caps = dur, caps
        base = base_frame(title, step)
        draw_fn(ImageDraw.Draw(base), base)
        self.base = base

    def frame(self, t):
        im = self.base.copy()
        caption(im, pick(self.caps, t / self.dur))
        return im


# ---------------------------------------------------------------- cards
def center(d, y, text, f, fill):
    w = d.textlength(text, font=f)
    d.text(((W - w) // 2, y), text, font=f, fill=fill)


def draw_title(d, im):
    center(d, 250, "lmsr-sol", inter(120, "Bold"), GREEN)
    center(d, 410, "An N-outcome LMSR prediction-market maker,", inter(46, "Medium"), FG)
    center(d, 470, "as a native Solana program, running on devnet", inter(46, "Medium"), FG)
    center(d, 580, f"Program  {PROGRAM}", mono(30, MONO_MED), CYAN)
    center(d, 650, "2 to 8 outcomes  ·  any SPL token as collateral  ·  rounding always favours the market maker",
           inter(28), DIM)
    center(d, 760, "Design and math ported from our own open-source project github.com/halfabillion/lmsr-fe (Apache-2.0)",
           inter(24), AMBER)


def draw_math(d, im):
    x = 120
    d.text((x, 130), "The market maker", font=inter(48, "Bold"), fill=FG)
    d.rounded_rectangle([x, 210, W - 120, 380], radius=16, fill=PANEL, outline=BORDER)
    d.text((x + 36, 236), "C(q)  = m + b · ln( Σ exp(−(m − qᵢ)/b) ),   m = max qᵢ", font=mono(38, "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"), fill=CYAN)
    d.text((x + 36, 300), "pᵢ(q) = exp(−(m − qᵢ)/b) / Σ exp(−(m − qⱼ)/b)", font=mono(38, "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"), fill=CYAN)
    items = [
        (GREEN, "Buy", "charges C(q′) − C(q) + error budget, then rounds UP to token base units"),
        (RED, "Sell", "pays C(q) − C(q′) − error budget, then rounds DOWN (floored at 0)"),
        (AMBER, "Prices", "rounded down, so they always sum to at most 1"),
        (PURPLE, "Subsidy", "creator locks b · ln(n) + budget, so every winner can always be paid"),
    ]
    y = 430
    for col, k, v in items:
        d.rounded_rectangle([x, y, x + 14, y + 60], radius=4, fill=col)
        d.text((x + 40, y + 2), k, font=inter(34, "Bold"), fill=col)
        d.text((x + 230, y + 6), v, font=inter(32), fill=FG)
        y += 92
    d.text((x, y + 10), "WAD (1e18) fixed point in u128 with a 256-bit mul_div. exp only sees x ≤ 0, ln only x ≥ 1: no overflow.",
           font=inter(26), fill=DIM)


def draw_proof(d, im):
    x = 140
    d.text((x, 120), "Round-trip proof, on devnet", font=inter(52, "Bold"), fill=FG)
    d.text((x, 190), "Same trader, same outcome, same size: buy 40 shares, then sell the same 40 shares.",
           font=inter(30), fill=DIM)
    rows = [
        ("Shares (outcome 0)", "40", fmt(int(RT['shares'])) + " base units", FG),
        ("Buy  · paid into vault", "buy tx", fmt(paid), GREEN),
        ("Sell · returned to trader", "sell tx", fmt(back), RED),
        ("Kept by the market maker", "", "1", AMBER),
    ]
    y = 270
    for i, (k, tx, v, col) in enumerate(rows):
        fill = PANEL if i < 3 else (40, 34, 14)
        d.rounded_rectangle([x, y, W - x, y + 104], radius=14, fill=fill, outline=BORDER if i < 3 else AMBER, width=2)
        d.text((x + 36, y + 30), k, font=inter(38, "SemiBold"), fill=FG)
        vw = d.textlength(v, font=mono(52, MONO_BOLD))
        d.text((W - x - 36 - vw, y + 20), v, font=mono(52, MONO_BOLD), fill=col)
        y += 124
    d.text((x, y + 8), "15,181,215 − 15,181,214 = 1 base unit.  A trader can never profit from buy→sell round trips.",
           font=inter(32, "SemiBold"), fill=GREEN)
    d.text((x, y + 56), f"Program logs: buy {paid:#x} = {fmt(paid)},  sell {back:#x} = {fmt(back)}", font=mono(24, MONO_MED), fill=CYAN)
    y += 40
    d.text((x, y + 60), f"buy  {SIG['buy']}", font=mono(20), fill=DIM)
    d.text((x, y + 90), f"sell {SIG['sell']}", font=mono(20), fill=DIM)


def draw_tests(d, im):
    x = 140
    d.text((x, 120), "Tested before it shipped", font=inter(52, "Bold"), fill=FG)
    items = [
        ("18 / 18", "tests pass (cargo test --release --workspace)"),
        ("306", "cases vs. 80-digit exact reference: buy ≥ exact cost, sell ≤ exact payout"),
        ("6 × 512", "random property cases: no dust drain, prices sum ≤ 1, maker loss ≤ b·ln(n)"),
        ("5", "integration tests of the compiled program with the real SPL Token program"),
        ("≤ 435k CU", "worst case (8 outcomes, sell); clients request 600k"),
    ]
    y = 230
    for k, v in items:
        d.rounded_rectangle([x, y, W - x, y + 100], radius=14, fill=PANEL, outline=BORDER)
        d.text((x + 36, y + 22), k, font=mono(46, MONO_BOLD), fill=GREEN)
        d.text((x + 380, y + 32), v, font=inter(32), fill=FG)
        y += 120


def draw_end(d, im):
    center(d, 150, "lmsr-sol on Solana devnet", inter(64, "Bold"), FG)
    lines = [
        ("Program", ex("address", PROGRAM)),
        ("Deploy", ex("tx", DEPLOY)),
        ("create_market", ex("tx", SIG["create_market"])),
        ("buy", ex("tx", SIG["buy"])),
        ("sell", ex("tx", SIG["sell"])),
        ("buy (outcome 2)", ex("tx", SIG["buy_again"])),
        ("resolve", ex("tx", SIG["resolve"])),
        ("redeem", ex("tx", SIG["redeem"])),
    ]
    y = 260
    for k, u in lines:
        d.text((100, y), k, font=inter(24, "SemiBold"), fill=DIM)
        f = mono(17)
        d.text((330, y + 4), u, font=f, fill=CYAN)
        y += 50
    center(d, y + 40, "All transactions finalized on devnet. Test collateral is a fresh 6-decimal test SPL mint.",
           inter(28), FG)
    center(d, y + 90, "LMSR design ported from our own open-source github.com/halfabillion/lmsr-fe (Apache-2.0).",
           inter(26), AMBER)


# ---------------------------------------------------------------- scenes
def scenes():
    S = []
    S.append(CardScene(8, "", None, draw_title, [
        (0, "lmsr-sol: a logarithmic market scoring rule (LMSR) market maker for prediction markets on Solana."),
        (0.5, "Everything shown is real: deployed to devnet on 20 Nov 2026, every transaction finalized."),
    ]))
    S.append(CardScene(15, "How it prices", None, draw_math, [
        (0, "LMSR always quotes a price, and its prices are probabilities that sum to 1."),
        (0.35, "Every rounding error is an attack surface: one unit too cheap on a buy can be drained by repetition."),
        (0.7, "So every trade rounds toward the market maker: buys round up, sells round down."),
    ]))
    S.append(StillScene(12, "Program account", "1 / 8", "01-program.png", ex("address", PROGRAM),
        "Program", [("Program id", PROGRAM, CYAN), ("Loader", "BPF upgradeable", FG),
                    ("Size", "84,320 bytes", FG), ("Last deployed slot", fmt(SLOT['deploy']), FG)],
        [(0, "The deployed program on Solana devnet: executable, upgradeable, owned by the BPF upgradeable loader."),
         (0.5, "Its recent transaction history is the smoke test you are about to see.")],
        cut=(451, 528), logs=False))
    S.append(StillScene(10, "Deploy transaction", "2 / 8", "02-deploy.png", ex("tx", DEPLOY),
        "Deploy", [("Signature", DEPLOY[:22] + "…", CYAN), ("Slot", fmt(SLOT['deploy']), FG),
                   ("Result", "Success, finalized", GREEN)],
        [(0, "The deploy transaction that finalized the 84,320-byte program on devnet, in slot 524,398,076.")]))
    S.append(StillScene(14, "create_market", "3 / 8", "03-create-market.png", ex("tx", SIG["create_market"]),
        "Create market", [("Outcomes", "3", FG), ("Liquidity b", "100 tokens", FG),
                          ("Subsidy locked", fmt(subsidy) + " base units", AMBER),
                          ("Program log (hex)", f"{subsidy:#x} = {fmt(subsidy)}", CYAN),
                          ("Collateral", "test SPL mint, 6 decimals", FG)],
        [(0, "create_market: a 3-outcome market with liquidity b = 100 test tokens."),
         (0.45, "The creator locks the subsidy b·ln(3) plus the error budget, rounded up: 109,861,229 base units.")],
        note="100 · ln 3 = 109.861228… tokens"))
    S.append(StillScene(13, "buy", "4 / 8", "04-buy.png", ex("tx", SIG["buy"]),
        "Buy", [("Outcome", "0", FG), ("Shares", "40 (40,000,000 base units)", FG),
                ("Paid", fmt(paid) + " base units", GREEN),
                ("Program log (hex)", f"{paid:#x} = {fmt(paid)}", CYAN)],
        [(0, "buy: 40 shares of outcome 0. The cost is computed exactly, plus the error budget, then rounded UP."),
         (0.5, "The trader pays 15,181,215 base units into the market's vault.")]))
    S.append(StillScene(13, "sell", "5 / 8", "05-sell.png", ex("tx", SIG["sell"]),
        "Sell", [("Outcome", "0", FG), ("Shares", "40 (the same 40)", FG),
                 ("Returned", fmt(back) + " base units", RED),
                 ("Program log (hex)", f"{back:#x} = {fmt(back)}", CYAN)],
        [(0, "sell: the same 40 shares straight back. The payout is computed, minus the budget, then rounded DOWN."),
         (0.5, "The trader receives 15,181,214 base units: one less than they paid.")]))
    S.append(CardScene(15, "Maker-protective rounding", None, draw_proof, [
        (0, "Buy 40, sell 40: paid 15,181,215, got back 15,181,214. The market kept 1 base unit."),
        (0.5, "Round trips can only lose, so nobody can drain the market one dust unit at a time."),
    ]))
    S.append(StillScene(10, "buy (outcome 2)", "6 / 8", "06-buy-again.png", ex("tx", SIG["buy_again"]),
        "Buy again", [("Outcome", "2", FG), ("Shares", "25 (25,000,000 base units)", FG),
                      ("Paid", fmt(buy2_cost) + " base units", GREEN),
                      ("Program log (hex)", f"{buy2_cost:#x} = {fmt(buy2_cost)}", CYAN)],
        [(0, "A second buy: 25 shares of outcome 2 for 9,045,765 base units.")]))
    S.append(StillScene(10, "resolve", "7 / 8", "07-resolve.png", ex("tx", SIG["resolve"]),
        "Resolve", [("Winner", "outcome 2", AMBER), ("Signer", "market creator", FG),
                    ("Log", "lmsr: resolved (winner)", CYAN)],
        [(0, "resolve: the creator closes trading and sets outcome 2 as the winner.")]))
    S.append(StillScene(12, "redeem", "8 / 8", "08-redeem.png", ex("tx", SIG["redeem"]),
        "Redeem", [("Winning shares", "25", FG), ("Paid out", fmt(redeemed) + " base units", GREEN),
                   ("Program log (hex)", f"{redeemed:#x} = {fmt(redeemed)}", CYAN),
                   ("Log", "lmsr: redeemed (payout)", CYAN)],
        [(0, "redeem: each winning share pays one base unit. 25 shares pay 25,000,000."),
         (0.5, "The vault still holds 93,906,995 base units after paying the winner in full.")]))
    S.append(CardScene(12, "Tests", None, draw_tests, [
        (0, "The same rounding rules are checked by exact reference vectors and randomized property tests."),
        (0.5, "Maker loss never exceeds b·ln(n), and the vault always covers the largest outstanding outcome."),
    ]))
    S.append(CardScene(14, "Links", None, draw_end, [
        (0, "Every transaction above is on Solana devnet: open any link with ?cluster=devnet."),
        (0.5, "lmsr-sol: LMSR prediction markets on Solana that never lose money to rounding."),
    ]))
    return S


def main():
    S = scenes()
    total = sum(s.dur for s in S)
    print(f"scenes={len(S)} duration={total:.1f}s")
    assert total <= 180
    OUT.parent.mkdir(parents=True, exist_ok=True)
    cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
           "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
           "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p",
           "-profile:v", "high", "-tune", "stillimage", "-movflags", "+faststart",
           "-metadata", "title=lmsr-sol on Solana devnet", str(OUT)]
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    black = Image.new("RGB", (W, H), BG)
    for idx, s in enumerate(S):
        n = int(round(s.dur * FPS))
        fade_n = int(FADE * FPS)
        last = None
        for i in range(n):
            fr = s.frame(i / FPS)
            if i < fade_n:
                fr = Image.blend(black, fr, (i + 1) / fade_n)
            elif i >= n - fade_n:
                fr = Image.blend(black, fr, (n - i) / fade_n)
            p.stdin.write(fr.tobytes())
        print(f"  scene {idx + 1}/{len(S)} done", flush=True)
    p.stdin.close()
    p.wait()
    print("wrote", OUT)


if __name__ == "__main__":
    main()

# lmsr-sol

An N-outcome **LMSR prediction-market maker** as a native Solana program
(2 to 8 outcomes, any SPL token as collateral), with fixed-point math that
rounds every trade in the market maker's favour, so repeated round trips can
never drain the maker one dust unit at a time.

> **Prior work disclosure.** The LMSR design, the WAD fixed-point `exp`/`ln`
> approach, the rounding policy, the error budget and the reference-vector
> generator are ported from our own earlier open-source project
> [github.com/halfabillion/lmsr-fe](https://github.com/halfabillion/lmsr-fe)
> (Apache-2.0, written in Fe for the EVM). This repository is the Solana port:
> the Rust math crate, the on-chain program, the property tests, the LiteSVM
> integration tests and the client scripts are new.

## What it does

| Instruction | Who | Effect |
|---|---|---|
| `CreateMarket { market_id, n, b }` | creator | Creates the market PDA and a vault token account (PDA) for the collateral mint, and locks the subsidy `b·ln(n)` plus the error budget in the vault |
| `Buy { outcome, amount, max_cost }` | anyone | Pays `cost` (rounded **up**) into the vault, receives `amount` shares of `outcome` |
| `Sell { outcome, amount, min_payout }` | share holder | Returns shares, receives `payout` (rounded **down**) from the vault |
| `Resolve { winner }` | creator | Closes trading and fixes the winning outcome |
| `Redeem` | share holder | Receives 1 collateral base unit per winning share |

Shares are counted in the collateral's base units (with a 6-decimal mint,
1 share = 0.000001 tokens; 1,000,000 shares pay 1 token if they win).
`max_cost` / `min_payout` protect traders from slippage.

## The math

All quantities are WAD fixed point (1e18 = one whole token) in `u128`,
with a 256-bit intermediate for `mul_div`. LMSR is evaluated in max-shifted
log-sum-exp form:

```
C(q)   = m + b · ln( Σ_i exp(-(m - q_i) / b) ),   m = max_i q_i
p_i(q) = exp(-(m - q_i) / b) / Σ_j exp(-(m - q_j) / b)
```

Every exponent is ≤ 0 and the sum lies in `[1, n]`, so `exp` only ever sees
non-positive arguments and `ln` only arguments ≥ 1: no signed fixed point and
no overflow for any book.

- `exp_neg_wad(x) = exp(-x)`: range reduction `x = k·ln2 + r`, Taylor series
  for `exp(r)`, then `2^-k / exp(r)`. Native `u128`, error ≤ a few wei.
- `ln_wad(x)`, `x ≥ 1`: `x = 2^k·y`, `ln y = 2·atanh((y-1)/(y+1))`, series at
  1e36 internal precision; within about 1 wei.

## Rounding policy (maker-protective)

`cost_err_n(b, n) = ceil(b · 64n / 1e18) + 64n` wei is a documented error
budget per cost evaluation (much larger than the measured few wei).

- **Buy** charges `max(C(q') - C(q), 0) + cost_err_n`, never below the exact cost,
  and the WAD amount is converted to base units rounding **up**.
- **Sell** pays `max(C(q) - C(q') - cost_err_n, 0)`, never above the exact payout,
  and the WAD amount is converted to base units rounding **down**.
- Both sides use the same deterministic `C`, so buy-then-sell of the same size
  loses at least `2·cost_err_n` (or everything, for dust). A round trip can
  never profit, and a dust sell pays 0 instead of reverting.
- **Prices** are rounded down, so they sum to at most 1.0 (and within 16 wei
  per outcome of it).
- **Subsidy:** `lmsr_max_loss(b, n) = ceil(b · (ln n + 1 wei)) + cost_err_n`,
  converted to base units rounding up. With every trade rounded toward the
  maker, the vault always covers the largest outstanding outcome, so every
  winner can redeem in full, and the maker's loss never exceeds `b·ln(n)`.

## Tests

```bash
# build the SBF program (needed by the LiteSVM integration tests)
cargo build-sbf --manifest-path program/Cargo.toml
# all tests: unit, reference vectors, proptest, LiteSVM integration
cargo test --release --workspace -- --nocapture
```

Full output of the last run: [`docs/TEST_OUTPUT.txt`](docs/TEST_OUTPUT.txt).

| Suite | What it checks |
|---|---|
| `math` unit tests (6) | `exp`/`ln` against known constants, flat-book cost = `b·ln n`, input validation, unit conversion direction, dust |
| `math/tests/vectors.rs` (306 cases) | Against 80-digit `decimal` reference values (`scripts/vectors.py`): cost and prices within budget, buy ≥ exact, sell ≤ exact, subsidy ≥ `b·ln n` |
| `math/tests/props.rs` (6 properties × 512 random cases) | **No dust drain**: buy→sell round trips always lose, 1-wei round trips pay 0, random single-trader sequences closed out in token base units never profit. **Prices sum to 1**: sum ≤ 1.0 and ≥ 1.0 − 16n wei. **Maker loss ≤ b·ln(n)** after every step of random multi-trade sequences, and the vault stays solvent. Monotone costs |
| `program/tests/litesvm.rs` (5) | The compiled `.so` in LiteSVM with the real SPL Token program: full lifecycle, on-chain round trips at many sizes, seeded random 40-trade sequences for n = 2, 3, 5, 8 (vault ≥ largest outstanding outcome after every trade, all winners paid), error paths (slippage, bad outcome, over-sell, unauthorized resolve, early/double redeem), compute units |

Compute: an n = 8 buy that also creates the trader's position uses about
226k CU and an n = 8 sell about 398k CU. Clients should request 600k CU with a
`SetComputeUnitLimit` instruction (the smoke script does).

To regenerate the reference vectors: `python3 scripts/vectors.py`.

## Devnet

Status and addresses: see [`docs/DEVNET.md`](docs/DEVNET.md).

```bash
cd scripts && npm install && cd ..
WALLET=/path/outside/repo/payer.json \
PROGRAM_KP=/path/outside/repo/program-keypair.json \
  scripts/deploy-devnet.sh          # build, deploy, then run scripts/smoke.ts
```

`scripts/smoke.ts` creates a fresh 6-decimal **test** SPL mint, creates a
3-outcome market (b = 100 tokens), buys and sells 40 shares (checking that
the round trip loses money), buys again, resolves and redeems, and writes
all transaction signatures to `docs/SMOKE_<cluster>.json`. A run against a
local `solana-test-validator` is recorded in `docs/SMOKE_localnet.json`.

Keypairs are never stored in this repository (`.gitignore` excludes `*.json`
keypairs, `.secrets/`, `target/` and `node_modules/`).

## Layout

```
math/      lmsr-math: no_std WAD fixed-point LMSR (exp, ln, cost, prices, buy/sell, subsidy)
program/   lmsr-sol: native Solana program (solana-program only; raw CPI to System and SPL Token)
scripts/   vectors.py (reference vectors), smoke.ts (devnet/localnet client), deploy-devnet.sh
docs/      TEST_OUTPUT.txt, DEVNET.md, smoke results
```

## Limitations

- Single resolver (the market creator); no oracle, dispute or timeout/cancel
  path yet (lmsr-fe has a cancel/refund path that is not ported yet).
- No sweep of the leftover subsidy after resolution yet.
- SPL Token (classic) only; Token-2022 mints are rejected.
- Not audited. Devnet only.

## License

Apache-2.0, see [LICENSE](LICENSE). The ported design and math come from
[lmsr-fe](https://github.com/halfabillion/lmsr-fe), also Apache-2.0 and our own work.

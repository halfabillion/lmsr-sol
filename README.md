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

## Demo

**Demo video (2:38, silent with captions):** [watch in the browser](https://halfabillion.github.io/lmsr-sol/video.html) ·
[download the mp4](https://github.com/halfabillion/lmsr-sol/releases/download/v0.1.0-devnet/lmsr-sol-devnet-demo.mp4) (release [v0.1.0-devnet](https://github.com/halfabillion/lmsr-sol/releases/tag/v0.1.0-devnet)) ·
in this repo at [`submit/demo/lmsr-sol-devnet-demo.mp4`](submit/demo/lmsr-sol-devnet-demo.mp4).

It walks through the live devnet program
[`7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz`](https://explorer.solana.com/address/7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz?cluster=devnet):
create_market, buy, sell, the 1-base-unit round-trip proof, resolve and redeem, all on Solana Explorer.

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
247k CU and an n = 8 sell about 435k CU (the test asserts both stay under 600k). Clients should request 600k CU with a
`SetComputeUnitLimit` instruction (the smoke script does).

To regenerate the reference vectors: `python3 scripts/vectors.py`.

## Devnet

**Deployed to Solana devnet on 20 Nov 2026.**

| | |
|---|---|
| Program id | [`7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz`](https://explorer.solana.com/address/7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz?cluster=devnet) |
| Deploy transaction | [`3wAGJFchQSmug7ug…`](https://explorer.solana.com/tx/3wAGJFchQSmug7ugCqCys44jdwpKJhWcdMpZrGeH8Y4K1Yh8B8udUT7G9pEuVFRARiragZE6W9g7hsq9HSa4YCM5?cluster=devnet) |
| Smoke test (create, buy, sell, buy, resolve, redeem) | [`docs/SMOKE_devnet.json`](docs/SMOKE_devnet.json), every transaction finalized |

Smoke transactions on devnet:
- mint_to: [`2XQRrxCh8epZJax2…`](https://explorer.solana.com/tx/2XQRrxCh8epZJax2pdMV3uqnYDmVvf9oJErwc9fyba2drXEm3hNzjDBZCJ54A8oiPYWMyc9YrEFgmYDyARzasvoc?cluster=devnet)
- create_market: [`24FAHPzpn4sgLUEp…`](https://explorer.solana.com/tx/24FAHPzpn4sgLUEpZDaxZShpyEB6GhK3RTytniS4SazWoJ6Saf2NFoD8MjMqKGdD8zzeRMktKBEptuUEtWX5AEEV?cluster=devnet)
- buy: [`4RmDoD7shYvQkhrv…`](https://explorer.solana.com/tx/4RmDoD7shYvQkhrv728M3QCWxcKfX9NwSG4AKoraYxw4KWDizh72kLHQY6kGrY6EWp2TDUEZeUaksNf6C3M4U4p9?cluster=devnet)
- sell: [`p9SW8gHcTA9U2fg3…`](https://explorer.solana.com/tx/p9SW8gHcTA9U2fg37r2NiAZHqwXQawvqe2B164jeNrn1VfDSZp4NK5mfsfZUDRYX6t4zFDDUTuSqEqacsNZ1XWY?cluster=devnet)
- buy_again: [`5tkAjQ2MksxyzFs7…`](https://explorer.solana.com/tx/5tkAjQ2MksxyzFs7Sou8LJBLC1oYqjDSLf8zCNgqEL9i9Cv7BiRZpZZQdjZQnyJqQUkBiCXdMNqxwD3HygwRHYhQ?cluster=devnet)
- resolve: [`2LqEGVmmzLxiR2H4…`](https://explorer.solana.com/tx/2LqEGVmmzLxiR2H4WcckgD6htZUsUaSD8xyMtAo4vPK2jKGDuRC3tRvhSTzHtuHaEQUgQ8DrhPgTjUAcEvgcTR6n?cluster=devnet)
- redeem: [`2PBufyDh7yvufq9C…`](https://explorer.solana.com/tx/2PBufyDh7yvufq9ChRkbFWjfVERs7ueAp1ycLqYTTL4iE9pzNxnmda3ZyjNbRfKHm24j488PvLormTrAvcgN4iJ7?cluster=devnet)

The test collateral is a fresh 6-decimal test SPL mint created by the smoke
script, not a real token. Details, costs and funding notes: [`docs/DEVNET.md`](docs/DEVNET.md).

To redeploy or reproduce:

```bash
cd scripts && npm install && cd ..
WALLET=/path/outside/repo/payer.json \
PROGRAM_KP=/path/outside/repo/program-keypair.json \
  scripts/deploy-devnet.sh          # build, deploy, then run scripts/smoke.ts
```

A devnet deploy needs about 0.4305 SOL (program-data rent for the 84,320-byte
build plus fees; the write buffer is refunded before program data is funded)
and the smoke test about 0.0075 SOL more.

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

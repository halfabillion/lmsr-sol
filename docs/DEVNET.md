# Devnet status

| Item | Value |
|---|---|
| Program id (reserved keypair, kept outside the repo) | `7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz` |
| Payer (box wallet) | `HKJhkAZdkdNAmET7JoLHyTmDioka3CGYdajqaM31Wdgd` |
| Program size | 84,320 bytes (was 85,944), sha256 `249abfa22f5203a7020225ef73125c75508c7ebc49a5d023d9f25db1f6b0e91b` |
| Devnet SOL needed | **≈ 0.4305 SOL to deploy + ≈ 0.0075 SOL for the smoke test ≈ 0.438 SOL** (details below) |
| Devnet deploy | **Deployed 20 Nov 2026 ~15:30 UK**, tx [`3wAGJFchQSmug7ug…`](https://explorer.solana.com/tx/3wAGJFchQSmug7ugCqCys44jdwpKJhWcdMpZrGeH8Y4K1Yh8B8udUT7G9pEuVFRARiragZE6W9g7hsq9HSa4YCM5?cluster=devnet), slot 524398076 |
| Devnet smoke | Done, all 7 transactions finalized, see `docs/SMOKE_devnet.json` |
| Localnet deploy + smoke | Done 20 Nov 2026, see `docs/SMOKE_localnet.json` |

## Devnet deploy (20 Nov 2026)

Funding: **5 devnet SOL from faucet.solana.com (GitHub sign-in), 20 Nov 2026 ~15:29 UK**
to `HKJhkAZdkdNAmET7JoLHyTmDioka3CGYdajqaM31Wdgd`.

| Step | Payer balance (SOL) |
|---|---|
| After faucet claim | 5.00000000 |
| After deploy (program data 0.42922444 + program account + fees) | 4.56950244 |
| After smoke test (mint, token account, market, vault, position, fees) | 4.56207628 |

Total used: 0.43792372 SOL, in line with the ≈0.438 SOL estimate below. No
leftover write buffer. The on-chain program bytes match the local build
(84,320 bytes, sha256 `249abfa2…6b0e91b`). Program:
[7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz](https://explorer.solana.com/address/7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz?cluster=devnet), program data `FqdHAJL8iphPjD5TTQ4gLNt9Ap6NgdbMtH1NG3wCc8rL`,
upgrade authority = the box wallet. Cluster version 4.5.0-beta.0.

The smoke test ran the same flow as on localnet with the same results: subsidy
109,861,229 base units, 40-share round trip paid 15,181,215 and returned
15,181,214 (net −1, maker-protective), 25 winning shares redeemed for 25,000,000.

## Size cut (20 Nov 2026)

Release profile builds of the SBF program (`cargo build-sbf`, platform-tools v1.57):

| Profile | .so bytes |
|---|---|
| opt-level = 3, lto = "fat", codegen-units = 1 (M1 build) | 85,944 |
| opt-level = "s" | 89,296 |
| opt-level = "s" + strip + panic = "abort" | 89,440 |
| opt-level = "z" + strip + panic = "abort" | 87,056 |
| opt-level = "z" | 85,600 |
| opt-level = 2 | 84,616 |
| **opt-level = "z" + formatted `msg!` replaced by `sol_log_64`** (current) | **84,320** |

`cargo build-sbf` already strips the deployed `.so`; adding `strip = true` and
`panic = "abort"` made it larger, so they are left off. `overflow-checks = true`,
`lto = "fat"` and `codegen-units = 1` are kept. The remaining size is mostly the
256-bit `mul_div`, `u128` division intrinsics and the instruction dispatcher.
All 18 tests still pass; n = 8 compute units went from 226k/398k to 247k/435k
(buy/sell), under the 600k the client requests (the test now asserts < 600k).

## Devnet SOL needed (devnet rent: 5,080 lamports per byte-year, 2-year exemption)

`solana rent` on devnet:

| Account | Bytes | Lamports |
|---|---|---|
| Program data (45 + 84,320), default `--max-len` = program length | 84,365 | 429,224,440 |
| Write buffer (37 + 84,320), refunded at deploy | 84,357 | 429,183,800 |
| Program account | 36 | 833,120 |
| Program data with `--max-len` = 2 × program (upgrade headroom) | 168,685 | 857,570,040 |
| Fees: 88 transactions × 5,000 (measured on localnet) | | 440,000 |

The upgradeable loader drains the buffer back to the payer **before** it funds
the program-data account, so the buffer and the program data are never held at
the same time. Verified on a fresh local validator: a payer funded with only
0.5907 SOL (one program-data rent at localnet rates + program account + fees)
deployed successfully and kept 0.00105 SOL.

| Option | Peak SOL needed | Kept after deploy |
|---|---|---|
| **Upgradeable loader, default `--max-len` (= program length)** | **0.4305** (buffer 0.4292 + program 0.0008 + fees 0.0004) | 0.4301 locked in program data/program |
| Upgradeable loader, `--max-len` 2 × program | 0.8588 | 0.8584 |
| Buffer and program data held at once (naive estimate used on 19 Nov) | 0.8597 | n/a, not how the loader works |

Cheapest: the default `--max-len`. A `--max-len` smaller than the program is
not possible. Loader v4 (`solana program-v4`) is not in this CLI (agave 4.4.1).
Note: any later upgrade of a larger build would need a new program id or an
`extend` (more rent); this deploy is for M1 evidence.

Smoke test accounts on devnet: mint 1,066,800 + token account 1,488,440 + market
1,991,360 + vault 1,488,440 + position 1,341,120 + ~10 fees ≈ 7.5M lamports
(0.0075 SOL). `scripts/deploy-devnet.sh` refuses to start below 0.438 SOL
(`MIN_LAMPORTS`).

## Faucet claims

| Time (UK) | Source | Amount | Balance after |
|---|---|---|---|
| 20 Nov 2026 ~15:29 | faucet.solana.com (GitHub sign-in) | 5 SOL | 5 SOL |

### Jumpbit

The Jumpbit devnet faucet (page https://jumpbit.io/en/solana/devnet-faucet) has
no login and no captcha. The page calls
`POST https://api.jumpbit.io/api/v1/devnet-faucet/airdrop` with JSON
`{"address": "<pubkey>", "amount": 0.1}`; nothing else is required, so a daily
claim can be scripted over plain HTTP. Its stated limit is one request per
address per day, and failed requests may count.

| Time (UTC / UK) | Amount | Result | Tx | Balance after |
|---|---|---|---|---|
| 2026-11-20 12:42:29 UTC / 12:42 UK | 0.1 SOL requested | HTTP 503 `"The jumpbit devnet faucet is empty right now. Get test SOL from https://faucet.solana.com."` (code 5030) | none | 0 SOL |

At 0.1 SOL per day, 0.438 SOL takes 5 successful daily claims, once the faucet is refilled.

## Funding blocker (19 Nov 2026)

(Original 19 Nov notes. The ≈0.9 SOL figure below assumed the buffer and
program data are held at once; the measured need is ≈0.4305 SOL, see above.)

The payer has 0 devnet SOL. Free, sign-in-free funding was tried:

- `solana airdrop` / `requestAirdrop` on `https://api.devnet.solana.com`:
  `410 The RPC faucet is closed on this cluster. Request test SOL at https://faucet.solana.com`.
- faucet.solana.com: `GitHub authentication is required`.
- devnetfaucet.org: GitHub sign-in required.
- Ankr, Helius, dRPC devnet RPCs: need an API key or a paid plan.
- No-login web faucets found give at most 0.1 SOL per day, below the ≈0.9 SOL a deploy needs.

Once funded: `scripts/deploy-devnet.sh` deploys with the reserved program id
and runs the smoke test, which writes `docs/SMOKE_devnet.json`.

## Local validator run (20 Nov 2026)

`solana-test-validator` 4.4.1 running in the background with its ledger at
`/workspace/money/hackathons/solana-perps/localnet-ledger`. The 84,320-byte build
was deployed with `solana program deploy` (upgradeable loader) to the reserved
id, then `scripts/smoke.ts` ran create, buy, sell, buy, resolve and redeem.
Signatures, balances and round-trip numbers are in `docs/SMOKE_localnet.json`;
the console log is `docs/SMOKE_localnet_output.txt`. These are **localnet**
signatures, not devnet.

## Proof-of-work faucet check (19 Nov 2026, ~21:05 UK)

- `cargo install devnet-pow` (v0.1.4) fails to build on current Rust: it pins
  Solana 1.10/1.14 crates and an old `wasm-bindgen` ("older versions of the
  `wasm-bindgen` crate are incompatible with current versions of Rust").
- The on-chain PoW faucet program `PoWSNH2hEZogtCg1Zgm51FnkmJperzYDgPK4fvs8taL`
  has 24 faucet specs on devnet. All of their source accounts are empty
  except one that pays 100 lamports per solution (0.9997 SOL left) and one
  that holds 0.0009 SOL. Every useful faucet (0.01–1 SOL rewards) is drained,
  and claiming also needs the payer to have SOL for fees. The PoW route is dead.
- Other keyless HTTP faucets checked: solfaucet.com (front end for the closed
  public RPC faucet), Pine Stake (GitHub sign-in + Turnstile), Blueshift
  (wallet connect, NFT holders), QuickNode/Alchemy (mainnet balance and bot
  check), Jumpbit (no login, but 0.1 SOL per day, too little for a deploy).

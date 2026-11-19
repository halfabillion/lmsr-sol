# Devnet status

| Item | Value |
|---|---|
| Program id (reserved keypair, kept outside the repo) | `7uKjZAG4m5vFd77rWD1kasKV2YwQrzSaHeeFM6a7NWsz` |
| Payer (box wallet) | `HKJhkAZdkdNAmET7JoLHyTmDioka3CGYdajqaM31Wdgd` |
| Program size | 85,944 bytes (program data rent ≈ 0.436 SOL; ≈ 0.9 SOL needed during deploy for the write buffer) |
| Devnet deploy | **Pending devnet SOL**, see below |

## Funding blocker (19 Nov 2026)

The payer has 0 devnet SOL. Free, sign-in-free funding was tried:

- `solana airdrop` / `requestAirdrop` on `https://api.devnet.solana.com`:
  `410 The RPC faucet is closed on this cluster. Request test SOL at https://faucet.solana.com`.
- faucet.solana.com: `GitHub authentication is required`.
- devnetfaucet.org: GitHub sign-in required.
- Ankr, Helius, dRPC devnet RPCs: need an API key or a paid plan.
- No-login web faucets found give at most 0.1 SOL per day, below the ≈0.9 SOL a deploy needs.

Once funded: `scripts/deploy-devnet.sh` deploys with the reserved program id
and runs the smoke test, which writes `docs/SMOKE_devnet.json`.

## Local validator run

The same program and smoke script were run against `solana-test-validator`
(agave 4.4.1) with the program loaded at the reserved id. Signatures are in
`docs/SMOKE_localnet.json` and the log is `docs/SMOKE_localnet_output.txt`.
These are **localnet** signatures, not devnet.

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

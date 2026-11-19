#!/usr/bin/env bash
# Deploy lmsr-sol to Solana DEVNET and run the smoke test.
# Keys stay OUTSIDE the repo:
#   WALLET      payer keypair     (default ../keypair.json)
#   PROGRAM_KP  program keypair   (default ../.secrets/lmsr_sol-program-keypair.json)
# Needs about 0.9 SOL of devnet SOL (program data rent ~0.44 SOL plus a
# temporary write buffer of the same size, refunded after deploy).
set -euo pipefail
cd "$(dirname "$0")/.."
WALLET="${WALLET:-../keypair.json}"
PROGRAM_KP="${PROGRAM_KP:-../.secrets/lmsr_sol-program-keypair.json}"
URL=https://api.devnet.solana.com
WALLET="$(realpath "$WALLET")"
PROGRAM_KP="$(realpath "$PROGRAM_KP")"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

cargo build-sbf --manifest-path program/Cargo.toml
rm -f target/deploy/lmsr_sol-keypair.json   # never keep a program key in the repo tree
PAYER=$(solana address -k "$WALLET")
PROGRAM_ID=$(solana address -k "$PROGRAM_KP")
echo "payer $PAYER, program $PROGRAM_ID"
solana balance "$PAYER" --url "$URL"
solana program deploy target/deploy/lmsr_sol.so \
  --program-id "$PROGRAM_KP" --keypair "$WALLET" --fee-payer "$WALLET" --url "$URL"
solana program show "$PROGRAM_ID" --url "$URL"
cd scripts && RPC_URL="$URL" CLUSTER=devnet PROGRAM_ID="$PROGRAM_ID" WALLET="$WALLET" npx tsx smoke.ts

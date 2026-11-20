#!/usr/bin/env bash
# Deploy lmsr-sol to Solana DEVNET and run the smoke test.
# Keys stay OUTSIDE the repo:
#   WALLET      payer keypair     (default ../keypair.json)
#   PROGRAM_KP  program keypair   (default ../.secrets/lmsr_sol-program-keypair.json)
#   BUFFER_KP   write-buffer keypair, created if missing, so an interrupted
#               deploy can be resumed and no recovery phrase is printed
#               (default ../.secrets/lmsr_sol-buffer-keypair.json)
# Devnet SOL needed (measured, see docs/DEVNET.md): ~0.4305 SOL for the deploy
# (the loader drains the buffer back to the payer before funding program data),
# plus ~0.0075 SOL for the smoke test. MIN_LAMPORTS guards the start.
set -euo pipefail
cd "$(dirname "$0")/.."
WALLET="${WALLET:-../keypair.json}"
PROGRAM_KP="${PROGRAM_KP:-../.secrets/lmsr_sol-program-keypair.json}"
BUFFER_KP="${BUFFER_KP:-../.secrets/lmsr_sol-buffer-keypair.json}"
MIN_LAMPORTS="${MIN_LAMPORTS:-438000000}"
URL=https://api.devnet.solana.com
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
WALLET="$(realpath "$WALLET")"
PROGRAM_KP="$(realpath "$PROGRAM_KP")"
if [ ! -f "$BUFFER_KP" ]; then
  solana-keygen new --no-bip39-passphrase --silent -o "$BUFFER_KP" >/dev/null 2>&1
  chmod 600 "$BUFFER_KP"
fi
BUFFER_KP="$(realpath "$BUFFER_KP")"

cargo build-sbf --manifest-path program/Cargo.toml
rm -f target/deploy/lmsr_sol-keypair.json   # never keep a program key in the repo tree
PAYER=$(solana address -k "$WALLET")
PROGRAM_ID=$(solana address -k "$PROGRAM_KP")
BAL=$(solana balance "$PAYER" --url "$URL" --lamports | grep -oE '^[0-9]+')
echo "payer $PAYER, program $PROGRAM_ID, balance $BAL lamports (need >= $MIN_LAMPORTS)"
if [ "$BAL" -lt "$MIN_LAMPORTS" ]; then
  echo "Not enough devnet SOL yet; not deploying." >&2
  exit 2
fi
solana program deploy target/deploy/lmsr_sol.so \
  --program-id "$PROGRAM_KP" --buffer "$BUFFER_KP" \
  --keypair "$WALLET" --fee-payer "$WALLET" --url "$URL"
solana program show "$PROGRAM_ID" --url "$URL" -k "$WALLET"
cd scripts && RPC_URL="$URL" CLUSTER=devnet PROGRAM_ID="$PROGRAM_ID" WALLET="$WALLET" npx tsx smoke.ts

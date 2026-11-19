// Devnet / localnet smoke test for lmsr-sol: create a test SPL mint, create a
// 3-outcome market, buy, sell, resolve and redeem. Prints transaction
// signatures and writes them to docs/SMOKE_<cluster>.json.
//
// Env:
//   RPC_URL     (default https://api.devnet.solana.com)
//   CLUSTER     label for explorer links / output file (default devnet)
//   PROGRAM_ID  deployed program id (required)
//   WALLET      path to the payer keypair file (kept OUTSIDE the repo)
import {
  ComputeBudgetProgram, Connection, Keypair, PublicKey, SystemProgram,
  Transaction, TransactionInstruction, sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID, createMint, getOrCreateAssociatedTokenAccount, mintTo, getAccount,
} from "@solana/spl-token";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const RPC = process.env.RPC_URL ?? "https://api.devnet.solana.com";
const CLUSTER = process.env.CLUSTER ?? "devnet";
const PROGRAM_ID = new PublicKey(process.env.PROGRAM_ID ?? "");
const WALLET = process.env.WALLET ?? "";
if (!WALLET) throw new Error("set WALLET to the payer keypair path");

const WAD = 10n ** 18n;
const DECIMALS = 6;
const UNIT = 10n ** BigInt(DECIMALS);

const conn = new Connection(RPC, "confirmed");
const payer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(WALLET, "utf8"))));

const u64 = (v: bigint) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(v); return b; };
const u128 = (v: bigint) => { const b = Buffer.alloc(16); b.writeBigUInt64LE(v & ((1n << 64n) - 1n)); b.writeBigUInt64LE(v >> 64n, 8); return b; };

const pda = (seeds: Buffer[]) => PublicKey.findProgramAddressSync(seeds, PROGRAM_ID)[0];

const sigs: Record<string, string> = {};
async function send(label: string, ix: TransactionInstruction) {
  const tx = new Transaction().add(ComputeBudgetProgram.setComputeUnitLimit({ units: 600_000 }), ix);
  const sig = await sendAndConfirmTransaction(conn, tx, [payer], { commitment: "confirmed" });
  sigs[label] = sig;
  console.log(`${label}: ${sig}`);
  return sig;
}

async function main() {
  console.log(`cluster=${CLUSTER} program=${PROGRAM_ID.toBase58()} payer=${payer.publicKey.toBase58()}`);
  console.log(`payer balance: ${(await conn.getBalance(payer.publicKey)) / 1e9} SOL`);

  // Test collateral: a fresh 6-decimal SPL mint controlled by the payer.
  const mint = await createMint(conn, payer, payer.publicKey, null, DECIMALS);
  console.log(`test mint: ${mint.toBase58()}`);
  const ata = await getOrCreateAssociatedTokenAccount(conn, payer, mint, payer.publicKey);
  sigs["mint_to"] = await mintTo(conn, payer, mint, ata.address, payer, 10_000n * UNIT);

  const marketId = BigInt(Date.now());
  const market = pda([Buffer.from("market"), payer.publicKey.toBuffer(), u64(marketId)]);
  const vault = pda([Buffer.from("vault"), market.toBuffer()]);
  const position = pda([Buffer.from("position"), market.toBuffer(), payer.publicKey.toBuffer()]);

  const k = (pubkey: PublicKey, isSigner = false, isWritable = true) => ({ pubkey, isSigner, isWritable });

  await send("create_market", new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [k(payer.publicKey, true), k(market), k(vault), k(mint, false, false), k(ata.address),
      k(TOKEN_PROGRAM_ID, false, false), k(SystemProgram.programId, false, false)],
    data: Buffer.concat([Buffer.from([0]), u64(marketId), Buffer.from([3]), u128(100n * WAD)]),
  }));
  const vaultAfterCreate = (await getAccount(conn, vault)).amount;
  console.log(`market ${market.toBase58()} vault ${vault.toBase58()} subsidy ${vaultAfterCreate}`);

  const tradeKeys = (withSystem: boolean) => [k(payer.publicKey, true), k(market), k(position), k(vault),
    k(ata.address), k(TOKEN_PROGRAM_ID, false, false),
    ...(withSystem ? [k(SystemProgram.programId, false, false)] : [])];

  const bal = async () => (await getAccount(conn, ata.address)).amount;
  const b0 = await bal();
  await send("buy", new TransactionInstruction({
    programId: PROGRAM_ID, keys: tradeKeys(true),
    data: Buffer.concat([Buffer.from([1, 0]), u64(40n * UNIT), u64(1000n * UNIT)]),
  }));
  const b1 = await bal();
  await send("sell", new TransactionInstruction({
    programId: PROGRAM_ID, keys: tradeKeys(false),
    data: Buffer.concat([Buffer.from([2, 0]), u64(40n * UNIT), u64(0n)]),
  }));
  const b2 = await bal();
  console.log(`round trip 40 shares: paid ${b0 - b1}, got back ${b2 - b1}, net ${b2 - b0} (base units)`);
  if (b2 >= b0) throw new Error("round trip did not lose money");

  await send("buy_again", new TransactionInstruction({
    programId: PROGRAM_ID, keys: tradeKeys(true),
    data: Buffer.concat([Buffer.from([1, 2]), u64(25n * UNIT), u64(1000n * UNIT)]),
  }));
  await send("resolve", new TransactionInstruction({
    programId: PROGRAM_ID, keys: [k(payer.publicKey, true, false), k(market)], data: Buffer.from([3, 2]),
  }));
  const b3 = await bal();
  await send("redeem", new TransactionInstruction({
    programId: PROGRAM_ID, keys: tradeKeys(false), data: Buffer.from([4]),
  }));
  const b4 = await bal();
  console.log(`redeemed ${b4 - b3} base units for 25 winning shares`);

  const ex = (s: string, kind = "tx") =>
    `https://explorer.solana.com/${kind}/${s}${CLUSTER === "devnet" ? "?cluster=devnet" : "?cluster=custom"}`;
  const out = {
    cluster: CLUSTER, rpc: RPC, program_id: PROGRAM_ID.toBase58(), payer: payer.publicKey.toBase58(),
    mint: mint.toBase58(), market: market.toBase58(), vault: vault.toBase58(), market_id: marketId.toString(),
    subsidy_base_units: vaultAfterCreate.toString(),
    round_trip: { paid: (b0 - b1).toString(), got_back: (b2 - b1).toString(), net: (b2 - b0).toString() },
    redeemed: (b4 - b3).toString(),
    signatures: sigs,
    explorer: Object.fromEntries(Object.entries(sigs).map(([l, s]) => [l, ex(s)])),
    at: new Date().toISOString(),
  };
  const file = resolve(import.meta.dirname, "..", "docs", `SMOKE_${CLUSTER}.json`);
  writeFileSync(file, JSON.stringify(out, null, 2) + "\n");
  console.log(`wrote ${file}`);
}

main().catch((e) => { console.error(e); process.exit(1); });

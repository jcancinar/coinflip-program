#!/usr/bin/env node
"use strict";

const {
  AddressLookupTableProgram,
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  sendAndConfirmTransaction,
} = require("@solana/web3.js");
const fs = require("fs");

const PROGRAM_ID = new PublicKey("FLipUd6iHP9KTncLCt6Jo9bXXaz4yV4SuDpZdgRqVRHe");
const RAYDIUM_CLMM = new PublicKey("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");
const ORAO_VRF = new PublicKey("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
const TOKEN_PROGRAM = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const TOKEN_2022 = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const ATA_PROGRAM = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const MEMO_PROGRAM = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const SYSTEM_PROGRAM = new PublicKey("11111111111111111111111111111111");
const WSOL = new PublicKey("So11111111111111111111111111111111111111112");
const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

const POOLS = [
  "3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv",
  "8aDaBQkTrS6HVMjyc6EZebgdiaXhLYGriDWKWWp1NpFF",
  "AHNN6JmvaGG6XUoSg7sEr38gRYDB2jTbUvqXVuqaRHpq",
  "49iMatQtoyabsYAQc8GafVq6aeBFVDxSRH44oiatyyw6",
  "3L7KbPVaAQA4UTecaGQYsm6UCq5F3sZM9zAYkxqYt63j",
  "6m5aXAve4uh6Kt4ytKyCLWNMjd8PYP5vujwNCtycrUiD",
  "RyhF4cksVZY7vcqJpoytHcxcGNKRp27PEGhSnEPpbGv",
  "6truu3rZuiB9rKQg4VYC3Dt3QwV7DgwGqXrYUcrvnDDE",
  "DXWbip5LducMAbDSSpLYz9Xik3253EPeAYQufQtx7LXs",
];

const MINTS = [
  WSOL,
  USDC,
  new PublicKey("XsDoVfqeBukxuZHWhdvWHBhgEHjGNst4MLodqsJHzoB"),
  new PublicKey("Xs3oZwbHvqis4NYcf4YKWmEia2eC84wSiVrcYcTqpH8"),
  new PublicKey("Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh"),
  new PublicKey("Xsa62P5mvPszXL1krVUnU5ar38bBSVcWAB6fmPCo5Zu"),
  new PublicKey("Xs3eBt7uRfJX8QUs4suhyU8p2M6DoUDrJyWBa8LLZsg"),
  new PublicKey("XsP7xzNPvEHS1m6qfanPUGjNmdnmsLKEoNAnHjdxxyZ"),
  new PublicKey("XsoCS1TfEyfFhfvj8EtZ528L3CaKBDBRqRapnBbDF2W"),
  new PublicKey("XsvNBAYkrDRNhA7wPHQfX3ZUXZyZLdnCQDfHZ56bzpg"),
];

function uniqueKeys(keys) {
  const seen = new Set();
  const out = [];
  for (const key of keys) {
    const id = key.toBase58();
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(key);
  }
  return out;
}

function chunks(items, size) {
  const out = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}

async function send(connection, payer, ixs) {
  const tx = new Transaction().add(...ixs);
  return sendAndConfirmTransaction(connection, tx, [payer], { commitment: "confirmed" });
}

async function main() {
  const url = process.env.ANCHOR_PROVIDER_URL;
  const walletPath = process.env.ANCHOR_WALLET;
  if (!url || !walletPath) throw new Error("set ANCHOR_PROVIDER_URL and ANCHOR_WALLET");
  const connection = new Connection(url, "confirmed");
  const payer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8"))));

  const keys = [
    SYSTEM_PROGRAM,
    TOKEN_PROGRAM,
    TOKEN_2022,
    ATA_PROGRAM,
    MEMO_PROGRAM,
    RAYDIUM_CLMM,
    ORAO_VRF,
    PROGRAM_ID,
    ...MINTS,
    PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID)[0],
    PublicKey.findProgramAddressSync([Buffer.from("orao-vrf-network-configuration")], ORAO_VRF)[0],
  ];
  for (const mint of MINTS) {
    if (mint.equals(WSOL)) continue;
    keys.push(PublicKey.findProgramAddressSync([Buffer.from("token"), mint.toBuffer()], PROGRAM_ID)[0]);
  }

  const vrfNetwork = PublicKey.findProgramAddressSync(
    [Buffer.from("orao-vrf-network-configuration")],
    ORAO_VRF
  )[0];
  const vrfInfo = await connection.getAccountInfo(vrfNetwork);
  if (vrfInfo && vrfInfo.data.length >= 72) {
    keys.push(new PublicKey(vrfInfo.data.subarray(40, 72)));
  }

  const poolInfos = await connection.getMultipleAccountsInfo(POOLS.map((p) => new PublicKey(p)));
  for (let i = 0; i < POOLS.length; i++) {
    const pool = new PublicKey(POOLS[i]);
    const info = poolInfos[i];
    if (!info || info.data.length < 233) throw new Error(`pool missing ${POOLS[i]}`);
    keys.push(
      pool,
      new PublicKey(info.data.subarray(9, 41)),
      new PublicKey(info.data.subarray(137, 169)),
      new PublicKey(info.data.subarray(169, 201)),
      new PublicKey(info.data.subarray(201, 233)),
      PublicKey.findProgramAddressSync(
        [Buffer.from("pool_tick_array_bitmap_extension"), pool.toBuffer()],
        RAYDIUM_CLMM
      )[0]
    );
  }

  const addresses = uniqueKeys(keys);
  const slot = await connection.getSlot("finalized");
  const [createIx, lookupTable] = AddressLookupTableProgram.createLookupTable({
    authority: payer.publicKey,
    payer: payer.publicKey,
    recentSlot: slot,
  });
  const createSig = await send(connection, payer, [createIx]);
  console.log("created", lookupTable.toBase58(), createSig);

  for (const batch of chunks(addresses, 20)) {
    const ix = AddressLookupTableProgram.extendLookupTable({
      lookupTable,
      authority: payer.publicKey,
      payer: payer.publicKey,
      addresses: batch,
    });
    const sig = await send(connection, payer, [ix]);
    console.log("extended", batch.length, sig);
  }

  console.log("LOOKUP_TABLE", lookupTable.toBase58());
  console.log("addresses", addresses.length);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

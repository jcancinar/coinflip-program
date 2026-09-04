#!/usr/bin/env node
"use strict";

const anchor = require("@coral-xyz/anchor");
const { PublicKey } = require("@solana/web3.js");
const idl = require("../target/idl/coinflip.json");

const USDC = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const WSOL = new PublicKey("So11111111111111111111111111111111111111112");
const SOL_USDC_POOL = new PublicKey("3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv");
const SOL_MIN = new anchor.BN(10_000_000); // 0.01 SOL
const SOL_MAX = new anchor.BN(1_040_000_000); // ~1 SOL advertised (~$100) with fee pad

const FEE_BPS = 350;
const KEEP_BPS = 10_000 - FEE_BPS;

function pad(advertised) {
  return advertised.muln(10_000).addn(KEEP_BPS - 1).divn(KEEP_BPS);
}

const TOKENS = [
  {
    symbol: "USDC",
    mint: USDC,
    pool: SOL_USDC_POOL,
    quote: WSOL,
    min: new anchor.BN(1_000_000),
    max: pad(new anchor.BN(100_000_000)),
  },
  {
    symbol: "TSLAx",
    mint: "XsDoVfqeBukxuZHWhdvWHBhgEHjGNst4MLodqsJHzoB",
    pool: "8aDaBQkTrS6HVMjyc6EZebgdiaXhLYGriDWKWWp1NpFF",
    min: new anchor.BN(300_000),
    max: pad(new anchor.BN(30_000_000)),
  },
  {
    symbol: "SPCXx",
    mint: "Xs3oZwbHvqis4NYcf4YKWmEia2eC84wSiVrcYcTqpH8",
    pool: "AHNN6JmvaGG6XUoSg7sEr38gRYDB2jTbUvqXVuqaRHpq",
    min: new anchor.BN(700_000),
    max: pad(new anchor.BN(70_000_000)),
  },
  {
    symbol: "NVDAx",
    mint: "Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh",
    pool: "49iMatQtoyabsYAQc8GafVq6aeBFVDxSRH44oiatyyw6",
    min: new anchor.BN(500_000),
    max: pad(new anchor.BN(50_000_000)),
  },
  {
    symbol: "AAPLx",
    mint: "XsbEhLAtcf6HdfpFZ5xEMdqW8nfAvcsP5bdudRLJzJp",
    pool: "CKwJZwm7oj3nu4653N1EpDrqXbXAYXoPFiPeEnLouF8y",
    min: new anchor.BN(500_000),
    max: pad(new anchor.BN(50_000_000)),
  },
  {
    symbol: "METAx",
    mint: "Xsa62P5mvPszXL1krVUnU5ar38bBSVcWAB6fmPCo5Zu",
    pool: "3L7KbPVaAQA4UTecaGQYsm6UCq5F3sZM9zAYkxqYt63j",
    min: new anchor.BN(200_000),
    max: pad(new anchor.BN(20_000_000)),
  },
  {
    symbol: "AMZNx",
    mint: "Xs3eBt7uRfJX8QUs4suhyU8p2M6DoUDrJyWBa8LLZsg",
    pool: "6m5aXAve4uh6Kt4ytKyCLWNMjd8PYP5vujwNCtycrUiD",
    min: new anchor.BN(400_000),
    max: pad(new anchor.BN(40_000_000)),
  },
  {
    symbol: "MSTRx",
    mint: "XsP7xzNPvEHS1m6qfanPUGjNmdnmsLKEoNAnHjdxxyZ",
    pool: "RyhF4cksVZY7vcqJpoytHcxcGNKRp27PEGhSnEPpbGv",
    min: new anchor.BN(700_000),
    max: pad(new anchor.BN(70_000_000)),
  },
  {
    symbol: "SPYx",
    mint: "XsoCS1TfEyfFhfvj8EtZ528L3CaKBDBRqRapnBbDF2W",
    pool: "6truu3rZuiB9rKQg4VYC3Dt3QwV7DgwGqXrYUcrvnDDE",
    min: new anchor.BN(200_000),
    max: pad(new anchor.BN(20_000_000)),
  },
  {
    symbol: "HOODx",
    mint: "XsvNBAYkrDRNhA7wPHQfX3ZUXZyZLdnCQDfHZ56bzpg",
    pool: "DXWbip5LducMAbDSSpLYz9Xik3253EPeAYQufQtx7LXs",
    min: new anchor.BN(900_000),
    max: pad(new anchor.BN(90_000_000)),
  },
];

async function main() {
  const provider = anchor.AnchorProvider.env();
  const program = new anchor.Program(idl, provider);
  const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);

  try {
    const sig = await program.methods
      .migrateConfig()
      .accountsPartial({ authority: provider.wallet.publicKey })
      .rpc();
    console.log("migrate_config", sig);
  } catch (err) {
    console.log("migrate_config skipped", err.message ?? err);
  }

  let config = await program.account.config.fetch(configPda);
  if (!config.solMinAmount.eq(SOL_MIN)) {
    const sig = await program.methods
      .setSolMinAmount(SOL_MIN)
      .accountsPartial({ authority: provider.wallet.publicKey })
      .rpc();
    console.log("SOL min 0.01", sig);
  } else {
    console.log("SOL min already 0.01");
  }

  config = await program.account.config.fetch(configPda);
  if (!config.solMaxAmount || !config.solMaxAmount.eq(SOL_MAX)) {
    const sig = await program.methods
      .setSolMaxAmount(SOL_MAX)
      .accountsPartial({ authority: provider.wallet.publicKey })
      .rpc();
    console.log("SOL max 1.04", sig);
  } else {
    console.log("SOL max already 1.04");
  }

  for (const token of TOKENS) {
    const mint = new PublicKey(token.mint);
    const pool = new PublicKey(token.pool);
    const quote = new PublicKey(token.quote ?? USDC);
    const [tokenPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .migrateTokenConfig(mint)
        .accountsPartial({ authority: provider.wallet.publicKey })
        .rpc();
      console.log(`${token.symbol} migrated`, tokenPda.toBase58());
    } catch (err) {
      console.log(`${token.symbol} migrate skipped`, err.message ?? err);
    }

    try {
      const existing = await program.account.tokenConfig.fetch(tokenPda);
      if (
        existing.isEnabled &&
        existing.pool.equals(pool) &&
        existing.quoteMint.equals(quote) &&
        existing.minAmount.eq(token.min) &&
        existing.maxAmount &&
        existing.maxAmount.eq(token.max)
      ) {
        console.log(`${token.symbol} already set`, tokenPda.toBase58());
        continue;
      }
    } catch {
      // not created yet
    }

    const sig = await program.methods
      .enableToken(mint, token.min, token.max, true, pool, quote, false)
      .accountsPartial({ authority: provider.wallet.publicKey })
      .rpc();
    console.log(`${token.symbol} min/max updated`, sig, tokenPda.toBase58());
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

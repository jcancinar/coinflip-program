#!/usr/bin/env node
"use strict";

const anchor = require("@coral-xyz/anchor");
const { PublicKey } = require("@solana/web3.js");
const idl = require("../target/idl/coinflip.json");

const PROGRAM_ID = new PublicKey(idl.address);
const ORAO_VRF = new PublicKey("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");

async function main() {
  const provider = anchor.AnchorProvider.env();
  const program = new anchor.Program(idl, provider);
  if (!program.programId.equals(PROGRAM_ID)) {
    throw new Error(`IDL program id ${program.programId.toBase58()} != ${PROGRAM_ID.toBase58()}`);
  }

  const [config] = PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);
  try {
    const existing = await program.account.config.fetch(config);
    console.log("config already initialized");
    console.log("  authority ", existing.authority.toBase58());
    console.log("  vrf       ", existing.vrfProgram.toBase58());
    console.log("  fee_bps   ", existing.feeBps);
    return;
  } catch {
    // not initialized
  }

  const sig = await program.methods
    .initialize(ORAO_VRF)
    .accountsPartial({ authority: provider.wallet.publicKey })
    .rpc();
  console.log("initialized", sig);
  console.log("  authority", provider.wallet.publicKey.toBase58());
  console.log("  vrf      ", ORAO_VRF.toBase58());
  console.log("  config   ", config.toBase58());
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

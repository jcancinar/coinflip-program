#!/usr/bin/env bash
# Deploy only coinflip.so to mainnet. Does not deploy mock_vrf.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PAYER_PUB="AvYwZap1pLodHSC9oi4evEgVwdjb2NsgBHYtRPzPgFGT"
RPC="${RPC_URL:-https://api.mainnet-beta.solana.com}"
PAYER="${PAYER_KEYPAIR:-$ROOT/.deploy/payer.json}"
PROGRAM_KP="$ROOT/target/deploy/coinflip-keypair.json"
SO="$ROOT/target/deploy/coinflip.so"
MIN_SOL="${MIN_SOL:-7}"
PROGRAM_ID="$(solana-keygen pubkey "$PROGRAM_KP")"
DECLARED="$(sed -n 's/.*declare_id!("\([^"]*\)").*/\1/p' "$ROOT/src/lib.rs")"

if [[ ! -f "$PAYER" ]]; then
  echo "missing payer keypair: $PAYER"
  echo "convert the Flip Deployer secret:"
  echo "  python3 ../pk-to-json.py '<base58>' -o .deploy/payer.json"
  exit 1
fi

got_payer="$(solana-keygen pubkey "$PAYER")"
if [[ "$PROGRAM_ID" != "$DECLARED" ]]; then
  echo "program keypair is $PROGRAM_ID, declare_id! is $DECLARED"
  exit 1
fi
if [[ "$PROGRAM_ID" == "sc6TuL2w5UWBM9ygRZbH1MVjc7oLHZTgv1mg3Q1c21E" ]]; then
  echo "refusing to deploy the old devnet program id"
  exit 1
fi
if [[ "$got_payer" != "$PAYER_PUB" ]]; then
  echo "payer is $got_payer, expected Flip Deployer $PAYER_PUB"
  exit 1
fi

bal="$(solana balance "$got_payer" --url "$RPC" | awk '{print $1}')"
python3 -c "import sys; bal=float('$bal'); min=float('$MIN_SOL'); sys.exit(0 if bal>=min else 1)" || {
  echo "payer has ${bal} SOL, need at least ${MIN_SOL} SOL on mainnet"
  exit 1
}

echo "program  $PROGRAM_ID"
echo "payer    $got_payer  (${bal} SOL)"
echo "rpc      $RPC"
echo "building without --features devnet"
anchor build

echo "deploying coinflip.so only"
solana program deploy "$SO" \
  --program-id "$PROGRAM_KP" \
  --url "$RPC" \
  --keypair "$PAYER"

echo
echo "deployed. initialize next (authority = payer):"
echo "  cd $ROOT"
echo "  ANCHOR_PROVIDER_URL=$RPC ANCHOR_WALLET=$PAYER node scripts/initialize-mainnet.js"

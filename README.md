# Coinflip

Peer-to-peer Solana coin flip. Program id `FLipUd6iHP9KTncLCt6Jo9bXXaz4yV4SuDpZdgRqVRHe`.

**House** is `config.authority` (fee receiver / upgrade authority): `AvYwZap1pLodHSC9oi4evEgVwdjb2NsgBHYtRPzPgFGT`.

## House-only games

`create` takes `house_only`.

- Creator must pick Heads or Tails (Open is rejected).
- Only the house can `join` or `participate`.
- The API resolver auto-joins open house-only games (same mint and amount, opposite side). Set `HOUSE_PRIVATE_KEY` in `api/.env` to the house wallet.

## Stake limits

| Limit | Applies to | Set with |
| --- | --- | --- |
| **Min** | Every flip | `sol_min_amount` / `enable_token(..., min_amount, ...)` |
| **Max** | House-only creates only | `set_sol_max_amount` / `enable_token(..., max_amount, ...)` |

On-chain max includes a small fee pad so an advertised max button still clears `create`. `0` means no cap (until you set one after migrate).

Current advertised caps (~$100 per house-only flip):

| Asset | Min | House-only max |
| --- | --- | --- |
| SOL | 0.01 | 1 |
| USDC | 1 | 100 |
| TSLAx | 0.003 | 0.3 |
| SPCXx | 0.007 | 0.7 |
| NVDAx | 0.005 | 0.5 |
| METAx | 0.002 | 0.2 |
| AMZNx | 0.004 | 0.4 |
| MSTRx | 0.007 | 0.7 |
| SPYx | 0.002 | 0.2 |
| HOODx | 0.009 | 0.9 |

## Deploy

```bash
# upgrade program (payer = house)
./scripts/deploy-mainnet.sh

# resize config + token accounts, set SOL max, write token min/max
ANCHOR_PROVIDER_URL=$RPC_URL ANCHOR_WALLET=.deploy/payer.json node scripts/enable-xstocks.js
```

Do not use `anchor deploy` on mainnet (it also deploys `mock_vrf`). After an upgrade that grows `Game`, settle or cancel open games first — old game accounts will not decode.

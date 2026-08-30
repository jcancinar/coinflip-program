import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Coinflip } from "../target/types/coinflip";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SYSVAR_SLOT_HASHES_PUBKEY } from "@solana/web3.js";
import { MockVrf } from "../target/types/mock_vrf";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotent,
  createMint,
  getAccount,
  getAssociatedTokenAddressSync,
  mintTo,
  transfer,
} from "@solana/spl-token";
import { createHash } from "crypto";
import { assert, expect } from "chai";

const RESULT_PREFIX = Buffer.from("coinflip_p2p_v1");
const MOCK_VRF_PREFIX = Buffer.from("coinflip_mock_vrf");
const MOCK_VRF_ID = new PublicKey("8DeY7XokDSxSC5nJjwuoYW8Ajjo36UH9XXpu247tcJWf");
const AMOUNT = new anchor.BN(50_000_000);
const DEFAULT_FEE_BPS = 350;
const BPS_DENOMINATOR = 10_000;
const WSOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");
const DUMMY_POOL = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

function potFee(amount: anchor.BN, feeBps = DEFAULT_FEE_BPS): number {
  return amount.muln(2).muln(feeBps).divn(BPS_DENOMINATOR).toNumber();
}

type Side = { heads: {} } | { tails: {} } | { open: {} };

function sha256(...parts: Buffer[]): Buffer {
  const hash = createHash("sha256");
  for (const part of parts) {
    hash.update(part);
  }
  return hash.digest();
}

function resultBit(
  creatorEntropy: Buffer,
  joinerEntropy: Buffer,
  vrfFirst32: Buffer
): number {
  return sha256(RESULT_PREFIX, creatorEntropy, joinerEntropy, vrfFirst32)[0] & 1;
}

function mockVrfFirst32(game: PublicKey): Buffer {
  return sha256(MOCK_VRF_PREFIX, game.toBuffer());
}

function vrfRequestPda(game: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("orao-vrf-randomness-request"), game.toBuffer()],
    MOCK_VRF_ID
  )[0];
}

function vrfNetworkPda(): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("orao-vrf-network-configuration")],
    MOCK_VRF_ID
  )[0];
}

function entropy(fill: number): Buffer {
  const buf = Buffer.alloc(32, fill);
  buf[0] = fill || 1;
  return buf;
}

describe("coinflip", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Coinflip as Program<Coinflip>;
  const mockVrf = anchor.workspace.MockVrf as Program<MockVrf>;
  const connection = provider.connection;
  const owner = (provider.wallet as anchor.Wallet).payer;

  const resolver = Keypair.generate();
  const creator = Keypair.generate();
  const joiner = Keypair.generate();

  const [configPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId
  );

  let nonce = 0;

  function nextNonce(): anchor.BN {
    nonce += 1;
    return new anchor.BN(nonce);
  }

  function gamePdaFor(creatorPk: PublicKey, gameNonce: anchor.BN): PublicKey {
    const [pda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("game"),
        creatorPk.toBuffer(),
        gameNonce.toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    return pda;
  }

  async function expectError(fn: () => Promise<unknown>, code: string) {
    try {
      await fn();
      assert.fail(`expected error ${code}`);
    } catch (err: any) {
      const got = err.error?.errorCode?.code ?? err.toString();
      expect(String(got), err.toString()).to.include(code);
    }
  }

  async function createGame(params: {
    creator: Keypair;
    amount?: anchor.BN;
    side?: Side;
    creatorEntropy?: Buffer;
    gameNonce?: anchor.BN;
    mint?: PublicKey;
    tokenAccounts?: Record<string, PublicKey>;
  }) {
    const gameNonce = params.gameNonce ?? nextNonce();
    const creatorEntropy = params.creatorEntropy ?? entropy(7);
    const amount = params.amount ?? AMOUNT;
    const side = params.side ?? { heads: {} };
    const mint = params.mint ?? PublicKey.default;
    const game = gamePdaFor(params.creator.publicKey, gameNonce);

    await program.methods
      .create(amount, side, Array.from(creatorEntropy), gameNonce, mint)
      .accountsPartial({
        creator: params.creator.publicKey,
        tokenConfig: null,
        mintAccount: null,
        creatorToken: null,
        vault: null,
        tokenProgram: null,
        associatedTokenProgram: null,
        ...(params.tokenAccounts ?? {}),
      })
      .signers([params.creator])
      .rpc();

    return { game, gameNonce, creatorEntropy, amount, side, mint };
  }

  async function joinGame(params: {
    game: PublicKey;
    joiner: Keypair;
    joinerSide?: Side;
    joinerEntropy?: Buffer;
    amount?: anchor.BN;
    tokenAccounts?: Record<string, PublicKey>;
  }) {
    const joinerEntropy = params.joinerEntropy ?? entropy(11);
    await program.methods
      .join(
        params.joinerSide ?? { open: {} },
        Array.from(joinerEntropy),
        params.amount ?? AMOUNT
      )
      .accountsPartial({
        game: params.game,
        joiner: params.joiner.publicKey,
        mintAccount: null,
        joinerToken: null,
        vault: null,
        tokenProgram: null,
        vrfProgram: MOCK_VRF_ID,
        vrfNetwork: vrfNetworkPda(),
        vrfTreasury: owner.publicKey,
        vrfRequest: vrfRequestPda(params.game),
        ...(params.tokenAccounts ?? {}),
      })
      .signers([params.joiner])
      .rpc();
    return joinerEntropy;
  }

  async function initiateCancel(game: PublicKey) {
    await program.methods
      .initiateCancel()
      .accountsPartial({
        game,
        authority: owner.publicKey,
      })
      .rpc();
  }

  async function waitUntilSlot(slot: number) {
    for (let i = 0; i < 200; i++) {
      if ((await connection.getSlot("processed")) >= slot) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error(`timed out waiting for slot ${slot}`);
  }

  async function cancelGame(
    game: PublicKey,
    signer: Keypair | { publicKey: PublicKey },
    tokenAccounts?: Record<string, PublicKey>
  ) {
    const signers = "secretKey" in signer ? [signer as Keypair] : [];
    await program.methods
      .cancel()
      .accountsPartial({
        game,
        creator: creator.publicKey,
        signer: signer.publicKey,
        mintAccount: null,
        vault: null,
        creatorToken: null,
        tokenProgram: null,
        ...(tokenAccounts ?? {}),
      })
      .signers(signers)
      .rpc();
  }

  async function resolveGame(
    game: PublicKey,
    winner: PublicKey,
    tokenAccounts?: Record<string, PublicKey>
  ) {
    await program.methods
      .resolve()
      .accountsPartial({
        game,
        winner,
        settler: owner.publicKey,
        feeRecipient: owner.publicKey,
        mintAccount: null,
        vault: null,
        winnerToken: null,
        feeRecipientToken: null,
        tokenProgram: null,
        vrfRequest: vrfRequestPda(game),
        ...(tokenAccounts ?? {}),
      })
      .rpc();
  }

  async function winnerForGame(game: PublicKey): Promise<PublicKey> {
    const state = await program.account.game.fetch(game);
    const creatorEntropy = Buffer.from(state.creatorEntropy);
    const joinerEntropy = Buffer.from(state.joinerEntropy);
    const bit = resultBit(creatorEntropy, joinerEntropy, mockVrfFirst32(game));
    const creatorSide = "heads" in state.creatorSide ? 1 : 0;
    return bit === creatorSide ? state.creator : state.joiner;
  }

  async function expectResolvedPayout(params: {
    game: PublicKey;
    winner?: PublicKey;
    amount?: anchor.BN;
    feeBps?: number;
  }) {
    const winner = params.winner ?? (await winnerForGame(params.game));
    const amount = params.amount ?? AMOUNT;
    const fee = potFee(amount, params.feeBps);
    const pot = await connection.getBalance(params.game);
    const winnerBefore = await connection.getBalance(winner);
    const ownerBefore = await connection.getBalance(owner.publicKey);

    await resolveGame(params.game, winner);

    assert.isNull(await program.account.game.fetchNullable(params.game));
    assert.equal(await connection.getBalance(params.game), 0);
    assert.equal(
      await connection.getBalance(winner),
      winnerBefore + pot - fee
    );
    // Provider wallet pays the resolve tx fee, so owner net is fee minus that cost.
    assert.closeTo(
      await connection.getBalance(owner.publicKey),
      ownerBefore + fee,
      20_000
    );
  }

  before(async () => {
    for (const kp of [resolver, creator, joiner]) {
      const sig = await connection.requestAirdrop(kp.publicKey, 10 * LAMPORTS_PER_SOL);
      await connection.confirmTransaction(sig, "confirmed");
    }

    await program.methods
      .initialize(resolver.publicKey)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();

    await mockVrf.methods
      .initialize()
      .accountsPartial({ payer: owner.publicKey })
      .rpc();

    await program.methods
      .setVrfProgram(MOCK_VRF_ID)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const config = await program.account.config.fetch(configPda);
    assert.equal(config.authority.toBase58(), owner.publicKey.toBase58());
    assert.equal(config.resolver.toBase58(), resolver.publicKey.toBase58());
    assert.equal(config.feeBps, DEFAULT_FEE_BPS);
    assert.isFalse(config.paused);
    assert.equal(config.solMinAmount.toString(), "10000000");
  });

  it("create + cancel refunds the creator and closes the game", async () => {
    const before = await connection.getBalance(creator.publicKey);
    const { game } = await createGame({ creator });

    const pot = await connection.getBalance(game);
    assert.isAbove(pot, AMOUNT.toNumber());

    await cancelGame(game, creator);

    const closed = await program.account.game.fetchNullable(game);
    assert.isNull(closed);

    const after = await connection.getBalance(creator.publicKey);
    assert.isAbove(after, before - 20_000_000);
    assert.equal(await connection.getBalance(game), 0);
  });

  it("create + join + resolve pays the creator when they win", async () => {
    const creatorEntropy = entropy(21);
    const { game } = await createGame({
      creator,
      side: { heads: {} },
      creatorEntropy,
    });

    const joinerEntropy = entropy(11);
    await joinGame({
      game,
      joiner,
      joinerSide: { tails: {} },
      joinerEntropy,
    });

    const gameState = await program.account.game.fetch(game);
    assert.deepEqual(gameState.status, { ready: {} });
    assert.equal(gameState.joiner.toBase58(), joiner.publicKey.toBase58());
    assert.isAbove(Number(gameState.joinSlot), 0);

    await expectResolvedPayout({
      game,
    });
  });

  it("rejects refund while the join slot hash is still available", async () => {
    const { game } = await createGame({ creator });
    await joinGame({ game, joiner });
    await winnerForGame(game);
    await expectError(
      () =>
        program.methods
          .refundExpired()
          .accountsPartial({
            game,
            creator: creator.publicKey,
            joiner: joiner.publicKey,
            mintAccount: null,
            vault: null,
            creatorToken: null,
            joinerToken: null,
            tokenProgram: null,
            slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
            vrfRequest: vrfRequestPda(game),
          })
          .rpc(),
      "CancelNotAllowed"
    );
    await expectResolvedPayout({ game });
  });

  it("create + join + resolve pays the joiner when they win", async () => {
    const creatorEntropy = entropy(31);
    const { game } = await createGame({
      creator,
      side: { open: {} },
      creatorEntropy,
    });

    const joinerEntropy = entropy(11);
    await joinGame({
      game,
      joiner,
      joinerSide: { heads: {} },
      joinerEntropy,
    });

    const gameState = await program.account.game.fetch(game);
    assert.deepEqual(gameState.creatorSide, { tails: {} });
    assert.deepEqual(gameState.joinerSide, { heads: {} });

    await expectResolvedPayout({
      game,
    });
  });

  it("join works without a resolver", async () => {
    const { game } = await createGame({ creator });
    await joinGame({ game, joiner });
    await expectResolvedPayout({ game });
  });

  it("rejects resolve on an open game", async () => {
    const { game } = await createGame({ creator });
    await expectError(
      () => resolveGame(game, creator.publicKey),
      "NotReady"
    );
  });

  it("rejects the creator joining their own game", async () => {
    const { game } = await createGame({ creator });
    await expectError(
      () => joinGame({ game, joiner: creator }),
      "CannotJoinOwnGame"
    );
  });

  it("rejects a wrong join amount", async () => {
    const { game } = await createGame({ creator });
    await expectError(
      () =>
        joinGame({
          game,
          joiner,
          amount: AMOUNT.addn(1),
        }),
      "AmountMismatch"
    );
  });

  it("pause blocks create and join but not resolve", async () => {
    const creatorEntropy = entropy(51);
    const ready = await createGame({
      creator,
      side: { tails: {} },
      creatorEntropy,
    });
    const joinerEntropy = entropy(11);
    await joinGame({
      game: ready.game,
      joiner,
      joinerSide: { heads: {} },
      joinerEntropy,
    });

    const open = await createGame({ creator });

    await program.methods
      .setPaused(true)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();

    await expectError(
      () => createGame({ creator }),
      "Paused"
    );
    await expectError(
      () => joinGame({ game: open.game, joiner }),
      "Paused"
    );

    await expectResolvedPayout({
      game: ready.game,
    });

    await cancelGame(open.game, creator);

    await program.methods
      .setPaused(false)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();
  });

  it("owner can cancel an open game and refunds the creator", async () => {
    const stranger = Keypair.generate();
    const sig = await connection.requestAirdrop(stranger.publicKey, LAMPORTS_PER_SOL);
    await connection.confirmTransaction(sig, "confirmed");

    const { game } = await createGame({ creator });
    const pot = await connection.getBalance(game);
    const creatorBefore = await connection.getBalance(creator.publicKey);

    await expectError(
      () => cancelGame(game, stranger),
      "Unauthorized"
    );
    await expectError(
      () => cancelGame(game, owner),
      "AuthorityCancelNotInitiated"
    );

    await initiateCancel(game);
    const pending = await program.account.game.fetch(game);
    assert.isAbove(Number(pending.cancelAfterSlot), 0);
    await expectError(
      () => cancelGame(game, owner),
      "AuthorityCancelNotReady"
    );

    await waitUntilSlot(Number(pending.cancelAfterSlot));
    await cancelGame(game, owner);

    assert.isNull(await program.account.game.fetchNullable(game));
    assert.equal(await connection.getBalance(creator.publicKey), creatorBefore + pot);
    assert.equal(await connection.getBalance(game), 0);
  });

  it("join still works after the authority initiates cancel", async () => {
    const { game } = await createGame({ creator });
    await initiateCancel(game);
    await joinGame({ game, joiner });
    await expectResolvedPayout({ game });
  });

  it("owner can change fee BPS for future games only", async () => {
    const oldFeeBps = 350;
    const newFeeBps = 500;

    const creatorEntropyA = entropy(61);
    const gameA = await createGame({
      creator,
      creatorEntropy: creatorEntropyA,
    });
    const createdA = await program.account.game.fetch(gameA.game);
    assert.equal(createdA.feeBps, oldFeeBps);

    await program.methods
      .setFeeBps(newFeeBps)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();

    const config = await program.account.config.fetch(configPda);
    assert.equal(config.feeBps, newFeeBps);

    const creatorEntropyB = entropy(63);
    const gameB = await createGame({
      creator,
      creatorEntropy: creatorEntropyB,
    });
    const createdB = await program.account.game.fetch(gameB.game);
    assert.equal(createdB.feeBps, newFeeBps);

    await joinGame({
      game: gameA.game,
      joiner,
      joinerSide: { tails: {} },
      joinerEntropy: entropy(11),
    });

    await joinGame({
      game: gameB.game,
      joiner,
      joinerSide: { tails: {} },
      joinerEntropy: entropy(11),
    });

    await expectResolvedPayout({
      game: gameA.game,
      feeBps: oldFeeBps,
    });
    await expectResolvedPayout({
      game: gameB.game,
      feeBps: newFeeBps,
    });

    await program.methods
      .setFeeBps(DEFAULT_FEE_BPS)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();
  });

  it("rejects unauthorized and out-of-range fee updates", async () => {
    await expectError(
      () =>
        program.methods
          .setFeeBps(400)
          .accountsPartial({
            authority: creator.publicKey,
          })
          .signers([creator])
          .rpc(),
      "Unauthorized"
    );

    await expectError(
      () =>
        program.methods
          .setFeeBps(501)
          .accountsPartial({
            authority: owner.publicKey,
          })
          .rpc(),
      "InvalidFeeBps"
    );
  });

  it("owner can transfer authority", async () => {
    const next = Keypair.generate();
    const sig = await connection.requestAirdrop(next.publicKey, LAMPORTS_PER_SOL);
    await connection.confirmTransaction(sig, "confirmed");

    await expectError(
      () =>
        program.methods
          .setAuthority(next.publicKey)
          .accountsPartial({ authority: creator.publicKey })
          .signers([creator])
          .rpc(),
      "Unauthorized"
    );
    await expectError(
      () =>
        program.methods
          .setAuthority(PublicKey.default)
          .accountsPartial({ authority: owner.publicKey })
          .rpc(),
      "Unauthorized"
    );

    await program.methods
      .setAuthority(next.publicKey)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();
    assert.equal(
      (await program.account.config.fetch(configPda)).authority.toBase58(),
      next.publicKey.toBase58()
    );

    await expectError(
      () =>
        program.methods
          .setFeeBps(400)
          .accountsPartial({ authority: owner.publicKey })
          .rpc(),
      "Unauthorized"
    );

    await program.methods
      .setAuthority(owner.publicKey)
      .accountsPartial({ authority: next.publicKey })
      .signers([next])
      .rpc();
    assert.equal(
      (await program.account.config.fetch(configPda)).authority.toBase58(),
      owner.publicKey.toBase58()
    );
  });

  it("owner can enable, update, and disable a token", async () => {
    const mint = Keypair.generate().publicKey;
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    const minAmount = new anchor.BN(1_000_000);

    await expectError(
      () =>
        program.methods
          .enableToken(mint, minAmount, true, DUMMY_POOL, WSOL_MINT, false)
          .accountsPartial({
            authority: creator.publicKey,
          })
          .signers([creator])
          .rpc(),
      "Unauthorized"
    );

    await expectError(
      () =>
        program.methods
          .enableToken(mint, new anchor.BN(0), true, DUMMY_POOL, WSOL_MINT, false)
          .accountsPartial({
            authority: owner.publicKey,
          })
          .rpc(),
      "InvalidMinAmount"
    );

    await expectError(
      () =>
        program.methods
          .enableToken(mint, minAmount, true, PublicKey.default, WSOL_MINT, false)
          .accountsPartial({
            authority: owner.publicKey,
          })
          .rpc(),
      "PoolRequired"
    );

    await program.methods
      .enableToken(mint, minAmount, true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();

    let tokenConfig = await program.account.tokenConfig.fetch(tokenConfigPda);
    assert.equal(tokenConfig.mint.toBase58(), mint.toBase58());
    assert.equal(tokenConfig.minAmount.toString(), minAmount.toString());
    assert.isTrue(tokenConfig.isEnabled);
    assert.equal(tokenConfig.pool.toBase58(), DUMMY_POOL.toBase58());
    assert.equal(tokenConfig.quoteMint.toBase58(), WSOL_MINT.toBase58());
    assert.isFalse(tokenConfig.crossDisabled);

    const updatedMin = new anchor.BN(2_500_000);
    await program.methods
      .enableToken(mint, updatedMin, false, PublicKey.default, PublicKey.default, false)
      .accountsPartial({
        authority: owner.publicKey,
      })
      .rpc();

    tokenConfig = await program.account.tokenConfig.fetch(tokenConfigPda);
    assert.equal(tokenConfig.minAmount.toString(), updatedMin.toString());
    assert.isFalse(tokenConfig.isEnabled);
  });

  it("token game create + join + resolve pays the winner minus fee", async () => {
    const tokenAmount = new anchor.BN(2_000_000);
    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );

    await program.methods
      .enableToken(mint, new anchor.BN(1_000_000), true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    const joinerAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      joiner.publicKey
    );
    const ownerAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      owner.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);
    await mintTo(connection, owner, mint, joinerAta, owner, 10_000_000);

    const creatorEntropy = entropy(71);
    const gameNonce = nextNonce();
    const game = gamePdaFor(creator.publicKey, gameNonce);
    const vault = getAssociatedTokenAddressSync(mint, game, true);

    const { game: created } = await createGame({
      creator,
      amount: tokenAmount,
      creatorEntropy,
      gameNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });
    assert.equal(created.toBase58(), game.toBase58());

    const gameState = await program.account.game.fetch(game);
    assert.equal(gameState.mint.toBase58(), mint.toBase58());
    assert.equal(gameState.tokenDecimals, 6);
    assert.equal((await getAccount(connection, vault)).amount.toString(), tokenAmount.toString());

    const joinerEntropy = entropy(11);
    await joinGame({
      game,
      joiner,
      joinerSide: { tails: {} },
      joinerEntropy,
      amount: tokenAmount,
      tokenAccounts: {
        mintAccount: mint,
        joinerToken: joinerAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
      },
    });

    assert.equal(
      (await getAccount(connection, vault)).amount.toString(),
      tokenAmount.muln(2).toString()
    );

    const fee = potFee(tokenAmount);
    const winner = await winnerForGame(game);
    const winnerToken = winner.equals(creator.publicKey) ? creatorAta : joinerAta;
    const winnerBefore = (await getAccount(connection, winnerToken)).amount;
    const ownerBefore = (await getAccount(connection, ownerAta)).amount;

    await resolveGame(game, winner, {
      mintAccount: mint,
      vault,
      winnerToken,
      feeRecipientToken: ownerAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    });

    assert.isNull(await program.account.game.fetchNullable(game));
    assert.equal(
      (await getAccount(connection, winnerToken)).amount.toString(),
      (winnerBefore + BigInt(tokenAmount.muln(2).toNumber() - fee)).toString()
    );
    assert.equal(
      (await getAccount(connection, ownerAta)).amount.toString(),
      (ownerBefore + BigInt(fee)).toString()
    );
  });

  it("token game cancel refunds the creator", async () => {
    const tokenAmount = new anchor.BN(2_000_000);
    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .enableToken(mint, new anchor.BN(1_000_000), true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);

    const gameNonce = nextNonce();
    const game = gamePdaFor(creator.publicKey, gameNonce);
    const vault = getAssociatedTokenAddressSync(mint, game, true);
    const before = (await getAccount(connection, creatorAta)).amount;

    await createGame({
      creator,
      amount: tokenAmount,
      gameNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });

    await cancelGame(game, creator, {
      mintAccount: mint,
      vault,
      creatorToken: creatorAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    });

    assert.isNull(await program.account.game.fetchNullable(game));
    assert.equal((await getAccount(connection, creatorAta)).amount.toString(), before.toString());
  });

  it("token vault dust does not block cancel or resolve", async () => {
    const tokenAmount = new anchor.BN(2_000_000);
    const dust = 1;
    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .enableToken(mint, new anchor.BN(1_000_000), true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    const joinerAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      joiner.publicKey
    );
    const ownerAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      owner.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);
    await mintTo(connection, owner, mint, joinerAta, owner, 10_000_000);
    await mintTo(connection, owner, mint, ownerAta, owner, 1_000);

    const cancelNonce = nextNonce();
    const cancelGamePda = gamePdaFor(creator.publicKey, cancelNonce);
    const cancelVault = getAssociatedTokenAddressSync(mint, cancelGamePda, true);
    const cancelBefore = (await getAccount(connection, creatorAta)).amount;

    await createGame({
      creator,
      amount: tokenAmount,
      gameNonce: cancelNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault: cancelVault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });
    await transfer(connection, owner, ownerAta, cancelVault, owner, dust);
    await cancelGame(cancelGamePda, creator, {
      mintAccount: mint,
      vault: cancelVault,
      creatorToken: creatorAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    });
    assert.isNull(await program.account.game.fetchNullable(cancelGamePda));
    assert.equal(
      (await getAccount(connection, creatorAta)).amount.toString(),
      (cancelBefore + BigInt(dust)).toString()
    );

    const creatorEntropy = entropy(91);
    const gameNonce = nextNonce();
    const game = gamePdaFor(creator.publicKey, gameNonce);
    const vault = getAssociatedTokenAddressSync(mint, game, true);

    await createGame({
      creator,
      amount: tokenAmount,
      creatorEntropy,
      gameNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });
    const joinerEntropy = entropy(11);
    await joinGame({
      game,
      joiner,
      joinerSide: { tails: {} },
      joinerEntropy,
      amount: tokenAmount,
      tokenAccounts: {
        mintAccount: mint,
        joinerToken: joinerAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
      },
    });
    await transfer(connection, owner, ownerAta, vault, owner, dust);

    const fee = potFee(tokenAmount);
    const winner = await winnerForGame(game);
    const winnerToken = winner.equals(creator.publicKey) ? creatorAta : joinerAta;
    const winnerBefore = (await getAccount(connection, winnerToken)).amount;
    await resolveGame(game, winner, {
      mintAccount: mint,
      vault,
      winnerToken,
      feeRecipientToken: ownerAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    });
    assert.isNull(await program.account.game.fetchNullable(game));
    assert.equal(
      (await getAccount(connection, winnerToken)).amount.toString(),
      (winnerBefore + BigInt(tokenAmount.muln(2).toNumber() - fee + dust)).toString()
    );
  });

  it("rejects token games that are disabled or below the minimum", async () => {
    await expectError(
      () => createGame({ creator, mint: Keypair.generate().publicKey }),
      "TokenNotEnabled"
    );

    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);

    await program.methods
      .enableToken(mint, new anchor.BN(5_000_000), true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const gameNonce = nextNonce();
    await expectError(
      () =>
        createGame({
          creator,
          amount: new anchor.BN(1_000_000),
          gameNonce,
          mint,
          tokenAccounts: {
            tokenConfig: tokenConfigPda,
            mintAccount: mint,
            creatorToken: creatorAta,
            vault: getAssociatedTokenAddressSync(
              mint,
              gamePdaFor(creator.publicKey, gameNonce),
              true
            ),
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          },
        }),
      "AmountBelowMinimum"
    );
  });

  it("rejects SOL games below the configured minimum and lets the owner change it for new games only", async () => {
    await expectError(
      () =>
        createGame({
          creator,
          amount: new anchor.BN(1_000_000),
        }),
      "AmountBelowMinimum"
    );

    const open = await createGame({ creator });

    const raised = new anchor.BN(100_000_000);
    await program.methods
      .setSolMinAmount(raised)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    let config = await program.account.config.fetch(configPda);
    assert.equal(config.solMinAmount.toString(), raised.toString());

    await joinGame({ game: open.game, joiner });
    const joined = await program.account.game.fetch(open.game);
    assert.equal(joined.amount.toString(), AMOUNT.toString());

    await expectError(
      () => createGame({ creator, amount: AMOUNT }),
      "AmountBelowMinimum"
    );

    const above = await createGame({ creator, amount: raised });
    assert.equal((await program.account.game.fetch(above.game)).amount.toString(), raised.toString());
    await cancelGame(above.game, creator);

    await expectError(
      () =>
        program.methods
          .setSolMinAmount(raised)
          .accountsPartial({ authority: creator.publicKey })
          .signers([creator])
          .rpc(),
      "Unauthorized"
    );
    await expectError(
      () =>
        program.methods
          .setSolMinAmount(new anchor.BN(0))
          .accountsPartial({ authority: owner.publicKey })
          .rpc(),
      "InvalidMinAmount"
    );

    await program.methods
      .setSolMinAmount(new anchor.BN(10_000_000))
      .accountsPartial({ authority: owner.publicKey })
      .rpc();
  });

  it("owner can set the SOL-USDC pool", async () => {
    const usdc = Keypair.generate().publicKey;
    await program.methods
      .setSolUsdcPool(usdc, DUMMY_POOL)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const config = await program.account.config.fetch(configPda);
    assert.equal(config.usdcMint.toBase58(), usdc.toBase58());
    assert.equal(config.solUsdcPool.toBase58(), DUMMY_POOL.toBase58());

    await expectError(
      () =>
        program.methods
          .setSolUsdcPool(usdc, DUMMY_POOL)
          .accountsPartial({ authority: creator.publicKey })
          .signers([creator])
          .rpc(),
      "Unauthorized"
    );
  });

  it("participate joins a SOL game with the same mint", async () => {
    const { game } = await createGame({ creator });

    await program.methods
      .participate(
        { open: {} },
        Array.from(entropy(81)),
        AMOUNT,
        PublicKey.default,
        AMOUNT,
        new anchor.BN(0),
        0
      )
      .accountsPartial({
        game,
        joiner: joiner.publicKey,
        gameTokenConfig: null,
        mintAccount: null,
        joinerToken: null,
        joinerPayToken: null,
        vault: null,
        wsolAccount: null,
        tokenProgram: null,
        associatedTokenProgram: null,
        raydiumProgram: null,
        vrfProgram: MOCK_VRF_ID,
        vrfNetwork: vrfNetworkPda(),
        vrfTreasury: owner.publicKey,
        vrfRequest: vrfRequestPda(game),
      })
      .signers([joiner])
      .rpc();

    const state = await program.account.game.fetch(game);
    assert.equal(state.joiner.toBase58(), joiner.publicKey.toBase58());
    assert.equal(Object.keys(state.status)[0], "ready");
  });

  it("participate joins a token game with the same mint", async () => {
    const tokenAmount = new anchor.BN(2_000_000);
    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .enableToken(mint, new anchor.BN(1_000_000), true, DUMMY_POOL, WSOL_MINT, false)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    const joinerAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      joiner.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);
    await mintTo(connection, owner, mint, joinerAta, owner, 10_000_000);

    const gameNonce = nextNonce();
    const game = gamePdaFor(creator.publicKey, gameNonce);
    const vault = getAssociatedTokenAddressSync(mint, game, true);

    await createGame({
      creator,
      amount: tokenAmount,
      gameNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });

    await program.methods
      .participate(
        { open: {} },
        Array.from(entropy(82)),
        tokenAmount,
        mint,
        tokenAmount,
        new anchor.BN(0),
        0
      )
      .accountsPartial({
        game,
        joiner: joiner.publicKey,
        gameTokenConfig: tokenConfigPda,
        mintAccount: mint,
        joinerToken: joinerAta,
        joinerPayToken: joinerAta,
        vault,
        wsolAccount: null,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        raydiumProgram: null,
        vrfProgram: MOCK_VRF_ID,
        vrfNetwork: vrfNetworkPda(),
        vrfTreasury: owner.publicKey,
        vrfRequest: vrfRequestPda(game),
      })
      .signers([joiner])
      .rpc();

    const state = await program.account.game.fetch(game);
    assert.equal(state.joiner.toBase58(), joiner.publicKey.toBase58());
    assert.equal((await getAccount(connection, vault)).amount.toString(), tokenAmount.muln(2).toString());
  });

  it("participate rejects cross-pay when the token disables it", async () => {
    const tokenAmount = new anchor.BN(2_000_000);
    const mint = await createMint(connection, owner, owner.publicKey, null, 6);
    const [tokenConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .enableToken(mint, new anchor.BN(1_000_000), true, DUMMY_POOL, WSOL_MINT, true)
      .accountsPartial({ authority: owner.publicKey })
      .rpc();

    const creatorAta = await createAssociatedTokenAccountIdempotent(
      connection,
      owner,
      mint,
      creator.publicKey
    );
    await mintTo(connection, owner, mint, creatorAta, owner, 10_000_000);

    const gameNonce = nextNonce();
    const game = gamePdaFor(creator.publicKey, gameNonce);
    const vault = getAssociatedTokenAddressSync(mint, game, true);

    await createGame({
      creator,
      amount: tokenAmount,
      gameNonce,
      mint,
      tokenAccounts: {
        tokenConfig: tokenConfigPda,
        mintAccount: mint,
        creatorToken: creatorAta,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      },
    });

    await expectError(
      () =>
        program.methods
          .participate(
            { open: {} },
            Array.from(entropy(84)),
            tokenAmount,
            PublicKey.default,
            tokenAmount,
            new anchor.BN(0),
            0
          )
          .accountsPartial({
            game,
            joiner: joiner.publicKey,
            gameTokenConfig: tokenConfigPda,
            mintAccount: mint,
            joinerToken: null,
            joinerPayToken: null,
            vault,
            wsolAccount: null,
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
            raydiumProgram: null,
            vrfProgram: MOCK_VRF_ID,
            vrfNetwork: vrfNetworkPda(),
            vrfTreasury: owner.publicKey,
            vrfRequest: vrfRequestPda(game),
          })
          .signers([joiner])
          .rpc(),
      "InvalidPayMint"
    );
  });

  it("participate rejects a pay mint that cannot route into the game", async () => {
    const { game } = await createGame({ creator });
    const otherMint = Keypair.generate().publicKey;

    await expectError(
      () =>
        program.methods
          .participate(
            { open: {} },
            Array.from(entropy(83)),
            AMOUNT,
            otherMint,
            AMOUNT,
            new anchor.BN(0),
            0
          )
          .accountsPartial({
            game,
            joiner: joiner.publicKey,
            gameTokenConfig: null,
            mintAccount: null,
            joinerToken: null,
            joinerPayToken: null,
            vault: null,
            wsolAccount: null,
            tokenProgram: null,
            associatedTokenProgram: null,
            raydiumProgram: null,
            vrfProgram: MOCK_VRF_ID,
            vrfNetwork: vrfNetworkPda(),
            vrfTreasury: owner.publicKey,
            vrfRequest: vrfRequestPda(game),
          })
          .signers([joiner])
          .rpc(),
      "InvalidPayMint"
    );
  });
});

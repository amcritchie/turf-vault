import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { TurfVault } from "../target/types/turf_vault";
import {
  createMint,
  getAccount,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { expect } from "chai";
import { createHash } from "crypto";

describe("turf_vault verification matrix", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.TurfVault as Program<TurfVault>;
  const admin = provider.wallet as anchor.Wallet;
  const connection = provider.connection;

  const DECIMALS = 6;
  const MAX_CURRENCIES = 16;
  const DEFAULT_SEASON_ID = 1;
  const DEFAULT_SEED_SCHEDULE = [25, 19, 14, 10, 7];
  const QUEST_SEEDS = [12, 18, 45, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  const DEFAULT_PUBKEY = new PublicKey("11111111111111111111111111111111");

  let signer2: Keypair;
  let signer3: Keypair;
  let treasury: Keypair;
  let stranger: Keypair;
  let user1: Keypair;
  let user2: Keypair;

  let usdcMint: PublicKey;
  let usdtMint: PublicKey;
  let bonusMint: PublicKey;

  let vaultStatePda: PublicKey;
  let usdcOpRevPda: PublicKey;
  let usdtOpRevPda: PublicKey;
  let bonusOpRevPda: PublicKey;
  let defaultSeasonPda: PublicKey;

  let adminUsdcAta: PublicKey;
  let signer2UsdcAta: PublicKey;
  let treasuryUsdcAta: PublicKey;
  let treasuryUsdtAta: PublicKey;
  let wrongTreasuryUsdtAta: PublicKey;
  let user1UsdcAta: PublicKey;
  let user1UsdtAta: PublicKey;
  let user1BonusAta: PublicKey;
  let user2UsdcAta: PublicKey;
  let user2UsdtAta: PublicKey;

  let paidContest: ContestFixture;
  let bonusContest: ContestFixture;

  type ContestFixture = {
    id: Buffer;
    contestPda: PublicKey;
    prizePoolPda: PublicKey;
  };

  const amount = (tokens: number): number => tokens * 10 ** DECIMALS;
  const bn = (value: number | string): anchor.BN => new anchor.BN(value);
  const now = (): number => Math.floor(Date.now() / 1000);
  const tokenAmount = async (account: PublicKey): Promise<number> =>
    Number((await getAccount(connection, account)).amount);

  const bytes = (value: string, length: number): number[] => {
    const out = Buffer.alloc(length);
    out.write(value, 0, "utf8");
    return Array.from(out);
  };

  const username = (value: string): number[] => bytes(value, 32);
  const reason = (value: string): number[] => bytes(value, 64);
  const sourceRef = (value: string): number[] => bytes(value, 64);
  const sourceRefHash = (ref: number[]): number[] =>
    Array.from(createHash("sha256").update(Buffer.from(ref)).digest());

  const decodeFixedBytes = (value: number[] | Uint8Array): string =>
    Buffer.from(value).toString("utf8").replace(/\0+$/, "");

  const contestId = (slug: string): Buffer =>
    createHash("sha256").update(slug).digest();

  const u32Le = (value: number): Buffer => {
    const buffer = Buffer.alloc(4);
    buffer.writeUInt32LE(value);
    return buffer;
  };

  const feeSchedule = (
    fees: Record<number, number> = { 0: amount(9) }
  ): anchor.BN[] => {
    const values = Array.from({ length: MAX_CURRENCIES }, () => bn(0));
    for (const [idx, value] of Object.entries(fees)) {
      values[Number(idx)] = bn(value);
    }
    return values;
  };

  const deriveVault = (): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("vault")],
      program.programId
    )[0];

  const deriveOpRev = (mint: PublicKey): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("op_rev"), mint.toBuffer()],
      program.programId
    )[0];

  const deriveContest = (id: Buffer): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("contest"), id],
      program.programId
    )[0];

  const derivePrizePool = (id: Buffer): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("prize_pool"), id],
      program.programId
    )[0];

  const deriveEntry = (
    id: Buffer,
    wallet: PublicKey,
    entryNum: number
  ): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("entry"), id, wallet.toBuffer(), u32Le(entryNum)],
      program.programId
    )[0];

  const deriveUser = (wallet: PublicKey): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("user"), wallet.toBuffer()],
      program.programId
    )[0];

  const deriveSeason = (seasonId: number): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("season"), u32Le(seasonId)],
      program.programId
    )[0];

  const deriveEntryToken = (hash: number[]): PublicKey =>
    PublicKey.findProgramAddressSync(
      [Buffer.from("entry_token"), Buffer.from(hash)],
      program.programId
    )[0];

  const deriveSeedGrant = (
    wallet: PublicKey,
    kind: number,
    invitee: PublicKey
  ): PublicKey =>
    PublicKey.findProgramAddressSync(
      [
        Buffer.from("seed_grant"),
        wallet.toBuffer(),
        Buffer.from([kind]),
        invitee.toBuffer(),
      ],
      program.programId
    )[0];

  const statusName = (status: any): string => Object.keys(status)[0];

  const expectRejected = async (
    promise: Promise<unknown>,
    pattern: RegExp
  ): Promise<void> => {
    try {
      await promise;
      expect.fail(`expected rejection matching ${pattern}`);
    } catch (err: any) {
      expect(err.toString()).to.match(pattern);
    }
  };

  const fund = async (wallet: PublicKey, sol = 5): Promise<void> => {
    const tx = new anchor.web3.Transaction().add(
      SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: wallet,
        lamports: sol * LAMPORTS_PER_SOL,
      })
    );
    await provider.sendAndConfirm(tx);
  };

  const ata = async (
    mint: PublicKey,
    owner: PublicKey,
    initialAmount = 0
  ): Promise<PublicKey> => {
    const account = await getOrCreateAssociatedTokenAccount(
      connection,
      admin.payer,
      mint,
      owner
    );
    if (initialAmount > 0) {
      await mintTo(
        connection,
        admin.payer,
        mint,
        account.address,
        admin.publicKey,
        initialAmount
      );
    }
    return account.address;
  };

  const createUser = async (
    wallet: PublicKey,
    name: string,
    payer = admin.publicKey
  ): Promise<PublicKey> => {
    const userPda = deriveUser(wallet);
    await program.methods
      .createUserAccount(wallet, username(name) as any)
      .accountsStrict({
        payer,
        userAccount: userPda,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return userPda;
  };

  const createSeason = async (
    seasonId: number,
    name = "World Cup 2026"
  ): Promise<PublicKey> => {
    const seasonPda = deriveSeason(seasonId);
    await program.methods
      .createSeason(
        seasonId,
        bytes(name, 32) as any,
        DEFAULT_SEED_SCHEDULE.map((n) => bn(n)) as any,
        QUEST_SEEDS.map((n) => bn(n)) as any,
        bn(now())
      )
      .accountsStrict({
        admin: admin.publicKey,
        vaultState: vaultStatePda,
        season: seasonPda,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return seasonPda;
  };

  const createContest = async (
    slug: string,
    options: {
      fees?: Record<number, number>;
      maxEntries?: number;
      payouts?: number[];
      prizePool?: number;
      lockTimestamp?: number;
      payer?: PublicKey;
      creator?: PublicKey;
      creatorTokenAccount?: PublicKey;
      signers?: Keypair[];
    } = {}
  ): Promise<ContestFixture> => {
    const id = contestId(slug);
    const contestPda = deriveContest(id);
    const prizePoolPda = derivePrizePool(id);
    const prizePool = options.prizePool ?? amount(0);
    const payouts = options.payouts ?? (prizePool > 0 ? [prizePool] : []);

    await program.methods
      .createContest(
        Array.from(id) as any,
        DEFAULT_SEASON_ID,
        feeSchedule(options.fees) as any,
        options.maxEntries ?? 5,
        payouts.map((p) => bn(p)) as any,
        bn(prizePool),
        bn(options.lockTimestamp ?? 0)
      )
      .accountsStrict({
        payer: options.payer ?? admin.publicKey,
        creator: options.creator ?? admin.publicKey,
        vaultState: vaultStatePda,
        contest: contestPda,
        prizePool: prizePoolPda,
        payoutMint: usdcMint,
        creatorTokenAccount: options.creatorTokenAccount ?? adminUsdcAta,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers(options.signers ?? [])
      .rpc();

    return { id, contestPda, prizePoolPda };
  };

  const enterPaid = async (
    contest: ContestFixture,
    user: Keypair,
    userAccount: PublicKey,
    userTokenAccount: PublicKey,
    currencyMint: PublicKey,
    opRevAta: PublicKey,
    currencyIdx: number,
    entryNum: number
  ): Promise<PublicKey> => {
    const entryPda = deriveEntry(contest.id, user.publicKey, entryNum);
    await program.methods
      .enterContest(entryNum, currencyIdx)
      .accountsStrict({
        payer: admin.publicKey,
        user: user.publicKey,
        userAccount,
        vaultState: vaultStatePda,
        contest: contest.contestPda,
        contestEntry: entryPda,
        currencyMint,
        userTokenAccount,
        opRevAta,
        season: defaultSeasonPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();
    return entryPda;
  };

  const mintEntryToken = async (
    owner: PublicKey,
    refText: string
  ): Promise<{ pda: PublicKey; ref: number[]; hash: number[] }> => {
    const ref = sourceRef(refText);
    const hash = sourceRefHash(ref);
    const pda = deriveEntryToken(hash);
    await program.methods
      .mintEntryToken(0, ref as any, hash as any)
      .accountsStrict({
        admin: admin.publicKey,
        vaultState: vaultStatePda,
        userWallet: owner,
        entryToken: pda,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
    return { pda, ref, hash };
  };

  // `signer` omitted => the provider wallet (a vault signer) signs implicitly.
  // Pass a Keypair to burn AS someone else, which is how the auth case proves a
  // stranger is refused.
  const burnEntryToken = async (
    token: { pda: PublicKey; hash: number[] },
    signer?: Keypair
  ): Promise<void> => {
    await program.methods
      .burnEntryToken(token.hash as any)
      .accountsStrict({
        admin: signer ? signer.publicKey : admin.publicKey,
        vaultState: vaultStatePda,
        entryToken: token.pda,
      })
      .signers(signer ? [signer] : [])
      .rpc();
  };

  const enterWithToken = async (
    contest: ContestFixture,
    user: Keypair,
    userAccount: PublicKey,
    entryToken: PublicKey,
    entryNum: number
  ): Promise<PublicKey> => {
    const entryPda = deriveEntry(contest.id, user.publicKey, entryNum);
    await program.methods
      .enterContestWithToken(entryNum)
      .accountsStrict({
        payer: admin.publicKey,
        user: user.publicKey,
        userAccount,
        vaultState: vaultStatePda,
        contest: contest.contestPda,
        contestEntry: entryPda,
        entryToken,
        season: defaultSeasonPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();
    return entryPda;
  };

  const lockContestNow = async (contest: ContestFixture): Promise<void> => {
    await program.methods
      .setContestLockTime(bn(now() - 1))
      .accountsStrict({
        admin: admin.publicKey,
        cosigner: null,
        vaultState: vaultStatePda,
        contest: contest.contestPda,
      })
      .rpc();
  };

  before(async () => {
    signer2 = Keypair.generate();
    signer3 = Keypair.generate();
    treasury = Keypair.generate();
    stranger = Keypair.generate();
    user1 = Keypair.generate();
    user2 = Keypair.generate();

    for (const wallet of [signer2, signer3, treasury, stranger, user1, user2]) {
      await fund(wallet.publicKey, 10);
    }

    usdcMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey,
      null,
      DECIMALS
    );
    usdtMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey,
      null,
      DECIMALS
    );
    bonusMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey,
      null,
      DECIMALS
    );

    vaultStatePda = deriveVault();
    usdcOpRevPda = deriveOpRev(usdcMint);
    usdtOpRevPda = deriveOpRev(usdtMint);
    bonusOpRevPda = deriveOpRev(bonusMint);
    defaultSeasonPda = deriveSeason(DEFAULT_SEASON_ID);

    adminUsdcAta = await ata(usdcMint, admin.publicKey, amount(1_000));
    signer2UsdcAta = await ata(usdcMint, signer2.publicKey, amount(100));
    treasuryUsdcAta = await ata(usdcMint, treasury.publicKey);
    treasuryUsdtAta = await ata(usdtMint, treasury.publicKey);
    wrongTreasuryUsdtAta = await ata(usdtMint, user2.publicKey);
    user1UsdcAta = await ata(usdcMint, user1.publicKey, amount(100));
    user1UsdtAta = await ata(usdtMint, user1.publicKey, amount(100));
    user1BonusAta = await ata(bonusMint, user1.publicKey, amount(50));
    user2UsdcAta = await ata(usdcMint, user2.publicKey, amount(100));
    user2UsdtAta = await ata(usdtMint, user2.publicKey, amount(100));
  });

  describe("initialize", () => {
    it("creates VaultState and pins signers, treasury, USDC slot 0, and USDT slot 1", async () => {
      await program.methods
        .initialize(
          [admin.publicKey, signer2.publicKey, signer3.publicKey],
          2,
          treasury.publicKey
        )
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          payoutMint: usdcMint,
          secondCurrencyMint: usdtMint,
          payoutOpRevAta: usdcOpRevPda,
          secondOpRevAta: usdtOpRevPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .rpc();

      const vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.signers.map((s: PublicKey) => s.toBase58())).to.deep.equal([
        admin.publicKey.toBase58(),
        signer2.publicKey.toBase58(),
        signer3.publicKey.toBase58(),
      ]);
      expect(vault.threshold).to.equal(2);
      expect(vault.paused).to.equal(0);
      expect(vault.payoutMint.toBase58()).to.equal(usdcMint.toBase58());
      expect(vault.treasuryAuthority.toBase58()).to.equal(
        treasury.publicKey.toBase58()
      );
      expect(vault.acceptedCurrencies[0].mint.toBase58()).to.equal(
        usdcMint.toBase58()
      );
      expect(vault.acceptedCurrencies[0].opRevAta.toBase58()).to.equal(
        usdcOpRevPda.toBase58()
      );
      expect(vault.acceptedCurrencies[0].active).to.equal(1);
      expect(vault.acceptedCurrencies[1].mint.toBase58()).to.equal(
        usdtMint.toBase58()
      );
      expect(vault.acceptedCurrencies[1].opRevAta.toBase58()).to.equal(
        usdtOpRevPda.toBase58()
      );
      expect(vault.acceptedCurrencies[1].active).to.equal(1);
    });
  });

  describe("user accounts and usernames", () => {
    it("permissionless payer creates wallet user accounts", async () => {
      const user1Pda = await createUser(user1.publicKey, "user-one");
      const user2Pda = await createUser(user2.publicKey, "user-two");

      const account1 = await program.account.userAccount.fetch(user1Pda);
      const account2 = await program.account.userAccount.fetch(user2Pda);
      expect(account1.wallet.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(account1.seeds.toNumber()).to.equal(0);
      expect(account1.entries).to.equal(0);
      expect(decodeFixedBytes(account1.username)).to.equal("user-one");
      expect(account2.wallet.toBase58()).to.equal(user2.publicKey.toBase58());
    });

    it("enforces username owner, charset, length, and reserved-prefix rules", async () => {
      const user1Pda = deriveUser(user1.publicKey);

      await program.methods
        .setUsername(username("renamed-user") as any)
        .accountsStrict({ wallet: user1.publicKey, userAccount: user1Pda })
        .signers([user1])
        .rpc();

      const renamed = await program.account.userAccount.fetch(user1Pda);
      expect(decodeFixedBytes(renamed.username)).to.equal("renamed-user");

      await expectRejected(
        program.methods
          .setUsername(username("hacked") as any)
          .accountsStrict({ wallet: user2.publicKey, userAccount: user1Pda })
          .signers([user2])
          .rpc(),
        /ConstraintSeeds|Unauthorized|seeds/i
      );
      await expectRejected(
        program.methods
          .setUsername(username("admin-tom") as any)
          .accountsStrict({ wallet: user1.publicKey, userAccount: user1Pda })
          .signers([user1])
          .rpc(),
        /UsernameReserved/i
      );
      await expectRejected(
        program.methods
          .setUsername(username("ab") as any)
          .accountsStrict({ wallet: user1.publicKey, userAccount: user1Pda })
          .signers([user1])
          .rpc(),
        /UsernameTooShort/i
      );

      const invalid = new Array(32).fill(0);
      invalid[0] = 0x68;
      invalid[1] = 0x01;
      invalid[2] = 0x69;
      await expectRejected(
        program.methods
          .setUsername(invalid as any)
          .accountsStrict({ wallet: user1.publicKey, userAccount: user1Pda })
          .signers([user1])
          .rpc(),
        /UsernameInvalidChars/i
      );
    });

    it("admin username flows waive only reserved prefixes", async () => {
      const houseWallet = Keypair.generate();
      await fund(houseWallet.publicKey);
      const housePda = deriveUser(houseWallet.publicKey);

      await expectRejected(
        program.methods
          .createUserAccount(
            houseWallet.publicKey,
            username("turf-monster") as any
          )
          .accountsStrict({
            payer: admin.publicKey,
            userAccount: housePda,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /UsernameReserved/i
      );

      await expectRejected(
        program.methods
          .adminCreateUserAccount(
            houseWallet.publicKey,
            username("turf-monster") as any
          )
          .accountsStrict({
            payer: admin.publicKey,
            admin: stranger.publicKey,
            vaultState: vaultStatePda,
            userAccount: housePda,
            systemProgram: SystemProgram.programId,
          })
          .signers([stranger])
          .rpc(),
        /Unauthorized/i
      );

      await expectRejected(
        program.methods
          .adminCreateUserAccount(houseWallet.publicKey, username("ab") as any)
          .accountsStrict({
            payer: admin.publicKey,
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userAccount: housePda,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /UsernameTooShort/i
      );

      await program.methods
        .adminCreateUserAccount(
          houseWallet.publicKey,
          username("turf-monster") as any
        )
        .accountsStrict({
          payer: admin.publicKey,
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userAccount: housePda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      let account = await program.account.userAccount.fetch(housePda);
      expect(decodeFixedBytes(account.username)).to.equal("turf-monster");

      await expectRejected(
        program.methods
          .adminSetUsername(username("turf") as any)
          .accountsStrict({
            wallet: houseWallet.publicKey,
            admin: stranger.publicKey,
            vaultState: vaultStatePda,
            userAccount: housePda,
          })
          .signers([houseWallet, stranger])
          .rpc(),
        /Unauthorized/i
      );

      await program.methods
        .adminSetUsername(username("turf") as any)
        .accountsStrict({
          wallet: houseWallet.publicKey,
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userAccount: housePda,
        })
        .signers([houseWallet])
        .rpc();

      account = await program.account.userAccount.fetch(housePda);
      expect(decodeFixedBytes(account.username)).to.equal("turf");

      await expectRejected(
        program.methods
          .adminSetUsername(username("turf-x") as any)
          .accountsStrict({
            wallet: user1.publicKey,
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userAccount: housePda,
          })
          .signers([user1])
          .rpc(),
        /ConstraintSeeds|Unauthorized|seeds/i
      );
    });
  });

  describe("seasons and seed grants", () => {
    it("creates immutable season seed and quest schedules", async () => {
      defaultSeasonPda = await createSeason(DEFAULT_SEASON_ID);
      const season = await program.account.season.fetch(defaultSeasonPda);
      expect(season.seasonId).to.equal(DEFAULT_SEASON_ID);
      expect(decodeFixedBytes(season.name)).to.equal("World Cup 2026");
      expect(
        season.seedSchedule.map((s: anchor.BN) => s.toNumber())
      ).to.deep.equal(DEFAULT_SEED_SCHEDULE);
      expect(
        season.questSeeds.map((s: anchor.BN) => s.toNumber())
      ).to.deep.equal(QUEST_SEEDS);
    });

    it("rejects duplicate and non-signer season creation", async () => {
      await expectRejected(
        program.methods
          .createSeason(
            DEFAULT_SEASON_ID,
            bytes("Duplicate", 32) as any,
            DEFAULT_SEED_SCHEDULE.map((n) => bn(n)) as any,
            QUEST_SEEDS.map((n) => bn(n)) as any,
            bn(now())
          )
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            season: defaultSeasonPda,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /already in use|AccountAlreadyInitialized|custom program error: 0x0/i
      );

      const seasonPda = deriveSeason(999);
      await expectRejected(
        program.methods
          .createSeason(
            999,
            bytes("Unauthorized", 32) as any,
            DEFAULT_SEED_SCHEDULE.map((n) => bn(n)) as any,
            QUEST_SEEDS.map((n) => bn(n)) as any,
            bn(now())
          )
          .accountsStrict({
            admin: stranger.publicKey,
            vaultState: vaultStatePda,
            season: seasonPda,
            systemProgram: SystemProgram.programId,
          })
          .signers([stranger])
          .rpc(),
        /Unauthorized/i
      );
    });

    it("grants bounded idempotent quest seeds", async () => {
      const userPda = deriveUser(user1.publicKey);
      const before = await program.account.userAccount.fetch(userPda);
      const grantPda = deriveSeedGrant(user1.publicKey, 0, DEFAULT_PUBKEY);

      await program.methods
        .grantSeeds(bn(12), 0, DEFAULT_PUBKEY)
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userWallet: user1.publicKey,
          userAccount: userPda,
          seedGrant: grantPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const after = await program.account.userAccount.fetch(userPda);
      expect(after.seeds.toNumber() - before.seeds.toNumber()).to.equal(12);

      await expectRejected(
        program.methods
          .grantSeeds(bn(12), 0, DEFAULT_PUBKEY)
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userWallet: user1.publicKey,
            userAccount: userPda,
            seedGrant: grantPda,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /already in use|custom program error: 0x0|AccountAlreadyInitialized/i
      );

      const inviteGrantPda = deriveSeedGrant(
        user1.publicKey,
        2,
        user2.publicKey
      );
      await program.methods
        .grantSeeds(bn(45), 2, user2.publicKey)
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userWallet: user1.publicKey,
          userAccount: userPda,
          seedGrant: inviteGrantPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      await expectRejected(
        program.methods
          .grantSeeds(bn(1_001), 1, DEFAULT_PUBKEY)
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userWallet: user1.publicKey,
            userAccount: userPda,
            seedGrant: deriveSeedGrant(user1.publicKey, 1, DEFAULT_PUBKEY),
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /SeedGrantAmountInvalid/i
      );
    });
  });

  describe("governance and currency registry", () => {
    it("rotates signers with 2-of-3 while preserving authorizing cosigners", async () => {
      const replacement = Keypair.generate();
      await fund(replacement.publicKey);

      await program.methods
        .updateSigners([
          admin.publicKey,
          signer2.publicKey,
          replacement.publicKey,
        ])
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      let vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.signers[2].toBase58()).to.equal(
        replacement.publicKey.toBase58()
      );
      expect(vault.threshold).to.equal(2);

      await program.methods
        .updateSigners([admin.publicKey, signer2.publicKey, signer3.publicKey])
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.signers[2].toBase58()).to.equal(
        signer3.publicKey.toBase58()
      );
    });

    it("rejects duplicate, default, or continuity-breaking signer rotations", async () => {
      await expectRejected(
        program.methods
          .updateSigners([admin.publicKey, admin.publicKey, signer3.publicKey])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([signer2])
          .rpc(),
        /DuplicateSigner/i
      );

      await expectRejected(
        program.methods
          .updateSigners([admin.publicKey, signer2.publicKey, DEFAULT_PUBKEY])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([signer2])
          .rpc(),
        /SignerContinuityRequired/i
      );

      await expectRejected(
        program.methods
          .updateSigners([
            admin.publicKey,
            Keypair.generate().publicKey,
            Keypair.generate().publicKey,
          ])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([signer2])
          .rpc(),
        /SignerContinuityRequired/i
      );
    });

    it("registers and deactivates a currency without reclaiming its slot", async () => {
      await program.methods
        .registerCurrency(1)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          mint: bonusMint,
          opRevAta: bonusOpRevPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([signer2])
        .rpc();

      let vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.acceptedCurrencies[2].mint.toBase58()).to.equal(
        bonusMint.toBase58()
      );
      expect(vault.acceptedCurrencies[2].opRevAta.toBase58()).to.equal(
        bonusOpRevPda.toBase58()
      );
      expect(vault.acceptedCurrencies[2].active).to.equal(1);

      bonusContest = await createContest("bonus-before-deactivate", {
        fees: { 2: amount(4) },
        prizePool: amount(4),
        payouts: [amount(4)],
      });

      await expectRejected(
        program.methods
          .registerCurrency(1)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            mint: bonusMint,
            opRevAta: bonusOpRevPda,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([signer2])
          .rpc(),
        /already in use|CurrencyAlreadyRegistered|custom program error: 0x0/i
      );

      await program.methods
        .deactivateCurrency(2)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.acceptedCurrencies[2].mint.toBase58()).to.equal(
        bonusMint.toBase58()
      );
      expect(vault.acceptedCurrencies[2].active).to.equal(0);

      await expectRejected(
        createContest("inactive-currency-create", {
          fees: { 2: amount(1) },
          prizePool: amount(1),
          payouts: [amount(1)],
        }),
        /CurrencyNotActive/i
      );
    });
  });

  describe("contest lifecycle and entries", () => {
    it("creates a contest with per-currency fees, payout tiers, prize pool, and lock timestamp", async () => {
      const adminBefore = await tokenAmount(adminUsdcAta);
      paidContest = await createContest("matrix-paid-contest", {
        fees: { 0: amount(9), 1: amount(7) },
        maxEntries: 5,
        prizePool: amount(40),
        payouts: [amount(40)],
        lockTimestamp: 0,
      });

      const contest = await program.account.contest.fetch(
        paidContest.contestPda
      );
      expect(contest.entryFeeByCurrency[0].toNumber()).to.equal(amount(9));
      expect(contest.entryFeeByCurrency[1].toNumber()).to.equal(amount(7));
      expect(contest.prizePool.toNumber()).to.equal(amount(40));
      expect(contest.lockTimestamp.toNumber()).to.equal(0);
      expect(statusName(contest.status)).to.equal("open");
      expect(await tokenAmount(paidContest.prizePoolPda)).to.equal(amount(40));
      expect(adminBefore - (await tokenAmount(adminUsdcAta))).to.equal(
        amount(40)
      );
    });

    it("rejects invalid payout tiers and zero-fee zero-prize contests", async () => {
      await expectRejected(
        createContest("invalid-payout-sum", {
          fees: { 0: amount(9) },
          prizePool: amount(40),
          payouts: [amount(39)],
        }),
        /InvalidPayoutTiers/i
      );

      await expectRejected(
        createContest("no-fees-no-prize", {
          fees: {},
          prizePool: 0,
          payouts: [],
        }),
        /FeeAndPrizeBothZero/i
      );
    });

    it("accepts USDC and USDT entries by moving user ATA funds to op_rev", async () => {
      const user1Pda = deriveUser(user1.publicKey);
      const user2Pda = deriveUser(user2.publicKey);
      const user1Before = await program.account.userAccount.fetch(user1Pda);
      const user1UsdcBefore = await tokenAmount(user1UsdcAta);
      const opUsdcBefore = await tokenAmount(usdcOpRevPda);

      const user1Entry = await enterPaid(
        paidContest,
        user1,
        user1Pda,
        user1UsdcAta,
        usdcMint,
        usdcOpRevPda,
        0,
        0
      );

      const user1After = await program.account.userAccount.fetch(user1Pda);
      const entry1 = await program.account.contestEntry.fetch(user1Entry);
      expect(user1UsdcBefore - (await tokenAmount(user1UsdcAta))).to.equal(
        amount(9)
      );
      expect((await tokenAmount(usdcOpRevPda)) - opUsdcBefore).to.equal(
        amount(9)
      );
      expect(
        user1After.seeds.toNumber() - user1Before.seeds.toNumber()
      ).to.equal(25);
      expect(user1After.entries - user1Before.entries).to.equal(1);
      expect(entry1.currencyIdx).to.equal(0);
      expect(statusName(entry1.status)).to.equal("active");

      const user2Before = await program.account.userAccount.fetch(user2Pda);
      const user2UsdtBefore = await tokenAmount(user2UsdtAta);
      const opUsdtBefore = await tokenAmount(usdtOpRevPda);

      await enterPaid(
        paidContest,
        user2,
        user2Pda,
        user2UsdtAta,
        usdtMint,
        usdtOpRevPda,
        1,
        1
      );

      const user2After = await program.account.userAccount.fetch(user2Pda);
      const contest = await program.account.contest.fetch(
        paidContest.contestPda
      );
      expect(user2UsdtBefore - (await tokenAmount(user2UsdtAta))).to.equal(
        amount(7)
      );
      expect((await tokenAmount(usdtOpRevPda)) - opUsdtBefore).to.equal(
        amount(7)
      );
      expect(
        user2After.seeds.toNumber() - user2Before.seeds.toNumber()
      ).to.equal(19);
      expect(contest.currentEntries).to.equal(2);
      expect(contest.entryFees[0].toNumber()).to.equal(amount(9));
      expect(contest.entryFees[1].toNumber()).to.equal(amount(7));
    });

    it("rejects inactive currency, insufficient funds, full contest, and lock gate entries", async () => {
      await expectRejected(
        enterPaid(
          bonusContest,
          user1,
          deriveUser(user1.publicKey),
          user1BonusAta,
          bonusMint,
          bonusOpRevPda,
          2,
          0
        ),
        /CurrencyNotActive/i
      );

      const broke = Keypair.generate();
      await fund(broke.publicKey);
      const brokePda = await createUser(broke.publicKey, "broke-user");
      const brokeUsdc = await ata(usdcMint, broke.publicKey, 0);
      await expectRejected(
        enterPaid(
          paidContest,
          broke,
          brokePda,
          brokeUsdc,
          usdcMint,
          usdcOpRevPda,
          0,
          2
        ),
        /insufficient funds|0x1|custom program error/i
      );

      const maxContest = await createContest("max-entry-contest", {
        fees: { 0: amount(1) },
        maxEntries: 1,
        prizePool: amount(1),
        payouts: [amount(1)],
      });
      await enterPaid(
        maxContest,
        user1,
        deriveUser(user1.publicKey),
        user1UsdcAta,
        usdcMint,
        usdcOpRevPda,
        0,
        0
      );
      await expectRejected(
        enterPaid(
          maxContest,
          user2,
          deriveUser(user2.publicKey),
          user2UsdcAta,
          usdcMint,
          usdcOpRevPda,
          0,
          0
        ),
        /ContestFull/i
      );

      const lockedContest = await createContest("locked-entry-contest", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
        lockTimestamp: now() - 1,
      });
      await expectRejected(
        enterPaid(
          lockedContest,
          user1,
          deriveUser(user1.publicKey),
          user1UsdcAta,
          usdcMint,
          usdcOpRevPda,
          0,
          0
        ),
        /ContestLocked/i
      );
    });

    it("enforces set_contest_lock_time and set_contest_conclusion_time rules", async () => {
      const timingContest = await createContest("timing-contest", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
      });
      const lockAt = now() + 120;
      const conclusionAt = lockAt + 120;

      await program.methods
        .setContestLockTime(bn(lockAt))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: null,
          vaultState: vaultStatePda,
          contest: timingContest.contestPda,
        })
        .rpc();

      await expectRejected(
        program.methods
          .setContestConclusionTime(bn(lockAt - 1))
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: null,
            vaultState: vaultStatePda,
            contest: timingContest.contestPda,
          })
          .rpc(),
        /InvalidTimestamp/i
      );

      await program.methods
        .setContestConclusionTime(bn(conclusionAt))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: null,
          vaultState: vaultStatePda,
          contest: timingContest.contestPda,
        })
        .rpc();

      await expectRejected(
        program.methods
          .setContestConclusionTime(bn(conclusionAt + 60))
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: null,
            vaultState: vaultStatePda,
            contest: timingContest.contestPda,
          })
          .rpc(),
        /Unauthorized/i
      );

      await program.methods
        .setContestConclusionTime(bn(conclusionAt + 60))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: timingContest.contestPda,
        })
        .signers([signer2])
        .rpc();

      const postLockContest = await createContest("post-lock-amend-contest", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
        lockTimestamp: now() - 1,
      });
      await expectRejected(
        program.methods
          .setContestLockTime(bn(now() + 300))
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: null,
            vaultState: vaultStatePda,
            contest: postLockContest.contestPda,
          })
          .rpc(),
        /Unauthorized/i
      );
      await program.methods
        .setContestLockTime(bn(now() + 300))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: postLockContest.contestPda,
        })
        .signers([signer2])
        .rpc();
    });
  });

  describe("entry tokens and pause controls", () => {
    it("mints idempotent token-entry vouchers and consumes one without charging currency", async () => {
      const tokenContest = await createContest("token-entry-contest", {
        fees: { 0: amount(9) },
        prizePool: amount(9),
        payouts: [amount(9)],
      });
      const opBefore = await tokenAmount(usdcOpRevPda);
      const userBefore = await program.account.userAccount.fetch(
        deriveUser(user1.publicKey)
      );
      const token = await mintEntryToken(
        user1.publicKey,
        "stripe-session-token-entry"
      );

      await expectRejected(
        program.methods
          .mintEntryToken(0, token.ref as any, token.hash as any)
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userWallet: user1.publicKey,
            entryToken: token.pda,
            systemProgram: SystemProgram.programId,
          })
          .rpc(),
        /already in use|custom program error: 0x0|AccountAlreadyInitialized/i
      );

      const entryPda = await enterWithToken(
        tokenContest,
        user1,
        deriveUser(user1.publicKey),
        token.pda,
        0
      );
      const entry = await program.account.contestEntry.fetch(entryPda);
      const consumed = await program.account.entryTokenAccount.fetch(token.pda);
      const userAfter = await program.account.userAccount.fetch(
        deriveUser(user1.publicKey)
      );
      expect(entry.currencyIdx).to.equal(255);
      expect(consumed.consumed).to.equal(true);
      expect((await tokenAmount(usdcOpRevPda)) - opBefore).to.equal(0);
      expect(userAfter.seeds.toNumber() - userBefore.seeds.toNumber()).to.equal(
        25
      );

      await expectRejected(
        enterWithToken(
          tokenContest,
          user1,
          deriveUser(user1.publicKey),
          token.pda,
          1
        ),
        /EntryTokenAlreadyConsumed/i
      );

      const wrongOwnerToken = await mintEntryToken(
        user1.publicKey,
        "wrong-owner-token"
      );
      await expectRejected(
        enterWithToken(
          tokenContest,
          user2,
          deriveUser(user2.publicKey),
          wrongOwnerToken.pda,
          0
        ),
        /EntryTokenWrongOwner/i
      );
    });

    // ── burn_entry_token: the operator claw-back ─────────────────────────
    //
    // The property under test is that a burn STICKS. Rails derives what a user
    // is owed from the on-chain token COUNT, so a burn that removed the account
    // would re-read as owed and be re-minted by the next sweep. Everything here
    // is ultimately checking that the account survives while its spending power
    // does not.
    it("burns an unspent voucher as a tombstone the account survives", async () => {
      const token = await mintEntryToken(user1.publicKey, "burn-me-plain");
      const before = await program.account.entryTokenAccount.fetch(token.pda);
      expect(before.consumed).to.equal(false);
      expect(before.source).to.equal(0);

      await burnEntryToken(token);

      // NOT closed. This is the whole design: `getAccountInfo` must still find
      // it, or Rails' owed math re-opens the debt this burn just settled.
      const info = await provider.connection.getAccountInfo(token.pda);
      expect(info, "a burned token must NOT be closed — the count is load-bearing")
        .to.not.equal(null);

      const after = await program.account.entryTokenAccount.fetch(token.pda);
      expect(after.consumed, "consumed is what blocks the spend").to.equal(true);
      expect(after.consumedAt).to.not.equal(null);
      // source = OPERATOR(0) | BURNED_FLAG(0x80). The provenance half survives
      // in the low 7 bits so the audit trail still says where the token came
      // from.
      expect(after.source, "the burn flag rides in source's high bit").to.equal(128);
      expect(after.source & 0x7f, "provenance must survive the burn").to.equal(0);
      expect(after.owner.toBase58()).to.equal(user1.publicKey.toBase58());
    });

    it("a burned voucher can no longer fund an entry", async () => {
      const burnContest = await createContest("burned-token-contest", {
        fees: { 0: amount(3) },
        prizePool: amount(3),
        payouts: [amount(3)],
      });
      const token = await mintEntryToken(user1.publicKey, "burn-then-try-entry");
      await burnEntryToken(token);

      // The point of the feature. `consumed` is the guard
      // enter_contest_with_token already carried, which is exactly why the burn
      // sets it rather than introducing a new field the old accounts lack.
      await expectRejected(
        enterWithToken(
          burnContest,
          user1,
          deriveUser(user1.publicKey),
          token.pda,
          0
        ),
        /EntryTokenAlreadyConsumed/i
      );
    });

    it("refuses a double burn and refuses to burn a SPENT voucher", async () => {
      const doubleContest = await createContest("double-burn-contest", {
        fees: { 0: amount(2) },
        prizePool: amount(2),
        payouts: [amount(2)],
      });

      // Double burn. Rejected by its OWN guard, not by the consumed guard —
      // a burn sets consumed itself, so without the flag check a re-burn would
      // be accepted and would overwrite consumedAt, destroying the record of
      // when the burn actually happened.
      const twice = await mintEntryToken(user1.publicKey, "burn-me-twice");
      await burnEntryToken(twice);
      const stampedAt = (
        await program.account.entryTokenAccount.fetch(twice.pda)
      ).consumedAt;
      await expectRejected(burnEntryToken(twice), /EntryTokenAlreadyBurned/i);
      expect(
        (await program.account.entryTokenAccount.fetch(twice.pda)).consumedAt!.toString(),
        "a refused re-burn must not move the original burn timestamp"
      ).to.equal(stampedAt!.toString());

      // Spent, then burned. Burning a token that already funded a real entry
      // would rewrite the history of an entry that exists and stands.
      const spent = await mintEntryToken(user1.publicKey, "spend-then-burn");
      await enterWithToken(
        doubleContest,
        user1,
        deriveUser(user1.publicKey),
        spent.pda,
        0
      );
      await expectRejected(burnEntryToken(spent), /EntryTokenAlreadyConsumed/i);
    });

    it("only a vault signer may burn, and the hash must name the token", async () => {
      const token = await mintEntryToken(user1.publicKey, "burn-auth-checks");

      // A stranger holding no vault seat cannot destroy a user's property.
      await expectRejected(burnEntryToken(token, stranger), /Unauthorized/i);

      // The fat-finger guard. The account and the ref hash must agree, so a
      // burn aimed at the wrong account fails the seeds check instead of
      // quietly destroying some other user's token.
      const other = await mintEntryToken(user1.publicKey, "burn-wrong-hash-target");
      await expectRejected(
        program.methods
          .burnEntryToken(other.hash as any)
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            entryToken: token.pda,
          })
          .rpc(),
        /ConstraintSeeds|EntryTokenSeedMismatch|seeds constraint/i
      );

      // ...and the token the mismatched call named is untouched.
      expect(
        (await program.account.entryTokenAccount.fetch(token.pda)).consumed
      ).to.equal(false);
      expect(
        (await program.account.entryTokenAccount.fetch(other.pda)).consumed
      ).to.equal(false);
    });

    it("pause blocks paid and token entries only; unpause restores both", async () => {
      const pauseContest = await createContest("pause-contest", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
        maxEntries: 4,
      });

      await program.methods
        .pause(reason("local verification pause") as any)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      let vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.paused).to.equal(1);

      await expectRejected(
        enterPaid(
          pauseContest,
          user1,
          deriveUser(user1.publicKey),
          user1UsdcAta,
          usdcMint,
          usdcOpRevPda,
          0,
          0
        ),
        /VaultPaused/i
      );

      const pausedToken = await mintEntryToken(
        user1.publicKey,
        "paused-token-mint-still-allowed"
      );
      await expectRejected(
        enterWithToken(
          pauseContest,
          user1,
          deriveUser(user1.publicKey),
          pausedToken.pda,
          1
        ),
        /VaultPaused/i
      );

      await expectRejected(
        program.methods
          .pause(reason("bad cosigner") as any)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: stranger.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([stranger])
          .rpc(),
        /Unauthorized/i
      );

      await program.methods
        .unpause()
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.paused).to.equal(0);

      await enterPaid(
        pauseContest,
        user1,
        deriveUser(user1.publicKey),
        user1UsdcAta,
        usdcMint,
        usdcOpRevPda,
        0,
        0
      );
      await enterWithToken(
        pauseContest,
        user1,
        deriveUser(user1.publicKey),
        pausedToken.pda,
        1
      );
    });
  });

  describe("settlement, cancellation, closeout, and treasury", () => {
    it("settles a locked contest from prize_pool to winner ATA with 2-of-3", async () => {
      await lockContestNow(paidContest);

      const user1Pda = deriveUser(user1.publicKey);
      const user2Pda = deriveUser(user2.publicKey);
      const user1Entry = deriveEntry(paidContest.id, user1.publicKey, 0);
      const user2Entry = deriveEntry(paidContest.id, user2.publicKey, 1);
      const user1Before = await program.account.userAccount.fetch(user1Pda);
      const prizePoolBefore = await tokenAmount(paidContest.prizePoolPda);
      const user1AtaBefore = await tokenAmount(user1UsdcAta);

      await program.methods
        .settleContest([
          {
            wallet: user1.publicKey,
            entryNum: 0,
            rank: 1,
            payout: bn(amount(40)),
          },
          { wallet: user2.publicKey, entryNum: 1, rank: 2, payout: bn(0) },
        ])
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: paidContest.contestPda,
          prizePool: paidContest.prizePoolPda,
          payoutMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          { pubkey: user1Pda, isSigner: false, isWritable: true },
          { pubkey: user1Entry, isSigner: false, isWritable: true },
          { pubkey: user1UsdcAta, isSigner: false, isWritable: true },
          { pubkey: user2Pda, isSigner: false, isWritable: true },
          { pubkey: user2Entry, isSigner: false, isWritable: true },
          { pubkey: user2UsdcAta, isSigner: false, isWritable: true },
        ])
        .signers([signer2])
        .rpc();

      const contest = await program.account.contest.fetch(
        paidContest.contestPda
      );
      const user1After = await program.account.userAccount.fetch(user1Pda);
      const entry = await program.account.contestEntry.fetch(user1Entry);
      expect(statusName(contest.status)).to.equal("settled");
      expect(
        prizePoolBefore - (await tokenAmount(paidContest.prizePoolPda))
      ).to.equal(amount(40));
      expect((await tokenAmount(user1UsdcAta)) - user1AtaBefore).to.equal(
        amount(40)
      );
      expect(
        user1After.totalWon.toNumber() - user1Before.totalWon.toNumber()
      ).to.equal(amount(40));
      expect(user1After.wins - user1Before.wins).to.equal(1);
      expect(user1After.cashes - user1Before.cashes).to.equal(1);
      expect(statusName(entry.status)).to.equal("won");
      expect(entry.payout.toNumber()).to.equal(amount(40));
    });

    it("rejects unsafe settlement variants", async () => {
      const unlockedContest = await createContest("settle-before-lock", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
      });
      await expectRejected(
        program.methods
          .settleContest([])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            contest: unlockedContest.contestPda,
            prizePool: unlockedContest.prizePoolPda,
            payoutMint: usdcMint,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([signer2])
          .rpc(),
        /ContestNotLocked/i
      );

      const duplicateContest = await createContest("settle-duplicate-entry", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
      });
      await lockContestNow(duplicateContest);
      await expectRejected(
        program.methods
          .settleContest([
            { wallet: user1.publicKey, entryNum: 0, rank: 1, payout: bn(0) },
            { wallet: user1.publicKey, entryNum: 0, rank: 2, payout: bn(0) },
          ])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            contest: duplicateContest.contestPda,
            prizePool: duplicateContest.prizePoolPda,
            payoutMint: usdcMint,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([signer2])
          .rpc(),
        /DuplicateEntry/i
      );

      const badDestinationContest = await createContest(
        "settle-bad-destination",
        {
          fees: { 0: amount(1) },
          prizePool: amount(1),
          payouts: [amount(1)],
        }
      );
      await enterPaid(
        badDestinationContest,
        user1,
        deriveUser(user1.publicKey),
        user1UsdcAta,
        usdcMint,
        usdcOpRevPda,
        0,
        0
      );
      await lockContestNow(badDestinationContest);
      await expectRejected(
        program.methods
          .settleContest([
            {
              wallet: user1.publicKey,
              entryNum: 0,
              rank: 1,
              payout: bn(amount(1)),
            },
          ])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            contest: badDestinationContest.contestPda,
            prizePool: badDestinationContest.prizePoolPda,
            payoutMint: usdcMint,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .remainingAccounts([
            {
              pubkey: deriveUser(user1.publicKey),
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: deriveEntry(badDestinationContest.id, user1.publicKey, 0),
              isSigner: false,
              isWritable: true,
            },
            { pubkey: user2UsdcAta, isSigner: false, isWritable: true },
          ])
          .signers([signer2])
          .rpc(),
        /InvalidPayoutDestination/i
      );
    });

    it("cancels open contests by refunding prize pool while preserving op_rev", async () => {
      const cancelContest = await createContest("cancel-contest", {
        fees: { 0: amount(2) },
        prizePool: amount(8),
        payouts: [amount(8)],
      });
      const adminBefore = await tokenAmount(adminUsdcAta);
      const opBefore = await tokenAmount(usdcOpRevPda);
      await enterPaid(
        cancelContest,
        user1,
        deriveUser(user1.publicKey),
        user1UsdcAta,
        usdcMint,
        usdcOpRevPda,
        0,
        0
      );

      await program.methods
        .cancelContest()
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: cancelContest.contestPda,
          prizePool: cancelContest.prizePoolPda,
          payoutMint: usdcMint,
          creatorTokenAccount: adminUsdcAta,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([signer2])
        .rpc();

      const contest = await program.account.contest.fetch(
        cancelContest.contestPda
      );
      expect(statusName(contest.status)).to.equal("cancelled");
      expect((await tokenAmount(adminUsdcAta)) - adminBefore).to.equal(
        amount(8)
      );
      expect((await tokenAmount(usdcOpRevPda)) - opBefore).to.equal(amount(2));
    });

    it("closes finalized contests and dust-sweeps prize pool to USDC op_rev", async () => {
      const dustContest = await createContest("dust-close-contest", {
        fees: {},
        prizePool: 1,
        payouts: [1],
      });
      await lockContestNow(dustContest);
      await program.methods
        .settleContest([])
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: dustContest.contestPda,
          prizePool: dustContest.prizePoolPda,
          payoutMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([signer2])
        .rpc();

      const opBefore = await tokenAmount(usdcOpRevPda);
      await program.methods
        .closeContest()
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          contest: dustContest.contestPda,
          prizePool: dustContest.prizePoolPda,
          payoutMint: usdcMint,
          opRevUsdcAta: usdcOpRevPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      expect(await connection.getAccountInfo(dustContest.contestPda)).to.equal(
        null
      );
      expect(
        await connection.getAccountInfo(dustContest.prizePoolPda)
      ).to.equal(null);
      expect((await tokenAmount(usdcOpRevPda)) - opBefore).to.equal(1);

      const openContest = await createContest("close-open-reject", {
        fees: { 0: amount(1) },
        prizePool: amount(1),
        payouts: [amount(1)],
      });
      await expectRejected(
        program.methods
          .closeContest()
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            contest: openContest.contestPda,
            prizePool: openContest.prizePoolPda,
            payoutMint: usdcMint,
            opRevUsdcAta: usdcOpRevPda,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .rpc(),
        /ContestNotSettled/i
      );
    });

    it("sweeps operator revenue only to pinned treasury ATA", async () => {
      await expectRejected(
        program.methods
          .sweepOperatorRevenue(bn(0))
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            currencyMint: usdtMint,
            opRevAta: usdtOpRevPda,
            treasuryAta: wrongTreasuryUsdtAta,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([signer2])
          .rpc(),
        /TreasuryAuthorityMismatch/i
      );

      const treasuryBefore = await tokenAmount(treasuryUsdcAta);
      const opBefore = await tokenAmount(usdcOpRevPda);
      expect(opBefore).to.be.greaterThan(0);

      await program.methods
        .sweepOperatorRevenue(bn(0))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          currencyMint: usdcMint,
          opRevAta: usdcOpRevPda,
          treasuryAta: treasuryUsdcAta,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([signer2])
        .rpc();

      expect(await tokenAmount(usdcOpRevPda)).to.equal(0);
      expect((await tokenAmount(treasuryUsdcAta)) - treasuryBefore).to.equal(
        opBefore
      );

      await expectRejected(
        program.methods
          .sweepOperatorRevenue(bn(0))
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            currencyMint: usdcMint,
            opRevAta: usdcOpRevPda,
            treasuryAta: treasuryUsdcAta,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([signer2])
          .rpc(),
        /EmptyRevenueAccount/i
      );

      const usdtTreasuryBefore = await tokenAmount(treasuryUsdtAta);
      const usdtOpBefore = await tokenAmount(usdtOpRevPda);
      await program.methods
        .sweepOperatorRevenue(bn(amount(3)))
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          currencyMint: usdtMint,
          opRevAta: usdtOpRevPda,
          treasuryAta: treasuryUsdtAta,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([signer2])
        .rpc();
      expect(usdtOpBefore - (await tokenAmount(usdtOpRevPda))).to.equal(
        amount(3)
      );
      expect(
        (await tokenAmount(treasuryUsdtAta)) - usdtTreasuryBefore
      ).to.equal(amount(3));
    });

    it("rejects a full currency registry", async () => {
      for (let slot = 3; slot < MAX_CURRENCIES; slot++) {
        const mint = await createMint(
          connection,
          admin.payer,
          admin.publicKey,
          null,
          DECIMALS
        );
        await program.methods
          .registerCurrency(1)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            mint,
            opRevAta: deriveOpRev(mint),
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([signer2])
          .rpc();
      }

      const overflowMint = await createMint(
        connection,
        admin.payer,
        admin.publicKey,
        null,
        DECIMALS
      );
      await expectRejected(
        program.methods
          .registerCurrency(1)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            mint: overflowMint,
            opRevAta: deriveOpRev(overflowMint),
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: anchor.web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([signer2])
          .rpc(),
        /CurrencyRegistryFull/i
      );
    });
  });
});

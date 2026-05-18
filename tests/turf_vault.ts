import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { TurfVault } from "../target/types/turf_vault";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { expect } from "chai";
import { createHash } from "crypto";

describe("turf_vault", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.TurfVault as Program<TurfVault>;
  const admin = provider.wallet as anchor.Wallet;
  const connection = provider.connection;

  // Test keypairs
  let user1: Keypair;
  let user2: Keypair;
  let signer2: Keypair;  // Second multisig signer (was adminBackup)
  let signer3: Keypair;  // Third multisig signer (Mason)

  // Token mints
  let usdcMint: PublicKey;
  let usdtMint: PublicKey;

  // PDAs
  let vaultStatePda: PublicKey;
  let vaultUsdcPda: PublicKey;
  let vaultUsdtPda: PublicKey;

  // User token accounts
  let user1UsdcAccount: PublicKey;
  let user1UsdtAccount: PublicKey;
  let user2UsdcAccount: PublicKey;
  let adminUsdcAccount: PublicKey;
  let signer2UsdcAccount: PublicKey;

  // Contest
  const contestSlug = "turf-totals-v1-matchday-1";
  const contestId = createHash("sha256").update(contestSlug).digest();
  const DECIMALS = 6;
  const toTokenAmount = (dollars: number) => dollars * 10 ** DECIMALS;

  before(async () => {
    // Create test users and fund them
    user1 = Keypair.generate();
    user2 = Keypair.generate();
    signer2 = Keypair.generate();
    signer3 = Keypair.generate();

    // Fund test users with SOL (transfer from admin instead of airdrop — v3.1 airdrop is broken)
    for (const user of [user1, user2, signer2, signer3]) {
      const tx = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: user.publicKey,
          lamports: 10 * LAMPORTS_PER_SOL,
        })
      );
      await provider.sendAndConfirm(tx);
    }

    // Create USDC and USDT mints (admin is mint authority)
    usdcMint = await createMint(connection, admin.payer, admin.publicKey, null, DECIMALS);
    usdtMint = await createMint(connection, admin.payer, admin.publicKey, null, DECIMALS);

    // Derive PDAs
    [vaultStatePda] = PublicKey.findProgramAddressSync([Buffer.from("vault")], program.programId);
    [vaultUsdcPda] = PublicKey.findProgramAddressSync([Buffer.from("vault_usdc")], program.programId);
    [vaultUsdtPda] = PublicKey.findProgramAddressSync([Buffer.from("vault_usdt")], program.programId);

    // Create user token accounts
    user1UsdcAccount = await createAccount(connection, admin.payer, usdcMint, user1.publicKey);
    user1UsdtAccount = await createAccount(connection, admin.payer, usdtMint, user1.publicKey);
    user2UsdcAccount = await createAccount(connection, admin.payer, usdcMint, user2.publicKey);
    adminUsdcAccount = await createAccount(connection, admin.payer, usdcMint, admin.publicKey);
    signer2UsdcAccount = await createAccount(connection, admin.payer, usdcMint, signer2.publicKey);

    // Mint test tokens to users and admins
    await mintTo(connection, admin.payer, usdcMint, user1UsdcAccount, admin.publicKey, toTokenAmount(100));
    await mintTo(connection, admin.payer, usdtMint, user1UsdtAccount, admin.publicKey, toTokenAmount(50));
    await mintTo(connection, admin.payer, usdcMint, user2UsdcAccount, admin.publicKey, toTokenAmount(100));
    await mintTo(connection, admin.payer, usdcMint, adminUsdcAccount, admin.publicKey, toTokenAmount(500));
    await mintTo(connection, admin.payer, usdcMint, signer2UsdcAccount, admin.publicKey, toTokenAmount(100));
  });

  describe("initialize", () => {
    it("initializes the vault with 3 signers and threshold 2", async () => {
      await program.methods
        .initialize([admin.publicKey, signer2.publicKey, signer3.publicKey], 2)
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          usdcMint,
          usdtMint,
          vaultUsdc: vaultUsdcPda,
          vaultUsdt: vaultUsdtPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .rpc();

      const vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.signers[0].toBase58()).to.equal(admin.publicKey.toBase58());
      expect(vault.signers[1].toBase58()).to.equal(signer2.publicKey.toBase58());
      expect(vault.signers[2].toBase58()).to.equal(signer3.publicKey.toBase58());
      expect(vault.threshold).to.equal(2);
      expect(vault.usdcMint.toBase58()).to.equal(usdcMint.toBase58());
      expect(vault.usdtMint.toBase58()).to.equal(usdtMint.toBase58());
    });
  });

  describe("create_user_account", () => {
    it("creates user account for user1", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      await program.methods
        .createUserAccount(user1.publicKey)
        .accountsStrict({
          payer: admin.publicKey,
          userAccount: userAccountPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const account = await program.account.userAccount.fetch(userAccountPda);
      expect(account.wallet.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(account.balance.toNumber()).to.equal(0);
      expect(account.seeds.toNumber()).to.equal(0);
    });

    it("creates user account for user2", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user2.publicKey.toBuffer()],
        program.programId
      );

      await program.methods
        .createUserAccount(user2.publicKey)
        .accountsStrict({
          payer: admin.publicKey,
          userAccount: userAccountPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const account = await program.account.userAccount.fetch(userAccountPda);
      expect(account.wallet.toBase58()).to.equal(user2.publicKey.toBase58());
    });
  });

  describe("deposit", () => {
    it("deposits USDC for user1", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      const amount = toTokenAmount(10); // $10

      await program.methods
        .deposit(new anchor.BN(amount))
        .accountsStrict({
          user: user1.publicKey,
          userAccount: userAccountPda,
          vaultState: vaultStatePda,
          mint: usdcMint,
          userTokenAccount: user1UsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([user1])
        .rpc();

      const account = await program.account.userAccount.fetch(userAccountPda);
      expect(account.balance.toNumber()).to.equal(amount);
      expect(account.totalDeposited.toNumber()).to.equal(amount);

      // Verify vault received tokens
      const vaultBalance = await getAccount(connection, vaultUsdcPda);
      expect(Number(vaultBalance.amount)).to.equal(amount);
    });

    it("deposits USDT for user1", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      const amount = toTokenAmount(5); // $5

      await program.methods
        .deposit(new anchor.BN(amount))
        .accountsStrict({
          user: user1.publicKey,
          userAccount: userAccountPda,
          vaultState: vaultStatePda,
          mint: usdtMint,
          userTokenAccount: user1UsdtAccount,
          vaultTokenAccount: vaultUsdtPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([user1])
        .rpc();

      const account = await program.account.userAccount.fetch(userAccountPda);
      expect(account.balance.toNumber()).to.equal(toTokenAmount(15)); // 10 + 5
    });

    it("deposits USDC for user2", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user2.publicKey.toBuffer()],
        program.programId
      );

      await program.methods
        .deposit(new anchor.BN(toTokenAmount(10)))
        .accountsStrict({
          user: user2.publicKey,
          userAccount: userAccountPda,
          vaultState: vaultStatePda,
          mint: usdcMint,
          userTokenAccount: user2UsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([user2])
        .rpc();

      const account = await program.account.userAccount.fetch(userAccountPda);
      expect(account.balance.toNumber()).to.equal(toTokenAmount(10));
    });

    it("rejects deposit with invalid mint", async () => {
      const fakeMint = await createMint(connection, admin.payer, admin.publicKey, null, DECIMALS);
      const fakeTokenAccount = await createAccount(connection, admin.payer, fakeMint, user1.publicKey);
      await mintTo(connection, admin.payer, fakeMint, fakeTokenAccount, admin.publicKey, toTokenAmount(10));

      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      try {
        await program.methods
          .deposit(new anchor.BN(toTokenAmount(1)))
          .accountsStrict({
            user: user1.publicKey,
            userAccount: userAccountPda,
            vaultState: vaultStatePda,
            mint: fakeMint,
            userTokenAccount: fakeTokenAccount,
            vaultTokenAccount: vaultUsdcPda,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([user1])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("InvalidMint");
      }
    });
  });

  describe("create_contest", () => {
    it("admin creates a contest with prizes transfer", async () => {
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );

      const entryFee = toTokenAmount(9); // $9
      const maxEntries = 5;
      const payoutAmounts = [new anchor.BN(toTokenAmount(40))]; // Small format: 1st gets $40
      const prizes = toTokenAmount(40); // $40 guarantee

      const vaultBefore = await getAccount(connection, vaultUsdcPda);
      const adminBefore = await getAccount(connection, adminUsdcAccount);

      await program.methods
        .createContest(
          Array.from(contestId) as any,
          new anchor.BN(entryFee),
          maxEntries,
          payoutAmounts,
          new anchor.BN(prizes)
        )
        .accountsStrict({
          payer: admin.publicKey,
          creator: admin.publicKey,
          vaultState: vaultStatePda,
          contest: contestPda,
          mint: usdcMint,
          creatorTokenAccount: adminUsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const contest = await program.account.contest.fetch(contestPda);
      expect(contest.entryFee.toNumber()).to.equal(entryFee);
      expect(contest.maxEntries).to.equal(maxEntries);
      expect(contest.currentEntries).to.equal(0);
      expect(contest.entryFees.toNumber()).to.equal(0);
      expect(contest.prizes.toNumber()).to.equal(prizes);
      expect(contest.creator.toBase58()).to.equal(admin.publicKey.toBase58());
      expect(JSON.stringify(contest.status)).to.equal(JSON.stringify({ open: {} }));

      // Verify prizes USDC was transferred
      const vaultAfter = await getAccount(connection, vaultUsdcPda);
      const adminAfter = await getAccount(connection, adminUsdcAccount);
      expect(Number(vaultAfter.amount) - Number(vaultBefore.amount)).to.equal(prizes);
      expect(Number(adminBefore.amount) - Number(adminAfter.amount)).to.equal(prizes);
    });

    it("rejects non-signer creating a contest", async () => {
      const fakeContestId = createHash("sha256").update("fake-contest").digest();
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), fakeContestId],
        program.programId
      );

      try {
        await program.methods
          .createContest(
            Array.from(fakeContestId) as any,
            new anchor.BN(toTokenAmount(9)),
            5,
            [new anchor.BN(toTokenAmount(40))],
            new anchor.BN(toTokenAmount(40))
          )
          .accountsStrict({
            payer: user1.publicKey,
            creator: user1.publicKey,
            vaultState: vaultStatePda,
            contest: contestPda,
            mint: usdcMint,
            creatorTokenAccount: user1UsdcAccount,
            vaultTokenAccount: vaultUsdcPda,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([user1])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("Unauthorized");
      }
    });
  });

  describe("enter_contest", () => {
    it("user1 enters the contest", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );
      const entryNum = 1;
      const entryNumBytes = Buffer.alloc(4);
      entryNumBytes.writeUInt32LE(entryNum);
      const [entryPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("entry"),
          contestId,
          user1.publicKey.toBuffer(),
          entryNumBytes,
        ],
        program.programId
      );

      const userBefore = await program.account.userAccount.fetch(userAccountPda);

      await program.methods
        .enterContest(entryNum)
        .accountsStrict({
          payer: admin.publicKey,
          wallet: user1.publicKey,
          vaultState: vaultStatePda,
          userAccount: userAccountPda,
          contest: contestPda,
          contestEntry: entryPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const userAfter = await program.account.userAccount.fetch(userAccountPda);
      const contest = await program.account.contest.fetch(contestPda);
      const entry = await program.account.contestEntry.fetch(entryPda);

      // User balance decreased by entry fee
      expect(userAfter.balance.toNumber()).to.equal(
        userBefore.balance.toNumber() - toTokenAmount(9)
      );
      // 65 seeds awarded
      expect(userAfter.seeds.toNumber()).to.equal(65);
      // Contest pool increased
      expect(contest.currentEntries).to.equal(1);
      expect(contest.entryFees.toNumber()).to.equal(toTokenAmount(9));
      // Entry created
      expect(entry.wallet.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(entry.entryNum).to.equal(entryNum);
      expect(JSON.stringify(entry.status)).to.equal(JSON.stringify({ active: {} }));
    });

    it("user2 enters the contest", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user2.publicKey.toBuffer()],
        program.programId
      );
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );
      const entryNum = 1;
      const entryNumBytes = Buffer.alloc(4);
      entryNumBytes.writeUInt32LE(entryNum);
      const [entryPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("entry"),
          contestId,
          user2.publicKey.toBuffer(),
          entryNumBytes,
        ],
        program.programId
      );

      await program.methods
        .enterContest(entryNum)
        .accountsStrict({
          payer: admin.publicKey,
          wallet: user2.publicKey,
          vaultState: vaultStatePda,
          userAccount: userAccountPda,
          contest: contestPda,
          contestEntry: entryPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const contest = await program.account.contest.fetch(contestPda);
      expect(contest.currentEntries).to.equal(2);
      expect(contest.entryFees.toNumber()).to.equal(toTokenAmount(18));

      // 65 seeds awarded to user2
      const user2After = await program.account.userAccount.fetch(userAccountPda);
      expect(user2After.seeds.toNumber()).to.equal(65);
    });

    it("rejects entry with insufficient balance", async () => {
      // Create a broke user (transfer SOL instead of airdrop — v3.1 airdrop is broken)
      const brokeUser = Keypair.generate();
      const fundTx = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: brokeUser.publicKey,
          lamports: LAMPORTS_PER_SOL,
        })
      );
      await provider.sendAndConfirm(fundTx);

      const [brokeUserPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), brokeUser.publicKey.toBuffer()],
        program.programId
      );
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );

      // Create user account with 0 balance
      await program.methods
        .createUserAccount(brokeUser.publicKey)
        .accountsStrict({
          payer: admin.publicKey,
          userAccount: brokeUserPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const entryNumBytes = Buffer.alloc(4);
      entryNumBytes.writeUInt32LE(1);
      const [entryPda] = PublicKey.findProgramAddressSync(
        [
          Buffer.from("entry"),
          contestId,
          brokeUser.publicKey.toBuffer(),
          entryNumBytes,
        ],
        program.programId
      );

      try {
        await program.methods
          .enterContest(1)
          .accountsStrict({
            payer: admin.publicKey,
            wallet: brokeUser.publicKey,
            vaultState: vaultStatePda,
            userAccount: brokeUserPda,
            contest: contestPda,
            contestEntry: entryPda,
            systemProgram: SystemProgram.programId,
          })
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("InsufficientBalance");
      }
    });
  });

  describe("settle_contest", () => {
    it("settles with valid cosigner (2-of-3 multisig)", async () => {
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );
      const [user1AccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );
      const [user2AccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user2.publicKey.toBuffer()],
        program.programId
      );
      const entryNumBytes = Buffer.alloc(4);
      entryNumBytes.writeUInt32LE(1);
      const [user1EntryPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("entry"), contestId, user1.publicKey.toBuffer(), entryNumBytes],
        program.programId
      );
      const [user2EntryPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("entry"), contestId, user2.publicKey.toBuffer(), entryNumBytes],
        program.programId
      );

      const user1Before = await program.account.userAccount.fetch(user1AccountPda);
      const user2Before = await program.account.userAccount.fetch(user2AccountPda);

      // Contest entry_fees = $18 (2×$9), prizes = $40, total = $58
      // user1 rank 1 gets $40 (Small format payout), user2 rank 2 gets $0
      const settlements = [
        { wallet: user1.publicKey, entryNum: 1, rank: 1, payout: new anchor.BN(toTokenAmount(40)) },
        { wallet: user2.publicKey, entryNum: 1, rank: 2, payout: new anchor.BN(toTokenAmount(0)) },
      ];

      await program.methods
        .settleContest(settlements)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: contestPda,
        })
        .signers([signer2])
        .remainingAccounts([
          { pubkey: user1AccountPda, isSigner: false, isWritable: true },
          { pubkey: user1EntryPda, isSigner: false, isWritable: true },
          { pubkey: user2AccountPda, isSigner: false, isWritable: true },
          { pubkey: user2EntryPda, isSigner: false, isWritable: true },
        ])
        .rpc();

      const user1After = await program.account.userAccount.fetch(user1AccountPda);
      const user2After = await program.account.userAccount.fetch(user2AccountPda);
      const contest = await program.account.contest.fetch(contestPda);

      expect(user1After.balance.toNumber()).to.equal(
        user1Before.balance.toNumber() + toTokenAmount(40)
      );
      expect(user1After.totalWon.toNumber()).to.equal(toTokenAmount(40));
      expect(user2After.balance.toNumber()).to.equal(
        user2Before.balance.toNumber()
      );
      expect(JSON.stringify(contest.status)).to.equal(JSON.stringify({ settled: {} }));

      // Verify entry statuses
      const user1Entry = await program.account.contestEntry.fetch(user1EntryPda);
      const user2Entry = await program.account.contestEntry.fetch(user2EntryPda);
      expect(JSON.stringify(user1Entry.status)).to.equal(JSON.stringify({ won: {} }));
      expect(user1Entry.rank).to.equal(1);
      expect(user1Entry.payout.toNumber()).to.equal(toTokenAmount(40));
      expect(JSON.stringify(user2Entry.status)).to.equal(JSON.stringify({ lost: {} }));
      expect(user2Entry.rank).to.equal(2);
    });

    it("rejects settlement with same signer twice", async () => {
      // Create a new contest to test with
      const dupeContestId = createHash("sha256").update("dupe-signer-test").digest();
      const [dupeContestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), dupeContestId],
        program.programId
      );

      await program.methods
        .createContest(
          Array.from(dupeContestId) as any,
          new anchor.BN(toTokenAmount(9)),
          5,
          [],
          new anchor.BN(0)
        )
        .accountsStrict({
          payer: admin.publicKey,
          creator: admin.publicKey,
          vaultState: vaultStatePda,
          contest: dupeContestPda,
          mint: usdcMint,
          creatorTokenAccount: adminUsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      try {
        await program.methods
          .settleContest([])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: admin.publicKey,
            vaultState: vaultStatePda,
            contest: dupeContestPda,
          })
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("Unauthorized");
      }
    });

    it("rejects settlement with non-signer cosigner", async () => {
      // Create a new contest to test with
      const nonSignerContestId = createHash("sha256").update("non-signer-test").digest();
      const [nonSignerContestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), nonSignerContestId],
        program.programId
      );

      await program.methods
        .createContest(
          Array.from(nonSignerContestId) as any,
          new anchor.BN(toTokenAmount(9)),
          5,
          [],
          new anchor.BN(0)
        )
        .accountsStrict({
          payer: admin.publicKey,
          creator: admin.publicKey,
          vaultState: vaultStatePda,
          contest: nonSignerContestPda,
          mint: usdcMint,
          creatorTokenAccount: adminUsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      try {
        await program.methods
          .settleContest([])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: user1.publicKey,
            vaultState: vaultStatePda,
            contest: nonSignerContestPda,
          })
          .signers([user1])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("Unauthorized");
      }
    });

    it("rejects settling an already settled contest", async () => {
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );

      try {
        await program.methods
          .settleContest([])
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
            contest: contestPda,
          })
          .signers([signer2])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("ContestAlreadySettled");
      }
    });
  });

  describe("withdraw", () => {
    it("user1 withdraws USDC", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      const userBefore = await program.account.userAccount.fetch(userAccountPda);
      const tokenBefore = await getAccount(connection, user1UsdcAccount);
      const withdrawAmount = toTokenAmount(2);

      await program.methods
        .withdraw(new anchor.BN(withdrawAmount))
        .accountsStrict({
          user: user1.publicKey,
          userAccount: userAccountPda,
          vaultState: vaultStatePda,
          mint: usdcMint,
          userTokenAccount: user1UsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([user1])
        .rpc();

      const userAfter = await program.account.userAccount.fetch(userAccountPda);
      const tokenAfter = await getAccount(connection, user1UsdcAccount);

      expect(userAfter.balance.toNumber()).to.equal(
        userBefore.balance.toNumber() - withdrawAmount
      );
      expect(userAfter.totalWithdrawn.toNumber()).to.equal(withdrawAmount);
      expect(Number(tokenAfter.amount) - Number(tokenBefore.amount)).to.equal(withdrawAmount);
    });

    it("rejects withdrawal exceeding balance", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      try {
        await program.methods
          .withdraw(new anchor.BN(toTokenAmount(999999)))
          .accountsStrict({
            user: user1.publicKey,
            userAccount: userAccountPda,
            vaultState: vaultStatePda,
            mint: usdcMint,
            userTokenAccount: user1UsdcAccount,
            vaultTokenAccount: vaultUsdcPda,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([user1])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("InsufficientBalance");
      }
    });
  });

  describe("close_contest", () => {
    it("admin closes settled contest", async () => {
      const [contestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), contestId],
        program.programId
      );

      const adminBefore = await connection.getBalance(admin.publicKey);

      await program.methods
        .closeContest()
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          contest: contestPda,
        })
        .rpc();

      const adminAfter = await connection.getBalance(admin.publicKey);

      // Admin should have received rent back (minus tx fee)
      expect(adminAfter).to.be.greaterThan(adminBefore - 10000);

      // Contest account should no longer exist
      const account = await connection.getAccountInfo(contestPda);
      expect(account).to.be.null;
    });

    it("rejects closing unsettled contest", async () => {
      // Create a new open contest
      const freshContestId = createHash("sha256").update("close-test").digest();
      const [freshContestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), freshContestId],
        program.programId
      );

      await program.methods
        .createContest(
          Array.from(freshContestId) as any,
          new anchor.BN(toTokenAmount(9)),
          5,
          [],
          new anchor.BN(0)
        )
        .accountsStrict({
          payer: admin.publicKey,
          creator: admin.publicKey,
          vaultState: vaultStatePda,
          contest: freshContestPda,
          mint: usdcMint,
          creatorTokenAccount: adminUsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      try {
        await program.methods
          .closeContest()
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            contest: freshContestPda,
          })
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("ContestNotSettled");
      }
    });
  });

  describe("migrate_user_account", () => {
    it("no-ops on already current account (idempotent)", async () => {
      const [userAccountPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("user"), user1.publicKey.toBuffer()],
        program.programId
      );

      // Account is already at current size — migrate should be a no-op
      const beforeAccount = await program.account.userAccount.fetch(userAccountPda);

      await program.methods
        .migrateUserAccount()
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userAccount: userAccountPda,
          wallet: user1.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      // Verify nothing changed
      const afterAccount = await program.account.userAccount.fetch(userAccountPda);
      expect(afterAccount.balance.toNumber()).to.equal(beforeAccount.balance.toNumber());
      expect(afterAccount.seeds.toNumber()).to.equal(beforeAccount.seeds.toNumber());
      expect(afterAccount.wallet.toBase58()).to.equal(beforeAccount.wallet.toBase58());
    });
  });

  describe("multisig", () => {
    it("any signer can create a contest (single-signer routine op)", async () => {
      const signer2ContestId = createHash("sha256").update("signer2-test").digest();
      const [signer2ContestPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("contest"), signer2ContestId],
        program.programId
      );

      await program.methods
        .createContest(
          Array.from(signer2ContestId) as any,
          new anchor.BN(toTokenAmount(9)),
          5,
          [],
          new anchor.BN(0)
        )
        .accountsStrict({
          payer: signer2.publicKey,
          creator: signer2.publicKey,
          vaultState: vaultStatePda,
          contest: signer2ContestPda,
          mint: usdcMint,
          creatorTokenAccount: signer2UsdcAccount,
          vaultTokenAccount: vaultUsdcPda,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([signer2])
        .rpc();

      const contest = await program.account.contest.fetch(signer2ContestPda);
      expect(contest.entryFee.toNumber()).to.equal(toTokenAmount(9));
      expect(contest.admin.toBase58()).to.equal(signer2.publicKey.toBase58());
    });

    it("update_signers with valid 2-of-3 cosign", async () => {
      // Create a 4th keypair to swap in
      const newSigner = Keypair.generate();
      const fundTx = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: newSigner.publicKey,
          lamports: LAMPORTS_PER_SOL,
        })
      );
      await provider.sendAndConfirm(fundTx);

      // Update signers: replace signer3 with newSigner
      const newSigners = [admin.publicKey, signer2.publicKey, newSigner.publicKey];

      await program.methods
        .updateSigners(newSigners, 2)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();

      const vault = await program.account.vaultState.fetch(vaultStatePda);
      expect(vault.signers[0].toBase58()).to.equal(admin.publicKey.toBase58());
      expect(vault.signers[1].toBase58()).to.equal(signer2.publicKey.toBase58());
      expect(vault.signers[2].toBase58()).to.equal(newSigner.publicKey.toBase58());
      expect(vault.threshold).to.equal(2);

      // Restore original signers for remaining tests
      await program.methods
        .updateSigners([admin.publicKey, signer2.publicKey, signer3.publicKey], 2)
        .accountsStrict({
          admin: admin.publicKey,
          cosigner: signer2.publicKey,
          vaultState: vaultStatePda,
        })
        .signers([signer2])
        .rpc();
    });

    it("rejects update_signers with invalid threshold", async () => {
      try {
        await program.methods
          .updateSigners([admin.publicKey, signer2.publicKey, signer3.publicKey], 4)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([signer2])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("InvalidThreshold");
      }
    });

    it("rejects update_signers with duplicate signers", async () => {
      try {
        await program.methods
          .updateSigners([admin.publicKey, admin.publicKey, signer3.publicKey], 2)
          .accountsStrict({
            admin: admin.publicKey,
            cosigner: signer2.publicKey,
            vaultState: vaultStatePda,
          })
          .signers([signer2])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("DuplicateSigner");
      }
    });
  });

  describe("mint_entry_token", () => {
    // Helper: derive the EntryTokenAccount PDA for a given wallet + sequence
    const deriveEntryTokenPda = (wallet: PublicKey, sequence: number | bigint): PublicKey => {
      const seqBuf = Buffer.alloc(8);
      seqBuf.writeBigUInt64LE(BigInt(sequence));
      const [pda] = PublicKey.findProgramAddressSync(
        [Buffer.from("entry_token"), wallet.toBuffer(), seqBuf],
        program.programId
      );
      return pda;
    };

    // Helper: build a 64-byte source_ref array (left-aligned, zero-padded)
    const makeSourceRef = (s: string): number[] => {
      const buf = Buffer.alloc(64);
      buf.write(s, 0, "utf8");
      return Array.from(buf);
    };

    it("admin mints an entry token for user1 (sequence 0)", async () => {
      const sequence = new anchor.BN(0);
      const entryTokenPda = deriveEntryTokenPda(user1.publicKey, 0);
      const sourceRef = makeSourceRef("operator-mint-test-0");
      const STRIPE = 1;

      await program.methods
        .mintEntryToken(sequence, STRIPE, sourceRef)
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userWallet: user1.publicKey,
          entryToken: entryTokenPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const token = await program.account.entryTokenAccount.fetch(entryTokenPda);
      expect(token.owner.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(token.source).to.equal(STRIPE);
      expect(Array.from(token.sourceRef)).to.deep.equal(sourceRef);
      expect(token.consumed).to.equal(false);
      expect(token.consumedAt).to.equal(null);
      expect(token.createdAt.toNumber()).to.be.greaterThan(0);
      expect(token.bump).to.be.greaterThan(0);
    });

    it("admin mints a second entry token for user1 (sequence 1)", async () => {
      const sequence = new anchor.BN(1);
      const entryTokenPda = deriveEntryTokenPda(user1.publicKey, 1);
      const sourceRef = makeSourceRef("operator-mint-test-1");
      const OPERATOR = 0;

      await program.methods
        .mintEntryToken(sequence, OPERATOR, sourceRef)
        .accountsStrict({
          admin: admin.publicKey,
          vaultState: vaultStatePda,
          userWallet: user1.publicKey,
          entryToken: entryTokenPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const token = await program.account.entryTokenAccount.fetch(entryTokenPda);
      expect(token.owner.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(token.source).to.equal(OPERATOR);
      expect(token.consumed).to.equal(false);

      // Confirm sequence 0 still exists and is distinct
      const firstPda = deriveEntryTokenPda(user1.publicKey, 0);
      const firstToken = await program.account.entryTokenAccount.fetch(firstPda);
      expect(firstToken.owner.toBase58()).to.equal(user1.publicKey.toBase58());
      expect(firstPda.toBase58()).to.not.equal(entryTokenPda.toBase58());
    });

    it("rejects mint by non-admin signer", async () => {
      const sequence = new anchor.BN(99);
      const entryTokenPda = deriveEntryTokenPda(user2.publicKey, 99);
      const sourceRef = makeSourceRef("non-admin-attempt");
      const STRIPE = 1;

      try {
        await program.methods
          .mintEntryToken(sequence, STRIPE, sourceRef)
          .accountsStrict({
            admin: user1.publicKey, // user1 is NOT a vault signer
            vaultState: vaultStatePda,
            userWallet: user2.publicKey,
            entryToken: entryTokenPda,
            systemProgram: SystemProgram.programId,
          })
          .signers([user1])
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        expect(err.toString()).to.contain("Unauthorized");
      }
    });

    it("rejects re-mint of the same sequence (PDA collision)", async () => {
      // user1 already has sequence 0 from the first test
      const sequence = new anchor.BN(0);
      const entryTokenPda = deriveEntryTokenPda(user1.publicKey, 0);
      const sourceRef = makeSourceRef("dupe-attempt");
      const STRIPE = 1;

      try {
        await program.methods
          .mintEntryToken(sequence, STRIPE, sourceRef)
          .accountsStrict({
            admin: admin.publicKey,
            vaultState: vaultStatePda,
            userWallet: user1.publicKey,
            entryToken: entryTokenPda,
            systemProgram: SystemProgram.programId,
          })
          .rpc();
        expect.fail("Should have thrown");
      } catch (err) {
        // Anchor's init constraint fails when the account is already initialized.
        // The error surfaces as "already in use" / custom 0 from the system program.
        const msg = err.toString();
        const looksLikeAlreadyInit =
          msg.includes("already in use") ||
          msg.includes("custom program error: 0x0") ||
          msg.includes("custom program error: 0") ||
          msg.includes("AccountAlreadyInitialized");
        expect(looksLikeAlreadyInit, `Expected an "already in use" error, got: ${msg}`).to.equal(true);
      }
    });
  });
});

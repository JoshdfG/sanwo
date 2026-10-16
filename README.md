# Sanwo Finance — PayFi POC

Two Soroban smart contracts on Stellar Testnet demonstrating the core loan
loop for Sanwo Finance: **a borrower submits a receivable on-chain and
receives stablecoin capital from a pool in return.**

This is a proof-of-concept, not the full six-contract protocol. Scope is
intentionally narrow — see [What This POC Is Not](#what-this-poc-is-not).

```
contracts/
  capital-pool/      Holds testnet USDC, disburses loans, tracks pool stats
  receivables/        Accepts invoices, computes 75% LTV, calls into the pool
scripts/
  deploy.sh           End-to-end testnet deploy + seed + one full loan cycle
```

## Status

| Item | Status |
|---|---|
| Contracts written, with `DataKey`-based storage | Done |
| Unit + integration tests (47 tests, real cross-contract calls) | Done, all passing |
| Native `cargo test` run | Verified in this environment |
| `wasm32v1-none` release build | Not yet run — see [Build Environment Note](#build-environment-note) |
| Deployed to testnet | Not yet done — run `scripts/deploy.sh` locally |
| Contract addresses / tx hashes | Pending the deploy step above |

## Build Environment Note

These contracts were written and **tested using `cargo test` against
`soroban-sdk`'s native test harness** (the same harness Soroban's own docs
recommend for contract tests — it runs your contract logic against a
simulated host environment, not as compiled WASM, so it exercises real
storage, auth, events, and cross-contract calls). All 47 tests pass.

What wasn't possible in the sandbox this was authored in: producing the
actual deployable `.wasm` artifacts. That requires the `wasm32v1-none` Rust
target (Stellar's current recommended target as of Rust 1.84+, replacing
the older `wasm32-unknown-unknown`) installed via `rustup`, and this
environment only had network access to package registries (crates.io, npm,
apt), not `static.rust-lang.org` where Rust's target components are hosted.
So: **the logic is real and tested; the final build + deploy step needs to
run on a machine with a normal Rust toolchain.** That's a 10-minute job —
see below.

## Setup (run locally)

```bash
# 1. Install Rust 1.84+ if you don't have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Add the WASM target Soroban uses
rustup target add wasm32v1-none

# 3. Install the Stellar CLI
brew install stellar-cli
# or: cargo install --locked stellar-cli

# 4. Build
cargo build --target wasm32v1-none --release --workspace

# 5. Run tests (works without the wasm target too)
cargo test --workspace

# 6. Deploy to testnet, seed the pool, run one full loan cycle
chmod +x scripts/deploy.sh
./scripts/deploy.sh
```

`scripts/deploy.sh` handles the full sequence from the brief: deploy pool →
deploy receivables → initialize pool with receivables as the sole
authorized disburser → seed pool with 10,000 test USDC → run one complete
submit → disburse → repay cycle as a test borrower → write all addresses
and transaction hashes to `deploy-output.env`.

## Architecture

### Trust model

```
                 +-----------------------+
   borrower ---> | Receivables Contract  |
                 +-----------+-----------+
                              | disburse_loan(borrower, amount)
                              | (only this contract is trusted)
                              v
                 +-----------------------+
                 |   Capital Pool        | ---> borrower wallet (USDC)
                 |   Contract            |
                 +-----------------------+
```

Deploy order matters, and matches the brief exactly:

1. Deploy **Capital Pool Contract** -> get its address
2. Deploy **Receivables Contract**, `initialize(capital_pool_address)`
3. `initialize(admin, token, receivables_contract_address)` on the **Capital
   Pool Contract** — this is what locks `disburse_loan` down to only ever
   being callable by that one Receivables Contract address. Any other
   caller — including the pool's own admin, including a borrower calling
   directly — is rejected. This is enforced by Soroban's native
   contract-to-contract authorization (a contract calling another contract
   is automatically authorized *as itself*, and `require_auth()` on that
   stored address checks the real caller identity, not a signature anyone
   could forge).

Verified directly in tests:
[`pool_disburse_loan_cannot_be_called_by_a_contract_other_than_receivables`](contracts/receivables/src/test.rs)
deploys a second, structurally identical "decoy" Receivables contract and
proves it cannot trigger a disbursement from the same pool.

### Storage: `DataKey`, not `Map`

Both contracts use a `DataKey` enum (the idiomatic Soroban pattern) instead
of a single `Map<K, V>` collection:

- **Capital Pool** — all fields are scalars/Addresses (admin, token,
  receivables address, balances, counters), so they live in **instance
  storage** under individual `DataKey` variants (`DataKey::PoolBalance`,
  `DataKey::TotalDeposited`, etc). Instance storage is the right fit here:
  small, fixed-size data, read on essentially every call.
- **Receivables** — loans are unbounded and grow over time, so each loan
  lives in its own **persistent storage** entry keyed by
  `DataKey::Loan(invoice_hash)` — a "variable" DataKey. This means reading
  or writing one loan never touches any other loan's ledger entry, and the
  contract can hold an arbitrary number of loans without bumping into
  instance storage's size ceiling. Each write also extends that entry's
  TTL so it doesn't expire mid-demo.

### Deviations from the original brief (flagged, not silent)

The brief asked me to flag anything unclear or any place I saw a better
structure before writing production code. Four small deviations, kept
deliberately minimal:

1. **`Capital Pool.initialize` takes three params, not two.** The brief's
   function spec listed `(admin, token)`, but the "How the Two Contracts
   Connect" section requires the pool to know the Receivables Contract
   address to enforce the trust boundary. There's no way to do both without
   adding the third param (or a separate one-time
   `set_receivables_contract` call — happy to switch to that pattern if you
   prefer it split out, but it has to be lockable exactly once either way).
2. **Added `close_loan`** on the Capital Pool, called automatically by
   `repay_loan`. Not in the original function list, but without it
   `get_pool_stats().active_loans` would only ever go up, never down, which
   makes the read-only pool view ecosystem partners see during the demo
   misleading once a loan is repaid.
3. **Amounts are `i128`, not `u128`.** Soroban's token interface (including
   the Stellar Asset Contract / SAC interface used for real and testnet
   USDC) is defined in terms of `i128` throughout. Using `u128` in our
   contracts would mean constant casting at every token call and a real
   risk of silent truncation bugs. All amounts here are `i128` and the
   contracts reject non-positive values explicitly.
4. **"Empty invoice hash" means "not the zero hash."** `BytesN<32>` is
   fixed-length by construction (always exactly 32 bytes) so it can never
   be "empty" in the sense of zero-length. We reject the degenerate
   all-zero hash instead, which is the realistic equivalent for a client
   that failed to actually hash an invoice.

## Contract Interfaces (ABI)

### Capital Pool Contract

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize` | `admin: Address, token: Address, receivables_contract: Address` | `Result<(), PoolError>` | `admin` |
| `deposit` | `depositor: Address, amount: i128` | `Result<i128, PoolError>` (new pool balance) | `depositor` |
| `disburse_loan` | `borrower: Address, loan_amount: i128` | `Result<(), PoolError>` | Receivables Contract only |
| `close_loan` | — | `Result<(), PoolError>` | Receivables Contract only |
| `get_pool_stats` | — | `PoolStats { total_deposited, pool_balance, total_disbursed, active_loans }` | none |
| `get_receivables_contract` | — | `Address` | none |
| `get_token` | — | `Address` | none |

`PoolError`: `AlreadyInitialized=1, NotInitialized=2, NotAuthorizedCaller=3, InsufficientPoolBalance=4, InvalidAmount=5, NoActiveLoansToClose=6`

Events: `("deposited",) -> PoolDeposited{depositor, amount, new_pool_balance, timestamp}`,
`("disbursed",) -> LoanDisbursed{borrower, loan_amount, remaining_pool_balance, timestamp}`

### Receivables Contract

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize` | `capital_pool: Address` | `Result<(), ReceivablesError>` | none (deployer-run once) |
| `submit_receivable` | `borrower: Address, invoice_hash: BytesN<32>, invoice_amount: i128, buyer: Address` | `Result<i128, ReceivablesError>` (loan amount) | `borrower` |
| `get_loan_status` | `invoice_hash: BytesN<32>` | `Result<LoanRecord, ReceivablesError>` | none |
| `repay_loan` | `invoice_hash: BytesN<32>, repayment_amount: i128` | `Result<(), ReceivablesError>` | original `borrower` |

`LoanRecord`: `{ borrower: Address, buyer: Address, invoice_amount: i128, loan_amount: i128, status: LoanStatus, timestamp: u64 }`
`LoanStatus`: `Pending | Active | Repaid`
`ReceivablesError`: `AlreadyInitialized=1, NotInitialized=2, EmptyInvoiceHash=3, InvoiceAmountTooLow=4, DuplicateInvoice=5, LoanNotFound=6, LoanNotActive=7, RepaymentTooLow=8`

Events: `("submitted",) -> ReceivableSubmitted{borrower, invoice_hash, invoice_amount, loan_amount, timestamp}`,
`("repaid",) -> LoanRepaid{borrower, invoice_hash, repayment_amount, timestamp}`

**Note on `buyer`:** the brief marks this optional for the POC. Soroban
doesn't have an ergonomic nullable `Address`, so the frontend should pass a
placeholder address (e.g. the borrower's own address, or a well-known
zero/burn address you designate) when there's no real buyer yet. No
verification of `buyer` happens in this contract — that's Month 2/3 scope.

**Note on decimals:** `invoice_amount`, `loan_amount`, and all token amounts
are raw `i128` values in the token contract's smallest unit. The frontend
is responsible for scaling — e.g. if the token uses 7 decimals (standard
for Stellar classic assets wrapped as SAC), "$10" is `100_000_000`. The $10
floor (`MIN_INVOICE_AMOUNT`) is currently a *raw* constant of `10` in the
contract, which assumes whatever scale the frontend consistently uses —
flag if you want this expressed against a fixed decimal count instead.

## Testing

```bash
cargo test --workspace
```

47 tests, all passing:

- **`capital-pool`** (20 tests): initialization guards, deposit accounting,
  the disbursement auth lockdown, insufficient-balance handling, pool stats
  accuracy across multiple loans, event emission.
- **`receivables`** (27 tests): LTV math (including integer-division
  rounding), duplicate invoice rejection (keyed by hash, not borrower),
  borrower auth enforcement on both submit and repay, the **real
  cross-contract disbursement** (two actual deployed contracts, a genuine
  token transfer, not a stub), atomic rollback when the pool can't cover a
  loan, the full submit -> disburse -> repay lifecycle, and — the most
  important one — proof that a second, decoy Receivables contract cannot
  trigger disbursement from a pool that was initialized to trust only the
  original.

Run a single test: `cargo test -p receivables full_loan_cycle_submit_disburse_repay`

## What This POC Is Not

Scope is deliberately narrow so we don't over-engineer the proof:

- No credit scoring — LTV fixed at 75% for all loans
- No buyer verification or buyer signature step
- No liquidation logic
- No Trust Circle / vouching (Month 3)
- No behavioral monitoring (Month 3 backend)
- No yield calculation for depositors
- No real USDC required — testnet/mock token is fine

## License

Apache-2.0

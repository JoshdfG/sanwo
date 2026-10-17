# Sanwo Contracts

Soroban smart contracts for Sanwo, receivables financing on Stellar. A business
submits an invoice its client owes, and the pool advances 75% of it in the same
transaction. When the advance is repaid, the funds go back into the pool, and
depositors can withdraw their principal.

This is the proof of concept: two contracts, deployed on Stellar testnet, running
the core advance loop end to end. It is deliberately narrower than the full
protocol. See [Limitations](#limitations).

```
contracts/
  capital-pool/   Holds the pool's tokens: deposits, advances, repayments, withdrawals
  receivables/    Registers invoices, computes the 75% advance, pays and collects it via the pool
scripts/
  deploy.sh       Builds, deploys, initializes, seeds the pool, runs one full cycle
```

## Deployment

Live on Stellar testnet:

| Contract | Address |
|---|---|
| Capital Pool | [`CBFWEYF3B4CESQAS4RVKJLEU52EZLWPEJL4KVRTSUFI73DBI4RY3MV34`](https://stellar.expert/explorer/testnet/contract/CBFWEYF3B4CESQAS4RVKJLEU52EZLWPEJL4KVRTSUFI73DBI4RY3MV34) |
| Receivables | [`CBYOWRE3PCTVA53DWVCRXNCGP5NCG4ZB62ZJ6S5ZYWKJGQIYKNRFVR2O`](https://stellar.expert/explorer/testnet/contract/CBYOWRE3PCTVA53DWVCRXNCGP5NCG4ZB62ZJ6S5ZYWKJGQIYKNRFVR2O) |
| Token | [`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`](https://stellar.expert/explorer/testnet/contract/CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC), the native XLM asset contract, standing in for USDC on testnet |

Example transactions:

- [Advance](https://stellar.expert/explorer/testnet/tx/b174950b6102771962813cb584ceee5e8729fd2b4c216f0eb885d026b5cff9e6): `submit_receivable` registers a 600 USD invoice and the pool pays 450 XLM to the borrower in the same transaction.
- [Repayment](https://stellar.expert/explorer/testnet/tx/cade2c22537ca9ee6d37a2e4577ceba880541e432b25a301f96de697f153f999): `repay_loan` moves the 450 XLM from the borrower back into the pool and closes the advance.
- [Deposit](https://stellar.expert/explorer/testnet/tx/b90c29f44946d66fd29fe1e48518fd43533f2afaa19e15e1c553182b9eb3e6c8): 500 XLM into the Capital Pool.
- [Withdrawal](https://stellar.expert/explorer/testnet/tx/8a1c7416f63f45e091eca5344d78641c1a75b6df615ea05817dfacbb612c900a): 200 XLM of that deposit back to the depositor.

## Status

| Item | Status |
|---|---|
| Capital Pool and Receivables contracts | Done |
| Tests | 58 passing: 29 in `capital-pool`, 29 in `receivables` |
| Testnet deployment | Live, addresses above |
| Web app | Calls these contracts directly from the browser |

## Build, test and deploy

Requires Rust 1.84 or later and the Stellar CLI.

```bash
# The WASM target Soroban uses
rustup target add wasm32v1-none

# Stellar CLI
brew install stellar-cli
# or: cargo install --locked stellar-cli

# Build
cargo build --target wasm32v1-none --release --workspace

# Test
cargo test --workspace

# Deploy to testnet, seed the pool, run one full loan cycle
chmod +x scripts/deploy.sh
./scripts/deploy.sh
```

`scripts/deploy.sh` deploys the Capital Pool, deploys Receivables, initializes
the pool with Receivables as its only trusted caller, seeds the pool with 1,000
XLM, runs one submit, disburse and repay cycle as a test borrower, withdraws part
of the seed deposit, and prints the pool stats. Set `STELLAR_IDENTITY` to choose
the deploying key.

## Architecture

### Trust model

```
                 +-----------------------+
   borrower ---> | Receivables Contract  |
                 +-----------+-----------+
                             | disburse_loan / record_repayment
                             | (only this contract is trusted)
                             v
                 +-----------------------+
                 |   Capital Pool        | ---> borrower wallet
                 |   Contract            |
                 +-----------------------+
```

Deploy order matters:

1. Deploy the **Capital Pool** and note its address.
2. Deploy **Receivables** and call `initialize(capital_pool_address)`.
3. Call `initialize_cp(admin, token, receivables_contract_address)` on the
   **Capital Pool**. This locks `disburse_loan` and `record_repayment` to that
   one Receivables address.
   Any other caller is rejected, including the pool's own admin and a borrower
   calling directly. Soroban's contract-to-contract authorization enforces it:
   a contract calling another contract is authorized as itself, and
   `require_auth()` on the stored address checks the real caller.

The test
[`pool_disburse_loan_cannot_be_called_by_a_contract_other_than_receivables`](contracts/receivables/src/test.rs)
deploys a second, identical "decoy" Receivables contract and shows it cannot
trigger a disbursement from the same pool.

### Storage: `DataKey`, not `Map`

Both contracts use a `DataKey` enum, the idiomatic Soroban pattern, rather than
a single `Map<K, V>`:

- **Capital Pool.** Every field is a scalar or an address (admin, token,
  receivables address, balances, counters), so each lives in **instance
  storage** under its own key, such as `DataKey::PoolBalance`. Instance storage
  suits small, fixed-size data that almost every call reads.
- **Receivables.** Loans grow without bound, so each one lives in its own
  **persistent storage** entry under `DataKey::Loan(invoice_hash)`. Reading or
  writing one loan never touches another, and the contract can hold any number
  of loans without reaching the instance storage size limit. Every write also
  extends that entry's TTL.

### Repayments and withdrawals

`repay_loan` moves exactly what is owed (the advance; the PoC charges no fee)
from the borrower into the Capital Pool, then calls `record_repayment`. The pool
checks that its token balance really covers the repayment before booking it, so
a repayment cannot be recorded without the funds. An overpayment is never kept:
`repayment_amount` only has to cover what is owed.

`withdraw` returns principal. Each depositor's principal is tracked under
`DataKey::Deposit(address)`, and a withdrawal is limited both to that amount and
to liquidity that is not out in advances.

### Design decisions

1. **`initialize_cp` takes three parameters:** `admin`, `token` and the
   Receivables address. The pool has to know the Receivables address to enforce
   the trust boundary above. A separate one-time setter would also work, but it
   would need the same lock.
2. **`record_repayment` on the Capital Pool.** `repay_loan` calls it after
   moving the funds, so the pool books the repayment and
   `get_pool_stats().active_loans` goes down when a loan is repaid.
3. **Amounts are `i128`, not `u128`.** Soroban's token interface, including the
   Stellar Asset Contract used for real and testnet USDC, uses `i128`
   throughout. Matching it avoids casts at every token call and the truncation
   bugs they invite. Both contracts reject non-positive amounts.
4. **The pool is a test-only dependency of Receivables.** Receivables talks to
   the pool through a small client interface. Linking the pool crate as a normal
   dependency would compile every pool function into the Receivables contract.
5. **An "empty" invoice hash means the all-zero hash.** `BytesN<32>` is always
   exactly 32 bytes, so it cannot be empty. The contract rejects the all-zero
   hash, which is what a client that never hashed an invoice would send.

## Contract interfaces

### Capital Pool

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize_cp` | `admin: Address, token: Address, receivables_contract: Address` | `Result<(), PoolError>` | `admin` |
| `deposit` | `depositor: Address, amount: i128` | `Result<i128, PoolError>` (new pool balance) | `depositor` |
| `withdraw` | `depositor: Address, amount: i128` | `Result<i128, PoolError>` (new pool balance) | `depositor` |
| `get_deposit` | `depositor: Address` | `i128` (principal in the pool) | none |
| `disburse_loan` | `borrower: Address, loan_amount: i128` | `Result<(), PoolError>` | Receivables contract only |
| `record_repayment` | `amount: i128` | `Result<i128, PoolError>` (new pool balance) | Receivables contract only |
| `get_pool_stats` | none | `PoolStats { total_deposited, pool_balance, total_disbursed, total_repaid, total_withdrawn, active_loans }` | none |
| `get_receivables_contract` | none | `Address` | none |
| `get_token` | none | `Address` | none |

`PoolError`: `AlreadyInitialized=1, NotInitialized=2, NotAuthorizedCaller=3, InsufficientPoolBalance=4, InvalidAmount=5, NoActiveLoansToClose=6, RepaymentNotReceived=7, WithdrawalExceedsDeposit=8`

Events: `("deposited",) -> PoolDeposited{depositor, amount, new_pool_balance, timestamp}`,
`("disbursed",) -> LoanDisbursed{borrower, loan_amount, remaining_pool_balance, timestamp}`,
`("received",) -> LoanRepaymentReceived{amount, new_pool_balance, timestamp}`,
`("withdrawn",) -> PoolWithdrawn{depositor, amount, new_pool_balance, timestamp}`

### Receivables

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize` | `capital_pool: Address` | `Result<(), ReceivablesError>` | none (run once by the deployer) |
| `submit_receivable` | `borrower: Address, invoice_hash: BytesN<32>, invoice_amount: i128, buyer: Address` | `Result<i128, ReceivablesError>` (advance amount) | `borrower` |
| `get_loan_status` | `invoice_hash: BytesN<32>` | `Result<LoanRecord, ReceivablesError>` | none |
| `repay_loan` | `invoice_hash: BytesN<32>, repayment_amount: i128` | `Result<(), ReceivablesError>` (moves what is owed into the pool) | original `borrower` |

`LoanRecord`: `{ borrower: Address, buyer: Address, invoice_amount: i128, loan_amount: i128, status: LoanStatus, timestamp: u64 }`
`LoanStatus`: `Pending | Active | Repaid`
`ReceivablesError`: `AlreadyInitialized=1, NotInitialized=2, EmptyInvoiceHash=3, InvoiceAmountTooLow=4, DuplicateInvoice=5, LoanNotFound=6, LoanNotActive=7, RepaymentTooLow=8`

Events: `("submitted",) -> ReceivableSubmitted{borrower, invoice_hash, invoice_amount, loan_amount, timestamp}`,
`("repaid",) -> LoanRepaid{borrower, invoice_hash, repayment_amount, timestamp}`

**`buyer`:** Soroban has no ergonomic nullable `Address`, so callers without a
real buyer pass a placeholder address. The contract does not verify `buyer` yet.

**Decimals:** `invoice_amount`, `loan_amount` and all token amounts are raw
`i128` values in the token's smallest unit. Callers handle scaling: with 7
decimals, standard for Stellar assets, $10 is `100_000_000`. The minimum invoice
(`MIN_INVOICE_AMOUNT`) is currently a raw `10`, so it depends on the caller's
scale.

## Testing

```bash
cargo test --workspace
```

58 tests, all passing:

- **`capital-pool`** (29): initialization guards, deposit accounting, per-depositor
  principal, the disbursement lock, insufficient-balance handling, repayments
  (booked only when the funds arrived), withdrawals (limited to the depositor's
  principal and to available liquidity, with depositor auth), pool stats and events.
- **`receivables`** (29): the 75% advance math, including integer rounding;
  duplicate rejection keyed by hash rather than borrower; borrower auth on submit
  and repay; a real cross-contract disbursement with a genuine token transfer;
  repayments that return funds to the pool, keep an overpayment with the borrower,
  and fail cleanly when the borrower cannot pay; full rollback when the pool cannot
  cover an advance; the full cycle through to a depositor withdrawing; and the
  decoy-contract test described above.

Run one test: `cargo test -p receivables full_loan_cycle_submit_disburse_repay`

## Limitations

The scope is deliberately narrow. In this version:

- Withdrawals return **principal only**. There are no share tokens yet, so yield
  and losses are not shared pro rata, and withdrawals are served first come,
  first served from available liquidity.
- On testnet the borrower repays the advance from their own wallet. Collection
  from the client to a controlled address per invoice is planned for v1.
- The token on testnet is native XLM, standing in for USDC.
- The advance rate is fixed at 75%. There is no credit scoring or fee.
- There is no buyer verification or buyer signature step.
- There is no liquidation logic, Trust Circle vouching, behavioral monitoring,
  or depositor yield calculation.

## License

Apache-2.0

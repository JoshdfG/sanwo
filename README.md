# Sanwo Contracts

Soroban smart contracts for Sanwo, receivables financing on Stellar. A business
submits an invoice its client owes, and the pool advances 75% of it in the same
transaction. When the client pays, the advance is marked repaid on-chain.

This is the proof of concept: two contracts, deployed on Stellar testnet, running
the core advance loop end to end. It is deliberately narrower than the full
protocol. See [Limitations](#limitations).

```
contracts/
  capital-pool/   Holds the pool's tokens, takes deposits, pays advances, tracks stats
  receivables/    Registers invoices, computes the 75% advance, calls the pool to pay it
scripts/
  deploy.sh       Builds, deploys, initializes, seeds the pool, runs one full loan cycle
```

## Deployment

Live on Stellar testnet:

| Contract | Address |
|---|---|
| Capital Pool | [`CBIDZFVYXVAFNJUDKZAB43JIY23BQMO56PKO6HUYI43S4XU2HNHTTVF2`](https://stellar.expert/explorer/testnet/contract/CBIDZFVYXVAFNJUDKZAB43JIY23BQMO56PKO6HUYI43S4XU2HNHTTVF2) |
| Receivables | [`CC7YS73X7J6AIAVF6HSI62NH4AVHEEBMSQBBQUCCFMIQTRJS7V6FOVYO`](https://stellar.expert/explorer/testnet/contract/CC7YS73X7J6AIAVF6HSI62NH4AVHEEBMSQBBQUCCFMIQTRJS7V6FOVYO) |
| Token | [`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`](https://stellar.expert/explorer/testnet/contract/CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC), the native XLM asset contract, standing in for USDC on testnet |

Example transactions:

- [Advance](https://stellar.expert/explorer/testnet/tx/0429899b47bd54f3c37e2ae172878f28e161e35ba00e5e7d9d9d6f746b65b86b): `submit_receivable` registers the invoice and the pool pays 420 XLM to the borrower in the same transaction.
- [Repayment](https://stellar.expert/explorer/testnet/tx/264e5b3fe13524e1907eefd61378f04908236bcc404aa60d16886e9d6cd59030): `repay_loan` marks an advance repaid and closes it in the pool.
- [Deposit](https://stellar.expert/explorer/testnet/tx/3e8cbcd15acc5bc8ee99ef50e518e1379264b8470f901c726103a3d31434ecd0): 1,000 XLM into the Capital Pool.

## Status

| Item | Status |
|---|---|
| Capital Pool and Receivables contracts | Done |
| Tests | 47 passing: 20 in `capital-pool`, 27 in `receivables` |
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
the pool with Receivables as its only authorized disburser, seeds the pool with
1,000 XLM, runs one submit, disburse and repay cycle as a test borrower, and
writes every address and transaction hash to `deploy-output.env`.

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
                 |   Capital Pool        | ---> borrower wallet
                 |   Contract            |
                 +-----------------------+
```

Deploy order matters:

1. Deploy the **Capital Pool** and note its address.
2. Deploy **Receivables** and call `initialize(capital_pool_address)`.
3. Call `initialize_cp(admin, token, receivables_contract_address)` on the
   **Capital Pool**. This locks `disburse_loan` to that one Receivables address.
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

### Design decisions

1. **`initialize_cp` takes three parameters:** `admin`, `token` and the
   Receivables address. The pool has to know the Receivables address to enforce
   the trust boundary above. A separate one-time setter would also work, but it
   would need the same lock.
2. **`close_loan` on the Capital Pool.** `repay_loan` calls it, so
   `get_pool_stats().active_loans` goes down when a loan is repaid instead of
   only ever going up.
3. **Amounts are `i128`, not `u128`.** Soroban's token interface, including the
   Stellar Asset Contract used for real and testnet USDC, uses `i128`
   throughout. Matching it avoids casts at every token call and the truncation
   bugs they invite. Both contracts reject non-positive amounts.
4. **An "empty" invoice hash means the all-zero hash.** `BytesN<32>` is always
   exactly 32 bytes, so it cannot be empty. The contract rejects the all-zero
   hash, which is what a client that never hashed an invoice would send.

## Contract interfaces

### Capital Pool

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize_cp` | `admin: Address, token: Address, receivables_contract: Address` | `Result<(), PoolError>` | `admin` |
| `deposit` | `depositor: Address, amount: i128` | `Result<i128, PoolError>` (new pool balance) | `depositor` |
| `disburse_loan` | `borrower: Address, loan_amount: i128` | `Result<(), PoolError>` | Receivables contract only |
| `close_loan` | none | `Result<(), PoolError>` | Receivables contract only |
| `get_pool_stats` | none | `PoolStats { total_deposited, pool_balance, total_disbursed, active_loans }` | none |
| `get_receivables_contract` | none | `Address` | none |
| `get_token` | none | `Address` | none |

`PoolError`: `AlreadyInitialized=1, NotInitialized=2, NotAuthorizedCaller=3, InsufficientPoolBalance=4, InvalidAmount=5, NoActiveLoansToClose=6`

Events: `("deposited",) -> PoolDeposited{depositor, amount, new_pool_balance, timestamp}`,
`("disbursed",) -> LoanDisbursed{borrower, loan_amount, remaining_pool_balance, timestamp}`

### Receivables

| Function | Params | Returns | Auth |
|---|---|---|---|
| `initialize` | `capital_pool: Address` | `Result<(), ReceivablesError>` | none (run once by the deployer) |
| `submit_receivable` | `borrower: Address, invoice_hash: BytesN<32>, invoice_amount: i128, buyer: Address` | `Result<i128, ReceivablesError>` (advance amount) | `borrower` |
| `get_loan_status` | `invoice_hash: BytesN<32>` | `Result<LoanRecord, ReceivablesError>` | none |
| `repay_loan` | `invoice_hash: BytesN<32>, repayment_amount: i128` | `Result<(), ReceivablesError>` | original `borrower` |

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

47 tests, all passing:

- **`capital-pool`** (20): initialization guards, deposit accounting, the
  disbursement lock, insufficient-balance handling, pool stats across several
  loans, and events.
- **`receivables`** (27): the 75% advance math, including integer rounding;
  duplicate rejection keyed by hash rather than borrower; borrower auth on submit
  and repay; a real cross-contract disbursement between two deployed contracts
  with a genuine token transfer; full rollback when the pool cannot cover an
  advance; the full submit, disburse and repay cycle; and the decoy-contract test
  described above.

Run one test: `cargo test -p receivables full_loan_cycle_submit_disburse_repay`

## Limitations

The scope is deliberately narrow. In this version:

- `repay_loan` records the repayment and closes the loan, but **no tokens move
  back to the pool**. The test `repay_loan_does_not_return_funds_to_pool_in_poc`
  pins this behavior.
- The pool has **no withdraw function** and issues no share tokens.
- The token on testnet is native XLM, standing in for USDC.
- The advance rate is fixed at 75%. There is no credit scoring or fee.
- There is no buyer verification or buyer signature step.
- There is no liquidation logic, Trust Circle vouching, behavioral monitoring,
  or depositor yield calculation.

## License

Apache-2.0

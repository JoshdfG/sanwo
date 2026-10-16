use crate::{capital_pool, storage::*};
use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env};

#[contract]
pub struct ReceivablesContract;

#[contractimpl]
impl ReceivablesContract {
    pub fn initialize(env: Env, capital_pool: Address) -> Result<(), ReceivablesError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ReceivablesError::AlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&DataKey::CapitalPool, &capital_pool);
        env.storage().instance().set(&DataKey::Initialized, &true);

        Ok(())
    }

    pub fn submit_receivable(
        env: Env,
        borrower: Address,
        invoice_hash: BytesN<32>,
        invoice_amount: i128,
        buyer: Address,
    ) -> Result<i128, ReceivablesError> {
        Self::require_initialized(&env)?;

        borrower.require_auth();

        if invoice_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(ReceivablesError::EmptyInvoiceHash);
        }

        if invoice_amount < MIN_INVOICE_AMOUNT {
            return Err(ReceivablesError::InvoiceAmountTooLow);
        }

        let loan_key = DataKey::Loan(invoice_hash.clone());
        if env.storage().persistent().has(&loan_key) {
            return Err(ReceivablesError::DuplicateInvoice);
        }

        let loan_amount = (invoice_amount * LTV_BPS) / BPS_DENOMINATOR;

        let timestamp = env.ledger().timestamp();

        let record = LoanRecord {
            borrower: borrower.clone(),
            buyer,
            invoice_amount,
            loan_amount,
            status: LoanStatus::Pending,
            timestamp,
        };
        env.storage().persistent().set(&loan_key, &record);
        Self::bump_loan_ttl(&env, &loan_key);

        env.events().publish(
            (symbol_short!("submitted"),),
            ReceivableSubmitted {
                borrower: borrower.clone(),
                invoice_hash: invoice_hash.clone(),
                invoice_amount,
                loan_amount,
                timestamp,
            },
        );

        let capital_pool: Address = env.storage().instance().get(&DataKey::CapitalPool).unwrap();
        let pool_client = capital_pool::Client::new(&env, &capital_pool);
        pool_client.disburse_loan(&borrower, &loan_amount);

        let mut updated = record;
        updated.status = LoanStatus::Active;
        env.storage().persistent().set(&loan_key, &updated);
        Self::bump_loan_ttl(&env, &loan_key);

        Ok(loan_amount)
    }

    pub fn get_loan_status(
        env: Env,
        invoice_hash: BytesN<32>,
    ) -> Result<LoanRecord, ReceivablesError> {
        env.storage()
            .persistent()
            .get(&DataKey::Loan(invoice_hash))
            .ok_or(ReceivablesError::LoanNotFound)
    }

    pub fn repay_loan(
        env: Env,
        invoice_hash: BytesN<32>,
        repayment_amount: i128,
    ) -> Result<(), ReceivablesError> {
        Self::require_initialized(&env)?;

        let loan_key = DataKey::Loan(invoice_hash.clone());
        let mut record: LoanRecord = env
            .storage()
            .persistent()
            .get(&loan_key)
            .ok_or(ReceivablesError::LoanNotFound)?;

        record.borrower.require_auth();

        if record.status != LoanStatus::Active {
            return Err(ReceivablesError::LoanNotActive);
        }

        if repayment_amount < record.loan_amount {
            return Err(ReceivablesError::RepaymentTooLow);
        }

        record.status = LoanStatus::Repaid;
        env.storage().persistent().set(&loan_key, &record);
        Self::bump_loan_ttl(&env, &loan_key);

        let timestamp = env.ledger().timestamp();

        env.events().publish(
            (symbol_short!("repaid"),),
            LoanRepaid {
                borrower: record.borrower,
                invoice_hash,
                repayment_amount,
                timestamp,
            },
        );

        let capital_pool: Address = env.storage().instance().get(&DataKey::CapitalPool).unwrap();
        let pool_client = capital_pool::Client::new(&env, &capital_pool);
        pool_client.close_loan();

        Ok(())
    }

    fn require_initialized(env: &Env) -> Result<(), ReceivablesError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(ReceivablesError::NotInitialized);
        }
        Ok(())
    }

    fn bump_loan_ttl(env: &Env, key: &DataKey) {
        const ONE_DAY_LEDGERS: u32 = 17280;
        const TTL_LEDGERS: u32 = ONE_DAY_LEDGERS * 30;
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_LEDGERS, TTL_LEDGERS);
    }
}

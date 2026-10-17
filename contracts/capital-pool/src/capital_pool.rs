use crate::storage::{
    DataKey, LoanDisbursed, LoanRepaymentReceived, PoolDeposited, PoolError, PoolStats, PoolWithdrawn,
};
use soroban_sdk::{contract, contractimpl, symbol_short, token::TokenClient, Address, Env};

#[contract]
pub struct CapitalPoolContract;

#[contractimpl]
impl CapitalPoolContract {
    pub fn initialize_cp(
        env: Env,
        admin: Address,
        token: Address,
        receivables_contract: Address,
    ) -> Result<(), PoolError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(PoolError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage()
            .instance()
            .set(&DataKey::ReceivablesContract, &receivables_contract);
        env.storage().instance().set(&DataKey::PoolBalance, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposited, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalDisbursed, &0i128);
        env.storage().instance().set(&DataKey::TotalRepaid, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawn, &0i128);
        env.storage().instance().set(&DataKey::ActiveLoans, &0u32);
        env.storage().instance().set(&DataKey::Initialized, &true);

        Ok(())
    }

    pub fn deposit(env: Env, depositor: Address, amount: i128) -> Result<i128, PoolError> {
        Self::require_initialized(&env)?;

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        depositor.require_auth();

        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = TokenClient::new(&env, &token_address);
        token_client.transfer(&depositor, &env.current_contract_address(), &amount);

        let pool_balance: i128 = env.storage().instance().get(&DataKey::PoolBalance).unwrap();
        let total_deposited: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDeposited)
            .unwrap();

        let new_balance = pool_balance + amount;
        let new_total_deposited = total_deposited + amount;

        env.storage()
            .instance()
            .set(&DataKey::PoolBalance, &new_balance);
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposited, &new_total_deposited);

        let deposit_key = DataKey::Deposit(depositor.clone());
        let deposited: i128 = env.storage().persistent().get(&deposit_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&deposit_key, &(deposited + amount));
        Self::bump_ttl(&env, &deposit_key);

        env.events().publish(
            (symbol_short!("deposited"),),
            PoolDeposited {
                depositor,
                amount,
                new_pool_balance: new_balance,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(new_balance)
    }

    pub fn disburse_loan(env: Env, borrower: Address, loan_amount: i128) -> Result<(), PoolError> {
        Self::require_initialized(&env)?;

        if loan_amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        let receivables_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::ReceivablesContract)
            .unwrap();

        receivables_contract.require_auth();

        let pool_balance: i128 = env.storage().instance().get(&DataKey::PoolBalance).unwrap();

        if pool_balance < loan_amount {
            return Err(PoolError::InsufficientPoolBalance);
        }

        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = TokenClient::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &borrower, &loan_amount);

        let new_balance = pool_balance - loan_amount;
        let total_disbursed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalDisbursed)
            .unwrap();
        let active_loans: u32 = env.storage().instance().get(&DataKey::ActiveLoans).unwrap();

        env.storage()
            .instance()
            .set(&DataKey::PoolBalance, &new_balance);
        env.storage()
            .instance()
            .set(&DataKey::TotalDisbursed, &(total_disbursed + loan_amount));
        env.storage()
            .instance()
            .set(&DataKey::ActiveLoans, &(active_loans + 1));

        env.events().publish(
            (symbol_short!("disbursed"),),
            LoanDisbursed {
                borrower,
                loan_amount,
                remaining_pool_balance: new_balance,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Called by the Receivables contract after it has moved a repayment into
    /// the pool. Checks the tokens actually arrived, books them back into the
    /// pool balance, and closes one active loan.
    pub fn record_repayment(env: Env, amount: i128) -> Result<i128, PoolError> {
        Self::require_initialized(&env)?;

        let receivables_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::ReceivablesContract)
            .unwrap();
        receivables_contract.require_auth();

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        let active_loans: u32 = env.storage().instance().get(&DataKey::ActiveLoans).unwrap();
        if active_loans == 0 {
            return Err(PoolError::NoActiveLoansToClose);
        }

        let pool_balance: i128 = env.storage().instance().get(&DataKey::PoolBalance).unwrap();
        let new_balance = pool_balance + amount;

        // The token balance must already cover the booked balance plus this repayment.
        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let held = TokenClient::new(&env, &token_address).balance(&env.current_contract_address());
        if held < new_balance {
            return Err(PoolError::RepaymentNotReceived);
        }

        let total_repaid: i128 = env.storage().instance().get(&DataKey::TotalRepaid).unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::PoolBalance, &new_balance);
        env.storage()
            .instance()
            .set(&DataKey::TotalRepaid, &(total_repaid + amount));
        env.storage()
            .instance()
            .set(&DataKey::ActiveLoans, &(active_loans - 1));

        env.events().publish(
            (symbol_short!("received"),),
            LoanRepaymentReceived {
                amount,
                new_pool_balance: new_balance,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(new_balance)
    }

    /// Returns principal to a depositor: up to what they deposited, and only
    /// from liquidity that is not out on loan.
    pub fn withdraw(env: Env, depositor: Address, amount: i128) -> Result<i128, PoolError> {
        Self::require_initialized(&env)?;

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        depositor.require_auth();

        let deposit_key = DataKey::Deposit(depositor.clone());
        let deposited: i128 = env.storage().persistent().get(&deposit_key).unwrap_or(0);
        if amount > deposited {
            return Err(PoolError::WithdrawalExceedsDeposit);
        }

        let pool_balance: i128 = env.storage().instance().get(&DataKey::PoolBalance).unwrap();
        if amount > pool_balance {
            return Err(PoolError::InsufficientPoolBalance);
        }

        let token_address: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        TokenClient::new(&env, &token_address).transfer(
            &env.current_contract_address(),
            &depositor,
            &amount,
        );

        let new_balance = pool_balance - amount;
        let total_withdrawn: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalWithdrawn)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::PoolBalance, &new_balance);
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawn, &(total_withdrawn + amount));
        env.storage()
            .persistent()
            .set(&deposit_key, &(deposited - amount));
        Self::bump_ttl(&env, &deposit_key);

        env.events().publish(
            (symbol_short!("withdrawn"),),
            PoolWithdrawn {
                depositor,
                amount,
                new_pool_balance: new_balance,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(new_balance)
    }

    /// Principal a depositor currently has in the pool.
    pub fn get_deposit(env: Env, depositor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Deposit(depositor))
            .unwrap_or(0)
    }

    pub fn get_pool_stats(env: Env) -> PoolStats {
        PoolStats {
            total_deposited: env
                .storage()
                .instance()
                .get(&DataKey::TotalDeposited)
                .unwrap_or(0),
            pool_balance: env
                .storage()
                .instance()
                .get(&DataKey::PoolBalance)
                .unwrap_or(0),
            total_disbursed: env
                .storage()
                .instance()
                .get(&DataKey::TotalDisbursed)
                .unwrap_or(0),
            total_repaid: env
                .storage()
                .instance()
                .get(&DataKey::TotalRepaid)
                .unwrap_or(0),
            total_withdrawn: env
                .storage()
                .instance()
                .get(&DataKey::TotalWithdrawn)
                .unwrap_or(0),
            active_loans: env
                .storage()
                .instance()
                .get(&DataKey::ActiveLoans)
                .unwrap_or(0),
        }
    }

    pub fn get_receivables_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::ReceivablesContract)
            .unwrap()
    }

    pub fn get_token(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Token).unwrap()
    }

    fn bump_ttl(env: &Env, key: &DataKey) {
        const ONE_DAY_LEDGERS: u32 = 17280;
        const TTL_LEDGERS: u32 = ONE_DAY_LEDGERS * 30;
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_LEDGERS, TTL_LEDGERS);
    }

    fn require_initialized(env: &Env) -> Result<(), PoolError> {
        if !env.storage().instance().has(&DataKey::Initialized) {
            return Err(PoolError::NotInitialized);
        }
        Ok(())
    }
}

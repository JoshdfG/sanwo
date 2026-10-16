#![no_std]

pub mod storage;

pub mod receivable_contract;

#[cfg(test)]
mod test;

mod capital_pool {
    use soroban_sdk::{contractclient, Address, Env};

    #[contractclient(name = "Client")]
    pub trait CapitalPoolInterface {
        fn disburse_loan(env: Env, borrower: Address, loan_amount: i128);
        fn close_loan(env: Env);
    }
}

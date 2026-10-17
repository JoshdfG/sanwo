use soroban_sdk::{contracterror, contracttype, Address};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    ReceivablesContract,
    PoolBalance,
    TotalDeposited,
    TotalDisbursed,
    TotalRepaid,
    TotalWithdrawn,
    ActiveLoans,
    Initialized,
    /// Principal a depositor has in the pool (persistent, one entry per depositor).
    Deposit(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStats {
    pub total_deposited: i128,
    pub pool_balance: i128,
    pub total_disbursed: i128,
    pub total_repaid: i128,
    pub total_withdrawn: i128,
    pub active_loans: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorizedCaller = 3,
    InsufficientPoolBalance = 4,
    InvalidAmount = 5,
    NoActiveLoansToClose = 6,
    RepaymentNotReceived = 7,
    WithdrawalExceedsDeposit = 8,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolDeposited {
    pub depositor: Address,
    pub amount: i128,
    pub new_pool_balance: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanDisbursed {
    pub borrower: Address,
    pub loan_amount: i128,
    pub remaining_pool_balance: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanRepaymentReceived {
    pub amount: i128,
    pub new_pool_balance: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolWithdrawn {
    pub depositor: Address,
    pub amount: i128,
    pub new_pool_balance: i128,
    pub timestamp: u64,
}

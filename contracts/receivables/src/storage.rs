use soroban_sdk::{contracterror, contracttype, Address, BytesN};

pub const MIN_INVOICE_AMOUNT: i128 = 10;

pub const LTV_BPS: i128 = 7500;

pub const BPS_DENOMINATOR: i128 = 10000;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoanStatus {
    Pending,
    Active,
    Repaid,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanRecord {
    pub borrower: Address,
    pub buyer: Address,
    pub invoice_amount: i128,
    pub loan_amount: i128,
    pub status: LoanStatus,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    CapitalPool,
    Initialized,
    Loan(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReceivablesError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    EmptyInvoiceHash = 3,
    InvoiceAmountTooLow = 4,
    DuplicateInvoice = 5,
    LoanNotFound = 6,
    LoanNotActive = 7,
    RepaymentTooLow = 8,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivableSubmitted {
    pub borrower: Address,
    pub invoice_hash: BytesN<32>,
    pub invoice_amount: i128,
    pub loan_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanRepaid {
    pub borrower: Address,
    pub invoice_hash: BytesN<32>,
    pub repayment_amount: i128,
    pub timestamp: u64,
}

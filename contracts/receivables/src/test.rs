use crate::{
    receivable_contract::{ReceivablesContract, ReceivablesContractClient},
    storage::{LoanRecord, LoanStatus, ReceivablesError},
};
use capital_pool::capital_pool::{CapitalPoolContract, CapitalPoolContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, Address, BytesN, Env, IntoVal, Symbol,
};

struct Harness {
    env: Env,
    receivables: ReceivablesContractClient<'static>,
    pool: CapitalPoolContractClient<'static>,
    token: token::Client<'static>,
    token_admin: token::StellarAssetClient<'static>,
    pool_admin: Address,
}

fn deploy() -> Harness {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin_addr = Address::generate(&env);

    let stellar_asset_contract = env.register_stellar_asset_contract_v2(token_admin_addr);

    let token_id = stellar_asset_contract.address();

    let token_client = token::Client::new(&env, &token_id);
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);

    let receivables_id = env.register(ReceivablesContract, ());
    let receivables_client = ReceivablesContractClient::new(&env, &receivables_id);

    receivables_client.initialize(&pool_id);

    let pool_admin = Address::generate(&env);

    pool_client.initialize_cp(&pool_admin, &token_id, &receivables_id);

    Harness {
        env,
        receivables: receivables_client,
        pool: pool_client,
        token: token_client,
        token_admin,
        pool_admin,
    }
}

fn seed_pool(h: &Harness, amount: i128) {
    let depositor = Address::generate(&h.env);
    h.token_admin.mint(&depositor, &amount);
    h.pool.deposit(&depositor, &amount);
}

fn hash_of(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn initialize_twice_fails() {
    let h = deploy();
    let result = h.receivables.try_initialize(&h.pool.address);
    assert_eq!(result, Err(Ok(ReceivablesError::AlreadyInitialized)));
}

#[test]
fn submit_before_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let receivables_id = env.register(ReceivablesContract, ());
    let receivables_client = ReceivablesContractClient::new(&env, &receivables_id);

    let borrower = Address::generate(&env);
    let buyer = Address::generate(&env);
    let result =
        receivables_client.try_submit_receivable(&borrower, &hash_of(&env, 1), &1_000, &buyer);
    assert_eq!(result, Err(Ok(ReceivablesError::NotInitialized)));
}

#[test]
fn submit_receivable_rejects_zero_invoice_hash() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let zero_hash = BytesN::from_array(&h.env, &[0u8; 32]);

    let result = h
        .receivables
        .try_submit_receivable(&borrower, &zero_hash, &1_000, &buyer);
    assert_eq!(result, Err(Ok(ReceivablesError::EmptyInvoiceHash)));
}

#[test]
fn submit_receivable_rejects_amount_below_ten_dollar_floor() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    let result = h
        .receivables
        .try_submit_receivable(&borrower, &hash_of(&h.env, 1), &9, &buyer);
    assert_eq!(result, Err(Ok(ReceivablesError::InvoiceAmountTooLow)));
}

#[test]
fn submit_receivable_accepts_exactly_ten_dollar_floor() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    // 10 * 0.75 = 7 (integer division, no floating point).
    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash_of(&h.env, 1), &10, &buyer);
    assert_eq!(loan_amount, 7);
}

#[test]
fn submit_receivable_rejects_duplicate_invoice_hash() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 7);

    h.receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);

    let result = h
        .receivables
        .try_submit_receivable(&borrower, &hash, &1_000, &buyer);
    assert_eq!(result, Err(Ok(ReceivablesError::DuplicateInvoice)));
}

#[test]
fn submit_receivable_duplicate_check_is_per_hash_not_per_borrower() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower_a = Address::generate(&h.env);
    let borrower_b = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 9);

    h.receivables
        .submit_receivable(&borrower_a, &hash, &1_000, &buyer);

    let result = h
        .receivables
        .try_submit_receivable(&borrower_b, &hash, &1_000, &buyer);
    assert_eq!(result, Err(Ok(ReceivablesError::DuplicateInvoice)));
}

#[test]
fn submit_receivable_requires_borrower_auth() {
    let env = Env::default();
    let token_admin_addr = Address::generate(&env);
    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let receivables_id = env.register(ReceivablesContract, ());
    let receivables_client = ReceivablesContractClient::new(&env, &receivables_id);

    env.mock_all_auths();

    let stellar_asset_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_id = stellar_asset_contract.address();

    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    receivables_client.initialize(&pool_id);
    let pool_admin = Address::generate(&env);
    pool_client.initialize_cp(&pool_admin, &token_id, &receivables_id);
    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &10_000);
    pool_client.deposit(&depositor, &10_000);

    env.set_auths(&[]);
    let borrower = Address::generate(&env);
    let buyer = Address::generate(&env);
    let result =
        receivables_client.try_submit_receivable(&borrower, &hash_of(&env, 1), &1_000, &buyer);
    assert!(result.is_err());
}

#[test]
fn submit_receivable_calculates_75_percent_ltv() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    let loan_amount =
        h.receivables
            .submit_receivable(&borrower, &hash_of(&h.env, 1), &1_000, &buyer);

    assert_eq!(loan_amount, 750);
}

#[test]
fn submit_receivable_ltv_rounds_down_integer_division() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash_of(&h.env, 1), &999, &buyer);
    assert_eq!(loan_amount, 749);
}

#[test]
fn submit_receivable_actually_moves_real_tokens_via_cross_contract_call() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    assert_eq!(h.token.balance(&borrower), 0);

    let loan_amount =
        h.receivables
            .submit_receivable(&borrower, &hash_of(&h.env, 1), &4_000, &buyer);

    assert_eq!(loan_amount, 3_000);
    assert_eq!(h.token.balance(&borrower), 3_000);

    let stats = h.pool.get_pool_stats();
    assert_eq!(stats.pool_balance, 10_000 - 3_000);
    assert_eq!(stats.total_disbursed, 3_000);
    assert_eq!(stats.active_loans, 1);
}

#[test]
fn submit_receivable_records_loan_as_active_after_successful_disbursement() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 3);

    h.receivables
        .submit_receivable(&borrower, &hash, &2_000, &buyer);

    let record = h.receivables.get_loan_status(&hash);
    assert_eq!(
        record,
        LoanRecord {
            borrower: borrower.clone(),
            buyer,
            invoice_amount: 2_000,
            loan_amount: 1_500,
            status: LoanStatus::Active,
            timestamp: h.env.ledger().timestamp(),
        }
    );
}

#[test]
fn submit_receivable_fails_whole_transaction_if_pool_has_insufficient_funds() {
    let h = deploy();
    seed_pool(&h, 100);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 5);

    let result = h
        .receivables
        .try_submit_receivable(&borrower, &hash, &1_000, &buyer);
    assert!(result.is_err());

    let lookup = h.receivables.try_get_loan_status(&hash);
    assert!(lookup.is_err());

    assert_eq!(h.token.balance(&borrower), 0);
}

#[test]
fn submit_receivable_emits_receivable_submitted_event() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    h.receivables
        .submit_receivable(&borrower, &hash_of(&h.env, 1), &1_000, &buyer);

    let events = h.env.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        let topics: soroban_sdk::Vec<soroban_sdk::Val> = topics.clone();
        if topics.is_empty() {
            return false;
        }
        let t0: Symbol = topics.get(0).unwrap().into_val(&h.env);
        t0 == Symbol::new(&h.env, "submitted")
    });
    assert!(found, "expected a 'submitted' event to have been published");
}

#[test]
fn get_loan_status_unknown_hash_returns_error() {
    let h = deploy();
    let result = h.receivables.try_get_loan_status(&hash_of(&h.env, 99));
    assert_eq!(result, Err(Ok(ReceivablesError::LoanNotFound)));
}

#[test]
fn get_loan_status_requires_no_auth() {
    let h = deploy();
    seed_pool(&h, 10_000);
    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);
    h.receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);

    h.env.set_auths(&[]);
    // Should succeed with zero authorized signatures -- pure read.
    let record = h.receivables.get_loan_status(&hash);
    assert_eq!(record.status, LoanStatus::Active);
}

#[test]
fn repay_loan_marks_status_repaid() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);

    h.receivables.repay_loan(&hash, &loan_amount);

    let record = h.receivables.get_loan_status(&hash);
    assert_eq!(record.status, LoanStatus::Repaid);
}

#[test]
fn repay_loan_decrements_pool_active_loan_count() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);
    assert_eq!(h.pool.get_pool_stats().active_loans, 1);

    h.receivables.repay_loan(&hash, &loan_amount);
    assert_eq!(h.pool.get_pool_stats().active_loans, 0);
}

#[test]
fn repay_loan_does_not_return_funds_to_pool_in_poc() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);
    let balance_after_disbursement = h.pool.get_pool_stats().pool_balance;

    h.receivables.repay_loan(&hash, &loan_amount);

    assert_eq!(
        h.pool.get_pool_stats().pool_balance,
        balance_after_disbursement
    );
}

#[test]
fn repay_loan_rejects_amount_below_principal() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);

    let result = h.receivables.try_repay_loan(&hash, &(loan_amount - 1));
    assert_eq!(result, Err(Ok(ReceivablesError::RepaymentTooLow)));
}

#[test]
fn repay_loan_accepts_overpayment() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);

    h.receivables.repay_loan(&hash, &(loan_amount + 500));
    assert_eq!(
        h.receivables.get_loan_status(&hash).status,
        LoanStatus::Repaid
    );
}

#[test]
fn repay_loan_unknown_hash_fails() {
    let h = deploy();
    let result = h.receivables.try_repay_loan(&hash_of(&h.env, 42), &100);
    assert_eq!(result, Err(Ok(ReceivablesError::LoanNotFound)));
}

#[test]
fn repay_loan_twice_fails_second_time() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &1_000, &buyer);
    h.receivables.repay_loan(&hash, &loan_amount);

    let result = h.receivables.try_repay_loan(&hash, &loan_amount);
    assert_eq!(result, Err(Ok(ReceivablesError::LoanNotActive)));
}

#[test]
fn repay_loan_requires_original_borrower_auth() {
    let env = Env::default();
    let token_admin_addr = Address::generate(&env);
    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let receivables_id = env.register(ReceivablesContract, ());
    let receivables_client = ReceivablesContractClient::new(&env, &receivables_id);

    env.mock_all_auths();

    let stellar_asset_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_id = stellar_asset_contract.address();

    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    receivables_client.initialize(&pool_id);
    let pool_admin = Address::generate(&env);
    pool_client.initialize_cp(&pool_admin, &token_id, &receivables_id);
    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &10_000);
    pool_client.deposit(&depositor, &10_000);

    let borrower = Address::generate(&env);
    let impostor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let hash = hash_of(&env, 1);
    let loan_amount = receivables_client.submit_receivable(&borrower, &hash, &1_000, &buyer);

    env.set_auths(&[]);
    let _ = impostor;
    let result = receivables_client.try_repay_loan(&hash, &loan_amount);
    assert!(result.is_err());
}

#[test]
fn pool_disburse_loan_cannot_be_called_by_a_contract_other_than_receivables() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let decoy_receivables_id = h.env.register(ReceivablesContract, ());
    let decoy_client = ReceivablesContractClient::new(&h.env, &decoy_receivables_id);
    decoy_client.initialize(&h.pool.address);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    let result = decoy_client.try_submit_receivable(&borrower, &hash_of(&h.env, 1), &1_000, &buyer);
    assert!(
        result.is_err(),
        "a second/decoy Receivables contract must not be able to trigger disbursement"
    );

    // Pool must be untouched.
    let stats = h.pool.get_pool_stats();
    assert_eq!(stats.pool_balance, 10_000);
    assert_eq!(stats.active_loans, 0);

    let _ = h.pool_admin;
}

#[test]
fn full_loan_cycle_submit_disburse_repay() {
    let h = deploy();
    seed_pool(&h, 10_000);

    let borrower = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);
    let hash = hash_of(&h.env, 1);

    let loan_amount = h
        .receivables
        .submit_receivable(&borrower, &hash, &5_000, &buyer);
    assert_eq!(loan_amount, 3_750);
    assert_eq!(h.token.balance(&borrower), 3_750);
    assert_eq!(
        h.receivables.get_loan_status(&hash).status,
        LoanStatus::Active
    );
    assert_eq!(h.pool.get_pool_stats().active_loans, 1);

    h.receivables.repay_loan(&hash, &loan_amount);
    assert_eq!(
        h.receivables.get_loan_status(&hash).status,
        LoanStatus::Repaid
    );
    assert_eq!(h.pool.get_pool_stats().active_loans, 0);

    let stats = h.pool.get_pool_stats();
    assert_eq!(stats.total_deposited, 10_000);
    assert_eq!(stats.total_disbursed, 3_750);
    assert_eq!(stats.pool_balance, 10_000 - 3_750);
}

#[test]
fn multiple_borrowers_independent_loan_cycles() {
    let h = deploy();
    seed_pool(&h, 20_000);

    let alice = Address::generate(&h.env);
    let bob = Address::generate(&h.env);
    let buyer = Address::generate(&h.env);

    let alice_hash = hash_of(&h.env, 1);
    let bob_hash = hash_of(&h.env, 2);

    let alice_loan = h
        .receivables
        .submit_receivable(&alice, &alice_hash, &4_000, &buyer);
    let bob_loan = h
        .receivables
        .submit_receivable(&bob, &bob_hash, &8_000, &buyer);

    assert_eq!(alice_loan, 3_000);
    assert_eq!(bob_loan, 6_000);
    assert_eq!(h.pool.get_pool_stats().active_loans, 2);

    h.receivables.repay_loan(&alice_hash, &alice_loan);
    assert_eq!(h.pool.get_pool_stats().active_loans, 1);
    assert_eq!(
        h.receivables.get_loan_status(&bob_hash).status,
        LoanStatus::Active
    );

    h.receivables.repay_loan(&bob_hash, &bob_loan);
    assert_eq!(h.pool.get_pool_stats().active_loans, 0);
}

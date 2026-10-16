use crate::capital_pool::{CapitalPoolContract, CapitalPoolContractClient};
use crate::storage::{PoolError, PoolStats};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, Symbol};

struct TestSetup {
    env: Env,
    pool_client: CapitalPoolContractClient<'static>,
    token_client: token::Client<'static>,
    token_admin: token::StellarAssetClient<'static>,
    admin: Address,
    receivables: Address,
}

fn setup() -> TestSetup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let receivables = Address::generate(&env);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);

    let token_admin_addr = Address::generate(&env);

    let stellar_asset_contract = env.register_stellar_asset_contract_v2(token_admin_addr);

    let token_id = stellar_asset_contract.address();

    let token_client = token::Client::new(&env, &token_id);
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    TestSetup {
        env,
        pool_client,
        token_client,
        token_admin,
        admin,
        receivables,
    }
}

fn init_pool(setup: &TestSetup) {
    setup.pool_client.initialize_cp(
        &setup.admin,
        &setup.token_client.address,
        &setup.receivables,
    );
}

#[test]
fn initialize_sets_all_fields_and_zeroes_stats() {
    let s = setup();
    init_pool(&s);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(
        stats,
        PoolStats {
            total_deposited: 0,
            pool_balance: 0,
            total_disbursed: 0,
            active_loans: 0,
        }
    );
    assert_eq!(s.pool_client.get_receivables_contract(), s.receivables);
    assert_eq!(s.pool_client.get_token(), s.token_client.address);
}

#[test]
fn initialize_twice_fails() {
    let s = setup();
    init_pool(&s);

    let result = s
        .pool_client
        .try_initialize_cp(&s.admin, &s.token_client.address, &s.receivables);
    assert_eq!(result, Err(Ok(PoolError::AlreadyInitialized)));
}

#[test]
fn initialize_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let receivables = Address::generate(&env);
    let token_admin_addr = Address::generate(&env);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let token_id = env.register_stellar_asset_contract_v2(token_admin_addr);

    let result = pool_client.try_initialize_cp(&admin, &token_id.address(), &receivables);
    assert!(result.is_err());
}

#[test]
fn deposit_transfers_tokens_and_updates_balance() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &10_000);

    let new_balance = s.pool_client.deposit(&depositor, &4_000);

    assert_eq!(new_balance, 4_000);
    assert_eq!(s.token_client.balance(&depositor), 6_000);
    assert_eq!(s.token_client.balance(&s.pool_client.address), 4_000);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 4_000);
    assert_eq!(stats.total_deposited, 4_000);
}

#[test]
fn multiple_deposits_accumulate() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &10_000);

    s.pool_client.deposit(&depositor, &1_000);
    s.pool_client.deposit(&depositor, &2_500);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 3_500);
    assert_eq!(stats.total_deposited, 3_500);
}

#[test]
fn deposit_seed_amount_from_brief_10000() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &10_000);
    let balance = s.pool_client.deposit(&depositor, &10_000);

    assert_eq!(balance, 10_000);
}

#[test]
fn deposit_zero_amount_rejected() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    let result = s.pool_client.try_deposit(&depositor, &0);
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
}

#[test]
fn deposit_negative_amount_rejected() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    let result = s.pool_client.try_deposit(&depositor, &-500);
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
}

#[test]
fn deposit_before_initialize_fails() {
    let s = setup();
    let depositor = Address::generate(&s.env);
    let result = s.pool_client.try_deposit(&depositor, &100);
    assert_eq!(result, Err(Ok(PoolError::NotInitialized)));
}

#[test]
fn deposit_emits_pool_deposited_event() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &5_000);
    s.pool_client.deposit(&depositor, &1_500);

    let events = s.env.events().all();
    let last = events.last().unwrap();
    let topics: soroban_sdk::Vec<soroban_sdk::Val> = last.1.clone();
    let topic0: Symbol = topics.get(0).unwrap().into_val(&s.env);
    assert_eq!(topic0, Symbol::new(&s.env, "deposited"));
}

#[test]
fn deposit_without_depositor_auth_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let receivables = Address::generate(&env);
    let token_admin_addr = Address::generate(&env);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let token_id = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_admin = token::StellarAssetClient::new(&env, &token_id.address());

    env.mock_all_auths();
    pool_client.initialize_cp(&admin, &token_id.address(), &receivables);
    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &1_000);

    env.set_auths(&[]);
    let result = pool_client.try_deposit(&depositor, &500);
    assert!(result.is_err());
}

#[test]
fn disburse_loan_by_receivables_contract_succeeds() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &10_000);
    s.pool_client.deposit(&depositor, &10_000);

    let borrower = Address::generate(&s.env);

    s.pool_client.disburse_loan(&borrower, &7_500);

    assert_eq!(s.token_client.balance(&borrower), 7_500);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 2_500);
    assert_eq!(stats.total_disbursed, 7_500);
    assert_eq!(stats.active_loans, 1);
}

#[test]
fn disburse_loan_rejects_when_no_auth_present() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let receivables = Address::generate(&env);
    let token_admin_addr = Address::generate(&env);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let token_id = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_admin = token::StellarAssetClient::new(&env, &token_id.address());

    env.mock_all_auths();
    pool_client.initialize_cp(&admin, &token_id.address(), &receivables);
    let depositor = Address::generate(&env);
    token_admin.mint(&depositor, &10_000);
    pool_client.deposit(&depositor, &10_000);

    env.set_auths(&[]);
    let borrower = Address::generate(&env);
    let result = pool_client.try_disburse_loan(&borrower, &1_000);
    assert!(
        result.is_err(),
        "disburse_loan must fail without receivables contract auth"
    );

    let stats = pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 10_000);
    assert_eq!(stats.active_loans, 0);
}

#[test]
fn disburse_loan_insufficient_balance_rejected() {
    let s = setup();
    init_pool(&s);

    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &1_000);
    s.pool_client.deposit(&depositor, &1_000);

    let borrower = Address::generate(&s.env);
    let result = s.pool_client.try_disburse_loan(&borrower, &5_000);
    assert_eq!(result, Err(Ok(PoolError::InsufficientPoolBalance)));

    assert_eq!(s.token_client.balance(&borrower), 0);
    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 1_000);
}

#[test]
fn disburse_loan_zero_or_negative_amount_rejected() {
    let s = setup();
    init_pool(&s);
    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &1_000);
    s.pool_client.deposit(&depositor, &1_000);

    let borrower = Address::generate(&s.env);
    assert_eq!(
        s.pool_client.try_disburse_loan(&borrower, &0),
        Err(Ok(PoolError::InvalidAmount))
    );
    assert_eq!(
        s.pool_client.try_disburse_loan(&borrower, &-100),
        Err(Ok(PoolError::InvalidAmount))
    );
}

#[test]
fn disburse_loan_exact_full_balance_succeeds() {
    let s = setup();
    init_pool(&s);
    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &5_000);
    s.pool_client.deposit(&depositor, &5_000);

    let borrower = Address::generate(&s.env);
    s.pool_client.disburse_loan(&borrower, &5_000);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 0);
    assert_eq!(stats.active_loans, 1);
}

#[test]
fn multiple_disbursements_accumulate_active_loans_and_totals() {
    let s = setup();
    init_pool(&s);
    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &10_000);
    s.pool_client.deposit(&depositor, &10_000);

    let b1 = Address::generate(&s.env);
    let b2 = Address::generate(&s.env);
    s.pool_client.disburse_loan(&b1, &3_000);
    s.pool_client.disburse_loan(&b2, &2_000);

    let stats = s.pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 5_000);
    assert_eq!(stats.total_disbursed, 5_000);
    assert_eq!(stats.active_loans, 2);
}

#[test]
fn close_loan_decrements_active_count() {
    let s = setup();
    init_pool(&s);
    let depositor = Address::generate(&s.env);
    s.token_admin.mint(&depositor, &5_000);
    s.pool_client.deposit(&depositor, &5_000);

    let borrower = Address::generate(&s.env);
    s.pool_client.disburse_loan(&borrower, &2_000);
    assert_eq!(s.pool_client.get_pool_stats().active_loans, 1);

    s.pool_client.close_loan();
    assert_eq!(s.pool_client.get_pool_stats().active_loans, 0);
}

#[test]
fn close_loan_with_no_active_loans_rejected() {
    let s = setup();
    init_pool(&s);
    let result = s.pool_client.try_close_loan();
    assert_eq!(result, Err(Ok(PoolError::NoActiveLoansToClose)));
}

#[test]
fn get_pool_stats_requires_no_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let receivables = Address::generate(&env);
    let token_admin_addr = Address::generate(&env);

    let pool_id = env.register(CapitalPoolContract, ());
    let pool_client = CapitalPoolContractClient::new(&env, &pool_id);
    let token_id = env.register_stellar_asset_contract_v2(token_admin_addr);

    env.mock_all_auths();
    pool_client.initialize_cp(&admin, &token_id.address(), &receivables);

    env.set_auths(&[]);
    let stats = pool_client.get_pool_stats();
    assert_eq!(stats.pool_balance, 0);
}

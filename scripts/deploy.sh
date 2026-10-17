#!/usr/bin/env bash
set -euo pipefail

NETWORK="testnet"
IDENTITY="${STELLAR_IDENTITY:-josh}"
OUT_FILE="deploy-output.env"

echo "== Sanwo Finance POC — Testnet Deploy =="
echo "Network:  $NETWORK"
echo "Identity: $IDENTITY"
echo ""

command -v stellar >/dev/null || {
  echo "ERROR: stellar-cli not found"
  exit 1
}

stellar keys address "$IDENTITY" >/dev/null 2>&1 || {
  echo "Creating identity..."
  stellar keys generate "$IDENTITY" --network "$NETWORK" --fund
}

DEPLOYER=$(stellar keys address "$IDENTITY")
echo "Deployer address: $DEPLOYER"
echo ""

echo "== Step 0: Build contracts =="
cargo build --target wasm32v1-none --release --workspace

POOL_WASM="target/wasm32v1-none/release/capital_pool.wasm"
RECEIVABLES_WASM="target/wasm32v1-none/release/receivables.wasm"

echo ""
echo "== Step 1: Use existing SAC(native XLM) =="
TOKEN_ID=$(stellar contract id asset \
  --asset "native" \
  --network "$NETWORK")

echo "XLM SAC Token ID: $TOKEN_ID"

echo ""
echo "== Step 2: Deploy Capital Pool =="
POOL_ID=$(stellar contract deploy \
  --wasm "$POOL_WASM" \
  --source-account "$IDENTITY" \
  --network "$NETWORK")

echo "Capital Pool: $POOL_ID"

echo ""
echo "== Step 3: Deploy Receivables =="
RECEIVABLES_ID=$(stellar contract deploy \
  --wasm "$RECEIVABLES_WASM" \
  --source-account "$IDENTITY" \
  --network "$NETWORK")

echo "Receivables: $RECEIVABLES_ID"

echo ""
echo "== Step 4: Initialize Receivables =="
stellar contract invoke \
  --id "$RECEIVABLES_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  -- initialize \
  --capital_pool "$POOL_ID"

echo ""
echo "== Step 5: Initialize Capital Pool =="
stellar contract invoke \
  --id "$POOL_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  -- initialize_cp \
  --admin "$DEPLOYER" \
  --token "$TOKEN_ID" \
  --receivables_contract "$RECEIVABLES_ID"

echo ""
echo "== Step 6: Seed Pool (NO MINTING) =="

DEPOSIT_TX=$(stellar contract invoke \
  --id "$POOL_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  -- deposit \
  --depositor "$DEPLOYER" \
  --amount 10000000000)

echo "Deposit tx: $DEPOSIT_TX"

echo ""
echo "== Step 7: Test Borrower Flow =="

stellar keys generate "sanwo-test-borrower" --network "$NETWORK" --fund 2>/dev/null || true
BORROWER=$(stellar keys address "sanwo-test-borrower")

echo "Borrower: $BORROWER"

INVOICE_HASH="00000000000000000000000000000000000000000000000000000000000000aa"
BUYER="$DEPLOYER"

SUBMIT_TX=$(stellar contract invoke \
  --id "$RECEIVABLES_ID" \
  --source-account "sanwo-test-borrower" \
  --network "$NETWORK" \
  --send=yes \
  -- submit_receivable \
  --borrower "$BORROWER" \
  --invoice_hash "$INVOICE_HASH" \
  --invoice_amount 1000000000 \
  --buyer "$BUYER")

echo "Submit tx: $SUBMIT_TX"

REPAY_TX=$(stellar contract invoke \
  --id "$RECEIVABLES_ID" \
  --source-account "sanwo-test-borrower" \
  --network "$NETWORK" \
  --send=yes \
  -- repay_loan \
  --invoice_hash "$INVOICE_HASH" \
  --repayment_amount 750000000)

echo "Repay tx: $REPAY_TX"

echo ""
echo "== Step 8: Withdraw part of the seed deposit =="

WITHDRAW_TX=$(stellar contract invoke \
  --id "$POOL_ID" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  --send=yes \
  -- withdraw \
  --depositor "$DEPLOYER" \
  --amount 500000000)

echo "Withdraw tx: $WITHDRAW_TX"

echo ""
echo "== Pool stats =="
stellar contract invoke --id "$POOL_ID" --source-account "$IDENTITY" --network "$NETWORK" -- get_pool_stats

echo ""
echo "== Writing output =="
cat >"$OUT_FILE" <<EOF
NETWORK=$NETWORK
TOKEN_SAC_NATIVE=$TOKEN_ID
POOL=$POOL_ID
RECEIVABLES=$RECEIVABLES_ID
DEPLOYER=$DEPLOYER
BORROWER=$BORROWER
SUBMIT_TX=$SUBMIT_TX
DEPOSIT_TX=$DEPOSIT_TX
REPAY_TX=$REPAY_TX
WITHDRAW_TX=$WITHDRAW_TX
EOF

cat "$OUT_FILE"

echo ""
echo "Done ✔"

IDENTITY=josh

POOL_ID=CAUK6D7JVPNRWSPPGSUXF4QGFUWK6KB3P6DPMHFZIJ4CSK7AHZRBOSNG
RECEIVABLES_ID=CB6HBD22R2LJ6MF5NEGO67Z7RVNI7JOQAP6MO6QKBEYYQTXJRJECMB54
TOKEN_ID=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
INVOICE_HASH="0000000000000000000000000000000000000000000000000000000000aa"

.PHONY: init init_cp

# ---------------------------
# Initialize Receivables
# ---------------------------
init:
	stellar contract invoke \
		--id "$(RECEIVABLES_ID)" \
		--source-account $(IDENTITY) \
		--network testnet \
		-- initialize \
		--capital_pool "$(POOL_ID)"

# ---------------------------
# Initialize Capital Pool
# ---------------------------
init_cp:
	stellar contract invoke \
		--id "$(POOL_ID)" \
		--source-account $(IDENTITY) \
		--network testnet \
		-- initialize_cp \
		--admin $(shell stellar keys address $(IDENTITY)) \
		--token "$(TOKEN_ID)" \
		--receivables_contract "$(RECEIVABLES_ID)"

get_loan_status:
	stellar contract invoke \
		--id "$(RECEIVABLES_ID)" \
		--source-account $(IDENTITY) \
		--network testnet \
		-- get_loan_status \
		--invoice_hash $(INVOICE_HASH) \

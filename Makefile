IDENTITY=josh

POOL_ID=CBIDZFVYXVAFNJUDKZAB43JIY23BQMO56PKO6HUYI43S4XU2HNHTTVF2
RECEIVABLES_ID=CC7YS73X7J6AIAVF6HSI62NH4AVHEEBMSQBBQUCCFMIQTRJS7V6FOVYO
TOKEN_ID=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
INVOICE_HASH="00000000000000000000000000000000000000000000000000000000000000aa"

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

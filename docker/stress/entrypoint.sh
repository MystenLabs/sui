#!/bin/bash

export RUST_BACKTRACE=1

sleep "${STARTUP_DELAY_SECONDS:-10}"

/usr/local/bin/stress \
    --staggered-start-max-multiplier "${STRESS_STAGGERED_START_MAX_MULTIPLIER:-0}" \
    --fullnode-rpc-addresses "${FULLNODE_RPC_ADDRESS}" \
    --use-fullnode-for-reconfig "${USE_FULLNODE_FOR_RECONFIG}" \
    --num-client-threads 24 \
    --num-server-threads 1 \
    --num-transfer-accounts 2 \
    --local false \
    --primary-gas-owner-id "${PRIMARY_GAS_OWNER}" \
    --genesis-blob-path ${GENESIS_BLOB_PATH} \
    --keystore-path ${KEYSTORE_PATH} \
    bench \
    --target-qps "${STRESS_TARGET_QPS}" \
    --in-flight-ratio 30 \
    --shared-counter "${STRESS_SHARED_COUNTER}" \
    --transfer-object "${STRESS_TRANSFER_OBJECT}" \
    --delegation "${STRESS_DELEGATION}" \
    --batch-payment "${BATCH_PAYMENT}" \
    --batch-payment-size "${BATCH_PAYMENT_SIZE}" \
    --adversarial "${STRESS_ADVERSARIAL}" \
    --client-metric-host 0.0.0.0 \
    --num-workers 24

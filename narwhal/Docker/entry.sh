#!/bin/bash

# Capture stack trace
export RUST_BACKTRACE=1

if [ -z "$VALIDATOR_ID" -a "$KUBERNETES_PORT" ]; then
    export VALIDATOR_ID=${HOSTNAME##*-}
    # assuming that WORKER_ID isn't also set.
    # currently they match the validator it is assigned to.
    export WORKER_ID=${WORKER_ID:=$VALIDATOR_ID}
fi


# Environment variables to use on the script
NODE_BIN="./bin/node"
KEYS_PATH=${KEYS_PATH:="/validators/validator-$VALIDATOR_ID/key.json"}
COMMITTEE_PATH=${COMMITTEE_PATH:="/validators/committee.json"}
PARAMETERS_PATH=${PARAMETERS_PATH:="/validators/parameters.json"}
DATA_PATH=${DATA_PATH:="/validators"}

if [[ "$CLEANUP_DISABLED" = "true" ]]; then
  echo "Will not clean up existing directories..."
else
  if [[ "$NODE_TYPE" = "primary" ]]; then
    # Clean up only the primary node's data
    rm -r "${DATA_PATH}/validator-$VALIDATOR_ID/db-primary"
  elif [[ "$NODE_TYPE" = "worker" ]]; then
    # Clean up only the specific worker's node data
    rm -r "${DATA_PATH}/validator-$VALIDATOR_ID/db-worker-${WORKER_ID}"
  fi
fi

# If this is a primary node, then run as primary
if [[ "$NODE_TYPE" = "primary" ]]; then
  echo "Bootstrapping primary node"

  $NODE_BIN $LOG_LEVEL run \
  --keys $KEYS_PATH \
  --committee $COMMITTEE_PATH \
  --store "${DATA_PATH}/validator-$VALIDATOR_ID/db-primary" \
  --parameters $PARAMETERS_PATH \
  primary $CONSENSUS_DISABLED
elif [[ "$NODE_TYPE" = "worker" ]]; then
  echo "Bootstrapping new worker node with id $WORKER_ID"

  $NODE_BIN $LOG_LEVEL run \
  --keys $KEYS_PATH \
  --committee $COMMITTEE_PATH \
  --store "${DATA_PATH}/validator-$VALIDATOR_ID/db-worker-$WORKER_ID" \
  --parameters $PARAMETERS_PATH \
  worker --id $WORKER_ID
else
  echo "Unknown provided value for parameter: NODE_TYPE=$NODE_TYPE"
  exit 1
fi

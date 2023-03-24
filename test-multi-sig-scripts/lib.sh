#!/bin/bash
#
source constants.sh

### Create multi-sig
create_musig() {
  musig_addr=$($sui keytool multi-sig-address \
    --pks $PK_1 $PK_2 $PK_3 \
    --weights 1 1 1 \
    --threshold 2 \
    | grep MultiSig \
    | sed 's/.*\://' \
    | xargs)

  object_id=$(curl \
    --location \
    --silent \
    --request POST 'http://127.0.0.1:9123/gas' \
    --header 'Content-Type: application/json' \
    --data-raw "{ \"FixedAmountRequest\": { \"recipient\": \"$musig_addr\" } }" \
    | jq '.transferredGasObjects[0].id' \
    | cut -d'"' -f 2)

  echo "multisig-account address: $musig_addr"
  echo "object_id: $object_id"
}


### Multi-Sig Publish
publish_move_module() {

  gas_object="${object_id}"


  serialized_tx=$($sui client serialize-publish "${module_path}" --gas $gas_object --gas-budget $gas_budget | grep execute | sed 's/.*\://' | xargs)

  sigs_1="$($sui keytool sign --address $addr_1 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"

  sigs_2="$($sui keytool sign --address $addr_2 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"


  serialized_musig="$($sui keytool multi-sig-combine-partial-sig --pks $PK_1 $PK_2 $PK_3 --weights 1 1 1 --threshold 2 --sigs $sigs_1 $sigs_2 | grep serialized | cut -d'"' -f 2)"


  $sui client execute-signed-tx --tx-bytes $serialized_tx --signatures $serialized_musig
}



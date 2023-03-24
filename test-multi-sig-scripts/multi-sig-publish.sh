
sui="./../target/debug/sui"
module_path="../sui_programmability/examples/move_tutorial"

addr_1="0xb31415151b86bd811643b5aeedc2c930b1ce592f23f87c9013b1b66ab8c8285b"
addr_2="0xb6df80c6eebfda99f3172e0345667e8148e4665a5b3c5f41b34879393dc19eae"
addr_3="0xbd5313093f2b8662c1f262c22b48d530ec965be0397d415763b41fd75fb2d9b8"

PK_1="AJQwyQMKQ7gLQw+KVbNh4pbr0473XV7Ec/j/Ljvggj3U"
PK_2="AOJbaGb622hGZlwJZ5SAh2rnr1WnR1TkhzIOMnya0QFm"
PK_3="AI+TXXrZDfq8vG24cNyayJHaizYN4KxYHxpwiJhYdqxK"

gas_object="0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e"
gas_budget="30000"


serialized_tx=$($sui client serialize-publish "${module_path}" --gas $gas_object --gas-budget $gas_budget | grep execute | sed 's/.*\://' | xargs)

sigs_1="$($sui keytool sign --address $addr_1 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"

sigs_2="$($sui keytool sign --address $addr_2 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"


serialized_musig="$($sui keytool multi-sig-combine-partial-sig --pks $PK_1 $PK_2 $PK_3 --weights 1 1 1 --threshold 2 --sigs $sigs_1 $sigs_2 | grep serialized | cut -d'"' -f 2)"


$sui client execute-signed-tx --tx-bytes $serialized_tx --signatures $serialized_musig



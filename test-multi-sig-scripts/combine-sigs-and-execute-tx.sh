
###
#{"transferredGasObjects":[{"amount":200000000,"id":"0x1eb5b0470d04b19bc967d60bc7b219bf26a21b9a19bbce99098c99cb9f6ad749","transfertxdigest":"fqhet7cmhggdnnqapu8t18ed7pbxursauaatgi9fjspy"},{"amount":200000000,"id":"0x5e4c1768b659fa86d9600a6cea3917ef4a1be23dd60a03a07e409cce8922b1cc","transfertxdigest":"fqhet7cmhggdnnqapu8t18ed7pbxursauaatgi9fjspy"},{"amount":200000000,"id":"0x63fdd3e82a117456b9c0a68ac838e8a9116e9ccc71cbcfd46ca5b62b7f8c6658","transfertxdigest":"fqhet7cmhggdnnqapu8t18ed7pbxursauaatgi9fjspy"},{"amount":200000000,"id":"0x8eacf1aace6bb59f96e09d3355eb6a42226d5d56fcc27ce6d81df139bf69fddb","transfertxdigest":"fqhet7cmhggdnnqapu8t18ed7pbxursauaatgi9fjspy"},{"amount":200000000,"id":"0x8f3c1e568cf1ccc527a24c29206350bc4e20642d46ea24a6d1b906aaa404354b","transfertxdigest":"fqhet7cmhggdnnqapu8t18ed7pbxursauaatgi9fjspy"}],"error":null}%
#
#

sui="./../target/debug/sui"


send_to="0xb31415151b86bd811643b5aeedc2c930b1ce592f23f87c9013b1b66ab8c8285b"
object_id="0x1eb5b0470d04b19bc967d60bc7b219bf26a21b9a19bbce99098c99cb9f6ad749"
#
addr_1="0xb31415151b86bd811643b5aeedc2c930b1ce592f23f87c9013b1b66ab8c8285b"
addr_2="0xb6df80c6eebfda99f3172e0345667e8148e4665a5b3c5f41b34879393dc19eae"
addr_3="0xbd5313093f2b8662c1f262c22b48d530ec965be0397d415763b41fd75fb2d9b8"

PK_1="AJQwyQMKQ7gLQw+KVbNh4pbr0473XV7Ec/j/Ljvggj3U"
PK_2="AOJbaGb622hGZlwJZ5SAh2rnr1WnR1TkhzIOMnya0QFm"
PK_3="AI+TXXrZDfq8vG24cNyayJHaizYN4KxYHxpwiJhYdqxK"
set -x

serialized_tx="$($sui client serialize-transfer-sui --to $send_to  --sui-coin-object-id $object_id --gas-budget 1000 | sed 's/.*\://' | xargs)"

sigs_1="$($sui keytool sign --address $addr_1 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"

sigs_2="$($sui keytool sign --address $addr_2 --data $serialized_tx | grep Serialized | cut -d'"' -f 2)"


SERIALIZED_MUSIG="$($sui keytool multi-sig-combine-partial-sig --pks $PK_1 $PK_2 $PK_3 --weights 1 1 1 --threshold 3 --sigs $sigs_1 $sigs_2 | grep serialized | cut -d'"' -f 2)"


$sui client execute-signed-tx --tx-bytes $serialized_tx --signatures $SERIALIZED_MUSIG




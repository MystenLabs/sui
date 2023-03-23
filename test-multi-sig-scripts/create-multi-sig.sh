sui="./../target/debug/sui"

PK_1="AJQwyQMKQ7gLQw+KVbNh4pbr0473XV7Ec/j/Ljvggj3U"
PK_2="AOJbaGb622hGZlwJZ5SAh2rnr1WnR1TkhzIOMnya0QFm"
PK_3="AI+TXXrZDfq8vG24cNyayJHaizYN4KxYHxpwiJhYdqxK"


musig_addr=$($sui keytool multi-sig-address --pks $PK_1 $PK_2 $PK_3 --weights 1 1 1 --threshold 2 | grep MultiSig | sed 's/.*\://' | xargs)



echo "multisig addr: $musig_addr"
curl --location --request POST 'http://127.0.0.1:9123/gas' --header 'Content-Type: application/json' --data-raw "{ \"FixedAmountRequest\": { \"recipient\": \"$musig_addr\" } }"



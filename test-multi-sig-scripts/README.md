# Testing Out Publishing Move Modules with Multiple Signature Accounts

Publish Move Modules with a MultiSignature Sui Account

## Setup Details

### Private Key Initialization

Requires Setting up Three Keys.

Use private keys in `sui.keystore` in directory for 'plug + play'

Otherwise, customize `constants.sh` with your Address and Base64 Encoded Public Keys. Use `sui keytool list` for outputting key information


## Running Test Setup

Pre-Reqs:
- Run the Sui Validator in the background `RUST_LOG="consensus=off" cargo run --bin sui-test-validator`
- Ensure Sui Client is set for [local](https://docs.sui.io/devnet/build/sui-local-network)


### Example Output

```
multisig-account address: 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694
object_id: 0x144e89a316d394106c5a3d6d92546895c93c3b8b2f8d5e87aa82246b59dfd4d9
----- Transaction Digest ----
Fe96qN5qtoCHZDpos1ReyFoQDqmyimoWwzEAqYAwNLJr
----- Transaction Data ----
Transaction Signature: [MultiSig(MultiSig { sigs: [Ed25519(BytesRepresentation([23, 142, 20, 2, 248, 48, 71, 36, 117, 147, 96, 49, 121, 97, 147, 98, 209, 83, 89, 38, 204, 165, 191, 59, 141, 71, 240, 56, 115, 238, 14, 244, 46, 16, 230, 221, 44, 13, 120, 205, 222, 54, 73, 238, 114, 219, 196, 46, 206, 86, 132, 92, 207, 118, 15, 204, 77, 78, 217, 83, 205, 31, 12, 14])), Ed25519(BytesRepresentation([81, 58, 144, 148, 2, 155, 197, 133, 149, 34, 130, 142, 126, 112, 45, 176, 154, 235, 29, 149, 153, 160, 248, 142, 161, 83, 148, 208, 204, 204, 93, 107, 173, 223, 254, 248, 230, 63, 248, 43, 78, 201, 251, 229, 70, 28, 123, 182, 108, 199, 62, 76, 106, 67, 8, 92, 103, 250, 224, 138, 106, 134, 136, 13]))], bitmap: RoaringBitmap<[0, 1]>, multisig_pk: MultiSigPublicKey { pk_map: [("AJQwyQMKQ7gLQw+KVbNh4pbr0473XV7Ec/j/Ljvggj3U", 1), ("AOJbaGb622hGZlwJZ5SAh2rnr1WnR1TkhzIOMnya0QFm", 1), ("AI+TXXrZDfq8vG24cNyayJHaizYN4KxYHxpwiJhYdqxK", 1)], threshold: 2 }, bytes: OnceCell(Uninit) })]
Transaction Kind : Programmable
Inputs: [Pure(SuiPureValue { value_type: Some(Address), value: "0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694" })]
Commands: [
  Publish(_,,0x00000000000000000000000000000000000000000000000000000000000000010x0000000000000000000000000000000000000000000000000000000000000002),
  TransferObjects([Result(0)],Input(0)),
]

Sender: 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694
Gas Payment: Object ID: 0x144e89a316d394106c5a3d6d92546895c93c3b8b2f8d5e87aa82246b59dfd4d9, version: 0x2, digest: EZRMZKG8suYVXjzbVevXU42pfaP9dwewNZUbv5WBxHpg 
Gas Owner: 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694
Gas Price: 1
Gas Budget: 30000

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x7024329b6e2da101b2b180eabd0ff08bd9f98f948931ccf6237cdcda55d80614 , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )
  - ID: 0xc39380b02475bb16084bb4b2e75f86d7776c027d8c383bca5a09cf324b06d242 , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )
  - ID: 0xd714c617da460f7d87c73824e35c2052a9c4d26bd3a9dc7f309b1346109e3562 , Owner: Immutable
Mutated Objects:
  - ID: 0x144e89a316d394106c5a3d6d92546895c93c3b8b2f8d5e87aa82246b59dfd4d9 , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )

----- Events ----
Array []
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("0x144e89a316d394106c5a3d6d92546895c93c3b8b2f8d5e87aa82246b59dfd4d9"),
        "version": Number(3),
        "previousVersion": Number(2),
        "digest": String("8vW8ky8Daczh1aszbgLxK2m5WTATvSt8Jfcy8oCcZzMM"),
    },
    Object {
        "type": String("created"),
        "sender": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "objectType": String("0x2::package::UpgradeCap"),
        "objectId": String("0x7024329b6e2da101b2b180eabd0ff08bd9f98f948931ccf6237cdcda55d80614"),
        "version": Number(3),
        "digest": String("2UrGu8dy7oDaGjhEHbiMtmkUTh98bYeDmLrDFi9MgNG7"),
    },
    Object {
        "type": String("created"),
        "sender": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "objectType": String("0xd714c617da460f7d87c73824e35c2052a9c4d26bd3a9dc7f309b1346109e3562::my_module::Forge"),
        "objectId": String("0xc39380b02475bb16084bb4b2e75f86d7776c027d8c383bca5a09cf324b06d242"),
        "version": Number(3),
        "digest": String("2dpBN7S4U4aqCXTFbp6SViDpHV3nY838otf1FjbP6CUY"),
    },
    Object {
        "type": String("published"),
        "packageId": String("0xd714c617da460f7d87c73824e35c2052a9c4d26bd3a9dc7f309b1346109e3562"),
        "version": Number(1),
        "digest": String("DZHMiScib9daws9HrujAAEXpiNkBXdA1EemVMtBngnVz"),
        "modules": Array [
            String("my_module"),
        ],
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-1121"),
    },
]
```


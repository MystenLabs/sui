## List Objects Owned by MultiSig Account
```
➜  sui git:(jnaulty/test-multi-sig-publish) ✗ cargo run --bin sui client objects $musig_addr
    Finished dev [unoptimized + debuginfo] target(s) in 1.30s
     Running `target/debug/sui client objects 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694`
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e |     2      | HpN9cMSLW185fuyCljpEDigBb5EnmAO1D5DxW8hua54= |  AddressOwner   |          Some(Struct(GasCoin))
 0x7bcae42189828845f974e2a3712feebf5c1c5d1a0ceddfca1474542cf8a997dc |     2      | ipPFJD/V4zdIIYemRyqEl12y3bqwIcOuklfYUEanVw8= |  AddressOwner   |          Some(Struct(GasCoin))
 0xda448dc86cfa5b892a2ca2a855ff018d4e06f5ce3a729c9a8f9460c982af81c4 |     2      | K/KNlneeDbqzDtEryStM13+9GW1iIf/hDKMGI010QXk= |  AddressOwner   |          Some(Struct(GasCoin))
 0xe34545bc13b834d0dd49cbc833c4b0d10136ffb9f06d1ee6e0bbb18443e01e52 |     2      | UUk+0eZy/vsRHYD2Mx+lc6HU5Rygw206XBRQJIL9NQA= |  AddressOwner   |          Some(Struct(GasCoin))
 0xf2f0a15820e91771633fdc9233b7e75a393e44d604a555c00dce2224b6cd3a43 |     2      | QVKKK0edUsHs0A2+fCkGmsxApUiycmlj9PQYQJwc5S8= |  AddressOwner   |          Some(Struct(GasCoin))
Showing 5 results.
➜  sui git:(jnaulty/test-multi-sig-publish) ✗ gascoin_objct="0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e"
➜  sui git:(jnaulty/test-multi-sig-publish) ✗ cargo run --bin sui client serialize-publish sui_programmability/examples/move_tutorial --gas $gascoin_objct --gas-budget 30000
    Finished dev [unoptimized + debuginfo] target(s) in 1.26s
     Running `target/debug/sui client serialize-publish sui_programmability/examples/move_tutorial --gas 0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e --gas-budget 30000`
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING MyFirstPackage
Successfully verified dependencies on-chain against source.
Raw tx_bytes to execute: AAABACBYnkpfZH83aSZpy503ZFSf6a198AHUlBGYeUt7u6p2lAIEAfMDoRzrCwYAAAAKAQAIAggQAxgvBEcEBUssB3eNAQiEAkAKxAISDNYCag3AAwYABwEJAQ8BEAABDAAAAAgAAQMEAAMCAgAABQABAAAGAgMAAAwCAwAADgQDAAANBQEAAQgABgACCgoBAQwCDwoBAQgDCwcIAAcJBgsBBwgDAAEGCAABAwEGCAEFBwgBAwMFBwgDAQgCAQYIAwEFAQgBAgkABQEIAAVGb3JnZQVTd29yZAlUeENvbnRleHQDVUlEAmlkBGluaXQFbWFnaWMJbXlfbW9kdWxlA25ldwZvYmplY3QPcHVibGljX3RyYW5zZmVyBnNlbmRlcghzdHJlbmd0aAxzd29yZF9jcmVhdGUOc3dvcmRzX2NyZWF0ZWQIdHJhbnNmZXIKdHhfY29udGV4dAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAgMECAIGAwwDAQICBAgCDgMAAAAAAQkKABEFBgAAAAAAAAAAEgELAC4RCDgAAgEBAAABBAsAEAAUAgIBAAABBAsAEAEUAgMBAAABBAsAEAIUAgQBBAABEAsEEQULAQsCEgALAzgBCgAQAhQGAQAAAAAAAAAWCwAPAhUCAAEAAgEBAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAQECAAABAABYnkpfZH83aSZpy503ZFSf6a198AHUlBGYeUt7u6p2lAFIipk2EOVorl5QjgxtwWipfFNAmXUCwjRh7McJ8jAnDgIAAAAAAAAAIB6TfXDEi1tfOX7sgpY6RA4oAW+RJ5gDtQ+Q8VvIbmueWJ5KX2R/N2kmacudN2RUn+mtffAB1JQRmHlLe7uqdpQBAAAAAAAAADB1AAAAAAAAAA==

```


## Publish Example Move Module with MultiSig Account
```
➜  test-multi-sig-scripts git:(jnaulty/test-multi-sig-publish) ✗ bash multi-sig-publish.sh
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING MyFirstPackage
Successfully verified dependencies on-chain against source.
----- Transaction Digest ----
2JovP9dRPMBZrDYJuRsypikvDzwRN2W3fjXsBQEFbBWB
----- Transaction Data ----
Transaction Signature: [MultiSig(MultiSig { sigs: [Ed25519(BytesRepresentation([96, 45, 141, 55, 62, 43, 199, 2, 38, 68, 105, 42, 145, 3, 232, 12, 226, 192, 6, 146, 124, 30, 84, 154, 40, 179, 157, 166, 72, 26, 46, 31, 163, 60, 11, 184, 211, 221, 16, 165, 125, 40, 81, 145, 89, 149, 108, 251, 56, 135, 24, 243, 192, 90, 223, 216, 39, 118, 56, 86, 218, 197, 191, 14])), Ed25519(BytesRepresentation([144, 101, 95, 239, 253, 37, 15, 53, 203, 235, 6, 228, 124, 241, 137, 139, 245, 59, 223, 60, 204, 199, 241, 63, 220, 66, 19, 8, 183, 92, 72, 250, 0, 120, 134, 86, 181, 209, 192, 119, 233, 145, 109, 46, 39, 16, 135, 20, 33, 8, 124, 155, 223, 177, 47, 149, 74, 255, 70, 177, 19, 71, 95, 12]))], bitmap: RoaringBitmap<[0, 1]>, multisig_pk: MultiSigPublicKey { pk_map: [("AJQwyQMKQ7gLQw+KVbNh4pbr0473XV7Ec/j/Ljvggj3U", 1), ("AOJbaGb622hGZlwJZ5SAh2rnr1WnR1TkhzIOMnya0QFm", 1), ("AI+TXXrZDfq8vG24cNyayJHaizYN4KxYHxpwiJhYdqxK", 1)], threshold: 2 }, bytes: OnceCell(Uninit) })]
Transaction Kind : Programmable
Inputs: [Pure(SuiPureValue { value_type: Some(Address), value: "0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694" })]
Commands: [
  Publish(_,,0x00000000000000000000000000000000000000000000000000000000000000010x0000000000000000000000000000000000000000000000000000000000000002),
  TransferObjects([Result(0)],Input(0)),
]

Sender: 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694
Gas Payment: Object ID: 0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e, version: 0x2, digest: 34Mfy4hodwuKj7s485SA57p832mkevxDdE9xjeZvMsuX
Gas Owner: 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694
Gas Price: 1
Gas Budget: 30000

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x2bf9a0dc63ccbdb14e348122798f999dbad6eb8ad4b922ae6f3ca6245978e529 , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )
  - ID: 0x34164b1da7e46460c80ae29529556c6273ba760d7649247a530aa6678b1115d5 , Owner: Immutable
  - ID: 0xc474ac1e2f48922dc59ef67cd5b0c337f3c5d0235640f102b417fbdc54d15381 , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )
Mutated Objects:
  - ID: 0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e , Owner: Account Address ( 0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694 )

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
        "objectId": String("0x488a993610e568ae5e508e0c6dc168a97c5340997502c23461ecc709f230270e"),
        "version": Number(3),
        "previousVersion": Number(2),
        "digest": String("F8vbpo3NTyAwgL2Agc7mFrr7ShHCJMpPjUecf4qnd1TA"),
    },
    Object {
        "type": String("created"),
        "sender": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "objectType": String("0x34164b1da7e46460c80ae29529556c6273ba760d7649247a530aa6678b1115d5::my_module::Forge"),
        "objectId": String("0x2bf9a0dc63ccbdb14e348122798f999dbad6eb8ad4b922ae6f3ca6245978e529"),
        "version": Number(3),
        "digest": String("6sr8jKKBnJzSq2JFkTjPmy9LJbv2Epkrvzjp3JFX6sYx"),
    },
    Object {
        "type": String("published"),
        "packageId": String("0x34164b1da7e46460c80ae29529556c6273ba760d7649247a530aa6678b1115d5"),
        "version": Number(1),
        "digest": String("m2dnrvzW2tgMrmXH9Xe4WVKDm41A1U1nrScnvMebj6h"),
        "modules": Array [
            String("my_module"),
        ],
    },
    Object {
        "type": String("created"),
        "sender": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        "owner": Object {
            "AddressOwner": String("0x589e4a5f647f37692669cb9d3764549fe9ad7df001d4941198794b7bbbaa7694"),
        },
        "objectType": String("0x2::package::UpgradeCap"),
        "objectId": String("0xc474ac1e2f48922dc59ef67cd5b0c337f3c5d0235640f102b417fbdc54d15381"),
        "version": Number(3),
        "digest": String("AGAKHNPH2u6uiC8VZQWGNCGmiKoaGbr8fCaQvRH3Bwyi"),
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


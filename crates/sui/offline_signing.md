# Offline Signing

This is a guide for users who wish to sign a transaction using an offline device without relying on sui keystore. Here are the steps to:
 1. Serialize the data for signing;
 2. Take the serialized data elsewhere to sign (wallet of your choice, or tools in other languages) to produce a signature and the corresponding public key.
 3. Execute the signed transaction.

## Step 1: Serialize a Transfer

A transaction data must be serialized according to [BCS](https://crates.io/crates/bcs). It is supported in [other languages](https://github.com/zefchain/serde-reflection#language-interoperability).

Here an example is provided to serialize a transfer data in CLI. This outputs a serialized transaction data in Base64.

```shell
sui client serialize-transfer-sui --to 0x581a119a6576d3b502b5dc47c5de497b774e68ca --sui-coin-object-id 0x0599b794da39169f7c75d34eba06ae105fedc61b --gas-budget 1000
VHJhbnNhY3Rpb25EYXRhOjoAA1gaEZpldtO1ArXcR8XeSXt3TmjKAFgaEZpldtO1ArXcR8XeSXt3TmjKBZm3lNo5Fp98ddNOugauEF/txhsCAAAAAAAAACC0knjIoZEdbQBQuqg3feG/GA0L2v9gLDfH2uX8iGf5SwEAAAAAAAAA6AMAAAAAAAA=
```

## Step 2: Sign the data
This can be done elsewhere in your signing device or implemented in languages of your choice. Sui accepts signature for both ECDSA Secp256k1 and pure Ed25519. 

An accepted ECDSA Secp256k1 signature follows:
1. The signature must be of length 65 bytes in the form of `[r, s, v]` where the first 32 bytes are `r`, the second 32 bytes are `s` and the last byte is `v`. 
2. The `r` value can be between 0x1 and 0xFFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364140 (inclusive). 
3. The `s` value must be in the lower half of the curve order, i.e. between 0x1 and 0x7FFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF 5D576E73 57A4501D DFE92F46 681B20A0 (inclusive). 
4. The `v` represents the recovery ID, which must be normalized to 0, 1, 2 or 3. Note that unlike [EIP-155](https://eips.ethereum.org/EIPS/eip-155) chain ID is not used to calculate the `v` value. 
5. Ideally, the signature must be generated with deterministic nonce according to [RFC6979](https://www.rfc-editor.org/rfc/rfc6979).

An accepted pure Ed25519 signature follows:
1. The signature must be produced according to [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6).
2. The signature must be valid according to [ZIP215](https://github.com/zcash/zips/blob/main/zip-0215.rst).

Here we use the keytool command to sign as an example, using the Ed25519 key corresponding to the provided address stored in `sui.keystore`. This commands outputs the signature, the public key and the flag encoded in Base64. This command is backed by [fastcrypto](https://crates.io/crates/fastcrypto).

```shell
sui keytool sign --address 0x581a119a6576d3b502b5dc47c5de497b774e68ca --data VHJhbnNhY3Rpb25EYXRhOjoAA1gaEZpldtO1ArXcR8XeSXt3TmjKAFgaEZpldtO1ArXcR8XeSXt3TmjKBZm3lNo5Fp98ddNOugauEF/txhsCAAAAAAAAACC0knjIoZEdbQBQuqg3feG/GA0L2v9gLDfH2uX8iGf5SwEAAAAAAAAA6AMAAAAAAAA=
2022-10-18T03:30:39.510775Z  INFO sui::keytool: Data to sign : VHJhbnNhY3Rpb25EYXRhOjoAA1gaEZpldtO1ArXcR8XeSXt3TmjKAFgaEZpldtO1ArXcR8XeSXt3TmjKBZm3lNo5Fp98ddNOugauEF/txhsCAAAAAAAAACC0knjIoZEdbQBQuqg3feG/GA0L2v9gLDfH2uX8iGf5SwEAAAAAAAAA6AMAAAAAAAA=
2022-10-18T03:30:39.510838Z  INFO sui::keytool: Address : 0x581a119a6576d3b502b5dc47c5de497b774e68ca
2022-10-18T03:30:39.511304Z  INFO sui::keytool: Flag Base64: AA==
2022-10-18T03:30:39.511318Z  INFO sui::keytool: Public Key Base64: rJzjxQ+FCK9m8YDU8Dq1Yx931HkIArhcw33kUPL9P8c=
2022-10-18T03:30:39.511326Z  INFO sui::keytool: Signature : epIttAjg4OBOzVBQQuMflR9sJwh12XiBFwDV9gmiBxomKJ0YyjcbhLONdvA1xs2NXy8xdagwHR/uRVdI6z+LAg==
```

## Step 3: Execute the signed transaction

Now that you had obtained the signature, signing scheme flag, and public key, you can submit using the execution transaction command. This command takes in the unsigned transaction data in Base64, the scheme flag for which the signature is produced wutg (can be `ed25519` or `secp256k1`), the public key in Base64, and the corresponding signature in Base64. This executes the signed transaction and returns the certificate and transaction effects if successful. 

```shell
sui client execute-signed-tx --tx-data VHJhbnNhY3Rpb25EYXRhOjoAA1gaEZpldtO1ArXcR8XeSXt3TmjKAFgaEZpldtO1ArXcR8XeSXt3TmjKBZm3lNo5Fp98ddNOugauEF/txhsCAAAAAAAAACC0knjIoZEdbQBQuqg3feG/GA0L2v9gLDfH2uX8iGf5SwEAAAAAAAAA6AMAAAAAAAA= --scheme ed25519 --pubkey rJzjxQ+FCK9m8YDU8Dq1Yx931HkIArhcw33kUPL9P8c= --signature epIttAjg4OBOzVBQQuMflR9sJwh12XiBFwDV9gmiBxomKJ0YyjcbhLONdvA1xs2NXy8xdagwHR/uRVdI6z+LAg==
----- Certificate ----
Transaction Hash: wnk9u71q8mhPgEOrDZJacVyqAzNBAmsMOPM4rNoS0LE=
Transaction Signature: AA==@epIttAjg4OBOzVBQQuMflR9sJwh12XiBFwDV9gmiBxomKJ0YyjcbhLONdvA1xs2NXy8xdagwHR/uRVdI6z+LAg==@rJzjxQ+FCK9m8YDU8Dq1Yx931HkIArhcw33kUPL9P8c=
Signed Authorities Bitmap: RoaringBitmap<[0, 1, 3]>
Transaction Kind : Transfer SUI
Recipient : 0x581a119a6576d3b502b5dc47c5de497b774e68ca
Amount: Full Balance

----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0x0599b794da39169f7c75d34eba06ae105fedc61b , Owner: Account Address ( 0x581a119a6576d3b502b5dc47c5de497b774e68ca )
```
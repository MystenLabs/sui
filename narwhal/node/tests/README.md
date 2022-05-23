## What this test is about 

This repository checks the conformance of our code to a BCS-compatible manifest of our serialized data formats.

It does this by running a manifest generator from the code (using serde-reflection) and checking the output has not changed.

If it has in a legitimate fashion (e.g. we update one of our main types), all that's left to do is to re-run the generator and check in the change.

Here are the references to the software above:
* https://github.com/diem/bcs
* https://github.com/zefchain/serde-reflection

## Examples

In this example, we will update one of our core types (DeleteBatches), and then update the manifest:

```
narwhal/node(main)» ruplacer --subvert 'DeleteBatches' 'RemoveBatches' --go                                                                                       [14:12:34]
./node/tests/staged/narwhal.yaml:70 - DeleteBatches:
./node/tests/staged/narwhal.yaml:70 + RemoveBatches:

./node/src/generate_format.rs:102 - PrimaryWorkerMessage::<Ed25519PublicKey>::DeleteBatches(vec![BatchDigest([0u8; 32])]);
./node/src/generate_format.rs:102 + PrimaryWorkerMessage::<Ed25519PublicKey>::RemoveBatches(vec![BatchDigest([0u8; 32])]);

./worker/src/tests/synchronizer_tests.rs:201 - let message = PrimaryWorkerMessage::<Ed25519PublicKey>::DeleteBatches(batch_digests.clone());
./worker/src/tests/synchronizer_tests.rs:201 + let message = PrimaryWorkerMessage::<Ed25519PublicKey>::RemoveBatches(batch_digests.clone());

./worker/src/synchronizer.rs:191 - PrimaryWorkerMessage::DeleteBatches(digests) => {
./worker/src/synchronizer.rs:191 + PrimaryWorkerMessage::RemoveBatches(digests) => {
./worker/src/synchronizer.rs:192 - self.handle_delete_batches(digests).await;
./worker/src/synchronizer.rs:192 + self.handle_remove_batches(digests).await;
./worker/src/synchronizer.rs:265 - async fn handle_delete_batches(&mut self, digests: Vec<BatchDigest>) {
./worker/src/synchronizer.rs:265 + async fn handle_remove_batches(&mut self, digests: Vec<BatchDigest>) {

./primary/src/primary.rs:63 - DeleteBatches(Vec<BatchDigest>),
./primary/src/primary.rs:63 + RemoveBatches(Vec<BatchDigest>),

Performed 7 replacements on 5 matching files
```

Now our code is modified in a way that will make the format test fail: let's update the manifest.

```
narwhal/node(main)» cd node                                                                                                                                       [14:12:53]
narwhal/node(main)» cargo -q run --example generate-format -- print > tests/staged/narwhal.yaml
```


Let's check that we pass the test again:
```
narwhal/node(main)» cargo test -- format 2>&1 |tail -n 40                                                                                                      [14:13:47]

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/formats.rs (narwhal/target/debug/deps/formats-de36ac230681a99f)

running 1 test
   Compiling typenum v1.15.0
   Compiling network v0.1.0 (narwhal/network)
   Compiling generic-array v0.14.5
   Compiling digest v0.9.0
   Compiling crypto-mac v0.8.0
   Compiling block-buffer v0.9.0
   Compiling ark-serialize v0.3.0
   Compiling blake2 v0.9.2
   Compiling sha2 v0.9.9
   Compiling curve25519-dalek v3.2.0
   Compiling ark-ff v0.3.0
   Compiling ed25519-dalek v1.0.1
   Compiling ark-ec v0.3.0
   Compiling ark-relations v0.3.0
   Compiling ark-snark v0.3.0
   Compiling ark-bls12-377 v0.3.0
   Compiling ark-crypto-primitives v0.3.0
   Compiling ark-ed-on-cp6-782 v0.3.0
   Compiling ark-ed-on-bw6-761 v0.3.0
   Compiling bls-crypto v0.2.0 (https://github.com/huitseeker/celo-bls-snark-rs?branch=updates-2#9f5a0e6f)
   Compiling crypto v0.1.0 (narwhal/crypto)
   Compiling config v0.1.0 (narwhal/config)
   Compiling primary v0.1.0 (narwhal/primary)
   Compiling consensus v0.1.0 (narwhal/consensus)
   Compiling worker v0.1.0 (narwhal/worker)
   Compiling node v0.1.0 (narwhal/node)
    Finished dev [unoptimized + debuginfo] target(s) in 7.68s
     Running `target/debug/examples/generate-format test`
test test_format ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.08s

narwhal/node(main)» git status -s                                                                                                                              [14:14:39]
 M src/generate_format.rs
 M tests/staged/narwhal.yaml
 M ../primary/src/primary.rs
 M ../worker/src/synchronizer.rs
 M ../worker/src/tests/synchronizer_tests.rs
 ```
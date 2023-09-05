const addresses = [ [ /* ... */ ], [ /* ... */ ], /* ... */ ];
const setupObject = '0x....'; // shared Setup {}
const setupCapArg = '0x....'; // owned SetupCap {}

let hashes = addresses.map((batch) => {
    return blake2b(bcs.ser('vector<address>', batch))
});

let tx = new TransactionBlock();
let capArg = tx.object(setupCapArg);
let setupArg = tx.pure(setupObject);
let hashesArg = tx.pure(hashes, 'vector<vector<u8>>');

tx.moveCall({
    target: `${myPackage}::simple::prepare`,
    args: [ capArg, setupArg, hashesArg ],
});

await client.signAndExecuteTransactionBlock({
    signer: keypair,
    TransactionBlock: tx,
    options: { /* ... */ }
});


// Path: examples/sui-move/airdrop/scripts/txs.js

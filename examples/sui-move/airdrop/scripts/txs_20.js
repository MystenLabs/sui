const addresses = [ /* ... */ ];
const adminCap = '0x.....';

let tx = new TransactionBlock();
let capArg = tx.object(adminCap); // reusable arguments should be defined once

while (addresses.length > 0) {
    let addrArg = tx.pure(addresses.pop(), 'address');

    tx.moveCall({
        target: `${myPackage}::simple::mint_to_address`,
        args: [ capArg, addrArg ],
    });
}

await client.signAndExecuteTransactionBlock({
    signer: keypair,
    TransactionBlock: tx,
    options: { /* ... */ }
});

// Path: examples/sui-move/airdrop/scripts/txs_2.js

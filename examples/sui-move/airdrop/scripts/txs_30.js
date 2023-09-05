const addrBatches = [ [ /* ... */ ], [ /* ... */ ], /* ... */ ];
const adminCap = '0x.....';

let tx = new TransactionBlock();
let capArg = tx.object(adminCap); // reusable arguments should be defined once

while (addrBatches.length > 0) {
    let i = 0;

    while (i < 4) {
        let addresses = addrBatches.pop();
        let addrVecArg = tx.pure(addresses.pop(), 'vector<address>');
        tx.moveCall({
            target: `${myPackage}::simple::mint_to_addresses`,
            args: [ capArg, addrVecArg ],
        });
        i++;
    }

    await client.signAndExecuteTransactionBlock({
        signer: keypair,
        TransactionBlock: tx,
        options: { /* ... */ }
    });
}

// Path: examples/sui-move/airdrop/scripts/txs_30.js

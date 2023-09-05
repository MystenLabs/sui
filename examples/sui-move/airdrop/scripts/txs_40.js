const addrBatches = [ [ /* ... */ ], [ /* ... */ ], /* ... */ ];
const adminCap = '0x.....';

let tx = new TransactionBlock();

while (addrBatches.length > 0) {
    let i = 0;
    let vecArg = tx.moveCall({
        target: '0x1::vector::empty',
        typeArguments: [ 'address' ]
    });

    while (i < 4) {
        let addresses = addrBatches.pop();
        let addrVecArg = tx.pure(
            addresses.pop(),
            'vector<address>'
        );

        tx.moveCall({
            target: '0x1::vector::append',
            typeArguments: [ 'address' ],
            args: [ vecArg, addrVecArg ],
        });

        i++;
    }

    tx.moveCall({
        target: `${myPackage}::simple::mint_to_addresses`,
        args: [ capArg, vecArg ],
    });

    await client.signAndExecuteTransactionBlock({
        signer: keypair,
        TransactionBlock: tx,
        options: { /* ... */ }
    });
}

// Path: examples/sui-move/airdrop/scripts/txs_30.js

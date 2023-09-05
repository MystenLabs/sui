const addresses = [ /* ... */ ];

while (addresses.length > 0) {
    let tx = new TransactionBlock();
    let addrArg = tx.pure(addresses.pop(), 'address');

    tx.moveCall({
        target: `${myPackage}::simple::mint_to_address`,
        args: [ addrArg ],
    });

    await client.signAndExecuteTransactionBlock({
        signer: keypair,
        TransactionBlock: tx,
        options: { /* ... */ }
    });
}

// Path: examples/sui-move/airdrop/scripts/txs.js

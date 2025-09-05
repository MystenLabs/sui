// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {SuiClient} from "@mysten/sui/client";
import {
    ADMIN_SECRET_KEY, COIN_TYPE,
    DENY_CAP_ID,
    SUI_DENY_LIST_OBJECT_ID,
    SUI_NETWORK,
    TREASURY_CAP_ID,
} from "./config";
import {Transaction} from '@mysten/sui/transactions';
import {program} from "commander";
import {Ed25519Keypair} from "@mysten/sui/keypairs/ed25519";


const run = async () => {

    program
        .name('regulated-coin-utility')
        .description('CLI to manage your regulated coin.')
        .version('0.0.1');

    // docs::#deny
    program.command('deny-list-add')
        .description('Adds an address to the deny list.')
        .requiredOption('--address <address>', 'Address to add.')

        .action((options) => {
            console.log("Executing addition to deny list...");
            console.log("Address to add to deny list: ", options.address);
            const txb = new Transaction();

            txb.moveCall({
                target: `0x2::coin::deny_list_v2_add`,
                arguments: [
                    txb.object(SUI_DENY_LIST_OBJECT_ID),
                    txb.object(DENY_CAP_ID),
                    txb.pure.address(options.address),
                ],
                typeArguments: [COIN_TYPE],
            });

            executeTx(txb);
        });


    program.command('deny-list-remove')
        .description('Removes an address from the deny list.')
        .requiredOption('--address <address>', 'Address to add.')
        .requiredOption('--deny_list <address>', 'Deny list object ID.')

        .action((options) => {
            console.log("Executing removal from deny list...");
            console.log("Address to remove in deny list: ", options.address);

            if(!DENY_CAP_ID) throw new Error("DENY_CAP_ID environment variable is not set. Are you sure the active address owns the deny list object?");

            const txb = new Transaction();

            txb.moveCall({
                target: `0x2::coin::deny_list_v2_remove`,
                arguments: [
                    txb.object(SUI_DENY_LIST_OBJECT_ID),
                    txb.object(DENY_CAP_ID),
                    txb.pure.address(options.address),
                ],
                typeArguments: [COIN_TYPE],
            });

            executeTx(txb);
        });
    // docs::/#deny
    // docs::#mint
    program.command('mint-and-transfer')
        .description('Mints coins and transfers to an address.')
        .requiredOption('--amount <amount>', 'The amount of coins to mint.')
        .requiredOption('--address <address>', 'Address to send coins.')

        .action((options) => {
            console.log("Executing mint new coins and transfer to address...");

            console.log("Amount to mint: ", options.amount);
            console.log("Address to send coins: ", options.address);
            console.log("TREASURY_CAP_ID: ", TREASURY_CAP_ID);
            console.log("COIN_TYPE: ", COIN_TYPE);

            if(!TREASURY_CAP_ID) throw new Error("TREASURY_CAP_ID environment variable is not set.");

            const txb = new Transaction();

            const coin = txb.moveCall({
                target: `0x2::coin::mint`,
                arguments: [
                    txb.object(TREASURY_CAP_ID),
                    txb.pure.u64(options.amount),
                ],
                typeArguments: [COIN_TYPE],
            });

            txb.transferObjects([coin], txb.pure.address(options.address));

            executeTx(txb);
        });
    // docs::/#mint


    program.command('burn')
        .description('Burns coins.')
        .requiredOption('--coin <address>', 'The coin to burn.')
        .action((options) => {
            console.log("Executing burn coin...");
            console.log("Coin to burn: ", options.coin);

            if(!TREASURY_CAP_ID) throw new Error("TREASURY_CAP_ID environment variable is not set.");

            const txb = new Transaction();

            txb.moveCall({
                target: `0x2::coin::burn`,
                arguments: [
                    txb.object(TREASURY_CAP_ID),
                    txb.object(options.coin),
                ],
                typeArguments: [COIN_TYPE],
            });

            executeTx(txb);
        });

    program.command('help')
        .description('prints help')
        .action((options) => {
            console.log("Regulated coin utility.");
            program.outputHelp();
        });

    program.parse();

};

run();

async function executeTx(txb: Transaction) {

    console.log("Connecting to Sui network: ", SUI_NETWORK);
    const suiClient = new SuiClient({url: SUI_NETWORK});

    if(!ADMIN_SECRET_KEY) throw new Error("ADMIN_SECRET_KEY environment variable is not set.");

    const adminKeypair = Ed25519Keypair.fromSecretKey(
      ADMIN_SECRET_KEY
    );

    txb.setGasBudget(1000000000);

    suiClient.signAndExecuteTransaction({
        signer: adminKeypair,
        transaction: txb,
        requestType: 'WaitForLocalExecution',
        options: {
            showEvents: true,
            showEffects: true,
            showObjectChanges: true,
            showBalanceChanges: true,
            showInput: true,
        }
    }).then((res) => {

        const status = res?.effects?.status.status;

        console.log("TxDigest = ", res?.digest);
        console.log("Status = ", status);

        if (status === "success") {
            console.log("Transaction executed successfully.");
        }
        if (status == "failure") {
            console.log("Transaction error = ", res?.effects?.status);
        }
    });

}
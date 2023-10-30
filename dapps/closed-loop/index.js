// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from "fs";
import path from "path";
import { program } from 'commander';
import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { TransactionBlock } from "@mysten/sui.js/src/builder";


/** Stores the result of the publish operation */
const publishRes = JSON.parse(fs.readFileSync(path.resolve('./published.json')));
const { pkg, cap, policy } = readConfig(publishRes);
const client = new SuiClient({ url: getFullnodeUrl('devnet') });

program
    .name('closed-loop-cli')
    .description('Simple Utility to try the Closed Loop Token functions')
    .version('0.1.0');

program
    .command('new')
    .description('Publish a new Closed Loop token')
    .action(newToken);

program
    .command('balance')
    .description('Check the balance of the current user')
    .action(getBalance);

program
    .command('allowed-actions')
    .description('Check allowed actions')
    .action(getAllowedActions);

program.parse(process.argv);

/**
 * Command: `new`
 * Publish a new Closed Loop token
 */
function newToken() {
    throw new Error('Not implemented');
}

/**
 * Command: `balance`
 * Check the balance of the current user
 */
async function getBalance() {
    const { data, error } = await client.getOwnedObjects({
        owner: address,
        
    })


    throw new Error('Not implemented');
}

/**
 * Command: `allowed-actions`
 * Check allowed actions
 */
async function getAllowedActions() {
    const { data, error } = await client.getObject({
        id: policy.objectId,
        options: {
            showContent: true
        }
    });

    if (error) {
        throw new Error("Error occured while fetching: " + error);
    }

    let rules = data.content.fields.rules.fields.contents;
    if (rules.length == 0) {
        console.log("- No actions are allowed");
    }
    console.table(rules);
}

async function allow() {
    let tx = new TransactionBlock();
    let capArg = tx.objectRef(cap);
    let policyArg = tx.sharedObjectRef(policy);
}

// === Admin Actions ===

function allow(tx, tokenType, policy, cap, amount) {
    return tx.moveCall({
        target: `${pkg}::closed_loop::allow`,
        arguments: [ policy, cap, amount ],
        typeArguments: [ tokenType ]
    });
}


// === Public Actions ===

/** Split a token into two - returns a TxResult */
function split(tx, tokenType, token, amount) {
    return tx.moveCall({
        target: `${pkg}::closed_loop::split`,
        arguments: [ token, amount ],
        typeArguments: [ tokenType ]
    });
}

/** Join two tokens together */
function join(tx, tokenType, token, another) {
    return tx.moveCall({
        target: `${pkg}::closed_loop::join`,
        arguments: [ token, another ],
        typeArguments: [ tokenType ]
    });
}

/** Create a zero token */
function zero(tx, tokenType, token) {
    return tx.moveCall({
        target: `${pkg}::closed_loop::zero`,
        arguments: [ token ],
        typeArguments: [ tokenType ]
    });
}

/** Destroy a zero token */
function destroyZero(tx, tokenType, token) {
    return tx.moveCall({
        target: `${pkg}::closed_loop::destroy_zero`,
        arguments: [ token ],
        typeArguments: [ tokenType ]
    });
}

// === Utilities ===

function readConfig(cfg) {
    let changes = cfg.objectChanges;

    const pkg = changes.find((e) => e.type == "published");
    const cap = changes.find((e) => e.objectType && e.objectType.includes('TokenPolicyCap<'));
    const policy = changes.find((e) => e.objectType && e.objectType.includes('TokenPolicy<'));

    return {
        tokenType: `${pkg.packageId}::cli::CLI`,
        pkg: pkg.packageId,
        cap: {
            objectId: cap.objectId,
            version: cap.version,
            digest: cap.digest
        },
        policy: {
            objectId: policy.objectId,
            initialSharedVersion: policy.owner.Shared.initial_shared_version,
            mutable: true
        }
    }
}

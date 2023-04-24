// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type Transport from '@ledgerhq/hw-transport';
import sha256 from 'fast-sha256';

export type GetPublicKeyResult = {
    publicKey: Uint8Array;
    address: Uint8Array;
};

export type SignTransactionResult = {
    signature: Uint8Array;
};

export type GetVersionResult = {
    major: number;
    minor: number;
    patch: number;
};

enum LedgerToHost {
    RESULT_ACCUMULATING = 0,
    RESULT_FINAL = 1,
    GET_CHUNK = 2,
    PUT_CHUNK = 3,
}

enum HostToLedger {
    START = 0,
    GET_CHUNK_RESPONSE_SUCCESS = 1,
    GET_CHUNK_RESPONSE_FAILURE = 2,
    PUT_CHUNK_RESPONSE = 3,
    RESULT_ACCUMULATING_RESPONSE = 4,
}

/**
 * Sui API
 *
 * @example
 * import Sui from "@mysten/ledgerjs-hw-app-sui";
 * const sui = new Sui(transport)
 */
export default class Sui {
    transport: Transport;
    readonly #verbose: boolean;

    constructor(
        transport: Transport,
        scrambleKey = 'default_sui_scramble_key',
        verbose = false
    ) {
        this.#verbose = verbose;
        this.transport = transport;
        this.transport.decorateAppAPIMethods(
            this,
            ['getPublicKey', 'signTransaction', 'getVersion'],
            scrambleKey
        );
    }

    /**
     * Retrieves the public key associated with a particular BIP32 path from the Ledger app.
     *
     * @param path - the path to retrieve.
     * @param displayOnDevice - whether or not the address should be displayed on the device.
     *
     */
    async getPublicKey(
        path: string,
        displayOnDevice = false
    ): Promise<GetPublicKeyResult> {
        const cla = 0x00;
        const ins = displayOnDevice ? 0x01 : 0x02;
        const p1 = 0;
        const p2 = 0;
        const payload = buildBip32KeyPayload(path);
        const response = await this.#sendChunks(cla, ins, p1, p2, payload);
        const keySize = response[0];
        const publicKey = response.slice(1, keySize + 1); // slice uses end index.
        let address: Uint8Array | null = null;
        if (response.length > keySize + 2) {
            const addressSize = response[keySize + 1];
            address = response.slice(keySize + 2, keySize + 2 + addressSize);
        }
        const res: GetPublicKeyResult = {
            publicKey: publicKey,
            address: address!,
        };
        return res;
    }

    /**
     * Sign a transaction with the key at a BIP32 path.
     *
     * @param txn - The transaction; this can be any of a node Buffer, Uint8Array, or a hexadecimal string, encoding the form of the transaction appropriate for hashing and signing.
     * @param path - the path to use when signing the transaction.
     */
    async signTransaction(
        path: string,
        txn: string | Buffer | Uint8Array
    ): Promise<SignTransactionResult> {
        const cla = 0x00;
        const ins = 0x03;
        const p1 = 0;
        const p2 = 0;
        // Transaction payload is the byte length as uint32le followed by the bytes
        // Type guard not actually required but TypeScript can't tell that.
        if (this.#verbose) this.#log(txn);
        const rawTxn =
            typeof txn == 'string' ? Buffer.from(txn, 'hex') : Buffer.from(txn);
        const hashSize = Buffer.alloc(4);
        hashSize.writeUInt32LE(rawTxn.length, 0);
        // Bip32key payload same as getPublicKey
        const bip32KeyPayload = buildBip32KeyPayload(path);
        // These are just squashed together
        const payload_txn = Buffer.concat([hashSize, rawTxn]);
        this.#log('Payload Txn', payload_txn);
        // TODO batch this since the payload length can be uint32le.max long
        const signature = await this.#sendChunks(cla, ins, p1, p2, [
            payload_txn,
            bip32KeyPayload,
        ]);
        return {
            signature,
        };
    }

    /**
     * Retrieve the app version on the attached Ledger device.
     */
    async getVersion(): Promise<GetVersionResult> {
        const [major, minor, patch] = await this.#sendChunks(
            0x00,
            0x00,
            0x00,
            0x00,
            Buffer.alloc(1)
        );
        return {
            major,
            minor,
            patch,
        };
    }

    /**
     * Convert a raw payload into what is essentially a singly-linked list of chunks, which
     * allows the ledger to re-seek the data in a secure fashion.
     */
    async #sendChunks(
        cla: number,
        ins: number,
        p1: number,
        p2: number,
        payload: Buffer | Buffer[],
        // Constant (protocol dependent) data that the ledger may want to refer to
        // besides the payload.
        extraData: Map<String, Buffer> = new Map<String, Buffer>()
    ): Promise<Buffer> {
        let chunkSize = 180;
        if (!(payload instanceof Array)) {
            payload = [payload];
        }
        let parameterList: Buffer[] = [];
        let data = new Map<String, Buffer>(extraData);
        for (let j = 0; j < payload.length; j++) {
            let chunkList: Buffer[] = [];
            for (let i = 0; i < payload[j].length; i += chunkSize) {
                let cur = payload[j].slice(i, i + chunkSize);
                chunkList.push(cur);
            }
            // Store the hash that points to the "rest of the list of chunks"
            let lastHash = Buffer.alloc(32);
            this.#log(lastHash);
            // Since we are doing a foldr, we process the last chunk first
            // We have to do it this way, because a block knows the hash of
            // the next block.
            data = chunkList.reduceRight((blocks, chunk) => {
                let linkedChunk = Buffer.concat([lastHash, chunk]);
                this.#log('Chunk: ', chunk);
                this.#log('linkedChunk: ', linkedChunk);
                lastHash = Buffer.from(sha256(linkedChunk));
                blocks.set(lastHash.toString('hex'), linkedChunk);
                return blocks;
            }, data);
            parameterList.push(lastHash);
            lastHash = Buffer.alloc(32);
        }
        this.#log(data);
        return await this.#handleBlocksProtocol(
            cla,
            ins,
            p1,
            p2,
            Buffer.concat(
                [Buffer.from([HostToLedger.START])].concat(parameterList)
            ),
            data
        );
    }

    async #handleBlocksProtocol(
        cla: number,
        ins: number,
        p1: number,
        p2: number,
        initialPayload: Buffer,
        data: Map<String, Buffer>
    ): Promise<Buffer> {
        let payload = initialPayload;
        let result = Buffer.alloc(0);
        do {
            this.#log('Sending payload to ledger: ', payload.toString('hex'));
            let rv = await this.transport.send(cla, ins, p1, p2, payload);
            this.#log('Received response: ', rv);
            var rv_instruction = rv[0];
            let rv_payload = rv.slice(1, rv.length - 2); // Last two bytes are a return code.
            if (!(rv_instruction in LedgerToHost)) {
                throw new TypeError('Unknown instruction returned from ledger');
            }
            switch (rv_instruction) {
                case LedgerToHost.RESULT_ACCUMULATING:
                case LedgerToHost.RESULT_FINAL:
                    result = Buffer.concat([result, rv_payload]);
                    // Won't actually send this if we drop out of the loop for RESULT_FINAL
                    payload = Buffer.from([
                        HostToLedger.RESULT_ACCUMULATING_RESPONSE,
                    ]);
                    break;
                case LedgerToHost.GET_CHUNK:
                    let chunk = data.get(rv_payload.toString('hex'));
                    this.#log('Getting block ', rv_payload);
                    this.#log('Found block ', chunk);
                    if (chunk) {
                        payload = Buffer.concat([
                            Buffer.from([
                                HostToLedger.GET_CHUNK_RESPONSE_SUCCESS,
                            ]),
                            chunk,
                        ]);
                    } else {
                        payload = Buffer.from([
                            HostToLedger.GET_CHUNK_RESPONSE_FAILURE,
                        ]);
                    }
                    break;
                case LedgerToHost.PUT_CHUNK:
                    data.set(
                        Buffer.from(sha256(rv_payload)).toString('hex'),
                        rv_payload
                    );
                    payload = Buffer.from([HostToLedger.PUT_CHUNK_RESPONSE]);
                    break;
            }
        } while (rv_instruction !== LedgerToHost.RESULT_FINAL);
        return result;
    }

    #log(...args: any[]) {
        if (this.#verbose) console.log(args);
    }
}

function buildBip32KeyPayload(path: string): Buffer {
    const paths = splitPath(path);
    // Bip32Key payload is:
    // 1 byte with number of elements in u32 array path
    // Followed by the u32 array itself
    const payload = Buffer.alloc(1 + paths.length * 4);
    payload[0] = paths.length;
    paths.forEach((element, index) => {
        payload.writeUInt32LE(element, 1 + 4 * index);
    });
    return payload;
}

// TODO use bip32-path library
function splitPath(path: string): number[] {
    const result: number[] = [];
    const components = path.split('/');
    components.forEach((element) => {
        let number = parseInt(element, 10);

        if (isNaN(number)) {
            return; // FIXME shouldn't it throws instead?
        }

        if (element.length > 1 && element[element.length - 1] === "'") {
            number += 0x80000000;
        }

        result.push(number);
    });
    return result;
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { faker } from '@faker-js/faker';
import { describe, beforeEach, expect, it } from 'vitest';

import { SuinsClient } from '../src';
import { getFullnodeUrl, SuiClient } from '@mysten/sui.js/client';

const domainName = 'test.sui';
const walletAddress = '0xfce343a643991c592c4f1a9ee415a7889293f694ab8828f78e3c81d11c9530c6';

describe('SuiNS Client', () => {
    const client = new SuinsClient(new SuiClient({ url: getFullnodeUrl('testnet') }), {
        networkType: 'testnet',
        contractObjects: {
            packageId: '0xfdba31b34a43e058f17c5cf4b12d9b9e0a08c0623d8569092c022e0c77df46d3',
            registry: '0xac06695279c2a92436068cebe5ea778135ac503337642e27493431603ae6a71d',
            reverseRegistry: '0x34a36dd204f8351a157d19b87bada9d448ec40229d56f22bff04fa23713a5c31',
            suins: '0x4acaf19db12fafce1943bbd44c7f794e1d81d00aeb63617096e5caa39499ba88',
        },
    });

    const nonExistingDomain = walletAddress + '.sui';
    const nonExistingWalletAddress = walletAddress.substring(0, walletAddress.length - 4) + '0000';

    beforeEach(async () => {
        await client.getSuinsContractObjects();
    });

    describe('getAddress', () => {
        describe('input domain has a linked address set', () => {
            it('returns the linked address', async () => {
                expect(await client.getAddress(domainName)).toEqual(walletAddress);
            });
        });

        describe('input domain does not have a linked address set', () => {
            it('returns undefined', async () => {
                expect(await client.getAddress(nonExistingDomain)).toBeUndefined();
            });
        });
    });

    describe('getName', () => {
        describe('input domain has a default name set', () => {
            it('returns the default name', async () => {
                expect(await client.getName(walletAddress)).toBe(domainName);
            });
        });

        describe('input domain does not have a default name set', () => {
            it('returns undefined', async () => {
                expect(await client.getName(nonExistingWalletAddress)).toBeUndefined();
            });
        });
    });

    describe('getNameObject', () => {
        it('returns related data of the name', async () => {
            expect(
                await client.getNameObject(domainName, {
                    showOwner: true,
                    showAvatar: true,
                }),
            ).toMatchObject({
                id: '0x7ee9ac31830e91f76f149952f7544b6d007b9a5520815e3d30264fa3d2791ad1',
                nftId: '0x2879ff9464f06c0779ca34eec6138459a3e9855852dd5d1a025164c344b2b555',
                expirationTimestampMs: '1715765005617',
                owner: walletAddress,
                targetAddress: walletAddress,
                // avatar: 'https://api-testnet.suifrens.sui.io/suifrens/0x4e3ba002444df6c6774f41833f881d351533728d585343c58cca1fec1fef74ef/svg',
                contentHash: 'QmZsHKQk9FbQZYCy7rMYn1z6m9Raa183dNhpGCRm3fX71s',
            });
        });

        it('Does not include avatar if the flag is off', async () => {
            expect(
                await client.getNameObject(domainName, {
                    showOwner: true,
                }),
            ).not.toHaveProperty('avatar');
        });

        it('Does not include owner if the flag is off', async () => {
            expect(await client.getNameObject(domainName)).not.toHaveProperty('owner');
        });
    });
});

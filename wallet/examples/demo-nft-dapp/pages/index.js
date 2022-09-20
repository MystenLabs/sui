// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Head from 'next/head';
import { useCallback } from 'react';
import { MODULE, PACKAGE_ID } from '../lottery/constants';
import { LotteryList } from '../lottery/list';
import { useSuiObjects } from '../shared/objects-store-context';
import { sleep } from '../shared/sleep';
import { useSuiWallet } from '../shared/useSuiWallet';

export default function Home() {
    const suiWallet = useSuiWallet();
    const { triggerUpdate } = useSuiObjects();
    const onHandleCreate = useCallback(async () => {
        console.log(suiWallet);
        if (suiWallet) {
            try {
                await suiWallet.executeMoveCall({
                    packageObjectId: PACKAGE_ID,
                    module: MODULE,
                    function: 'create_lottery',
                    typeArguments: [],
                    arguments: [],
                    gasBudget: 1000,
                });
                await sleep(500);
                triggerUpdate();
            } catch (e) {
                console.log(e);
            }
        }
    }, [suiWallet, triggerUpdate]);
    return (
        <>
            <Head>
                <title>LuckyCapy</title>
            </Head>
            <button type="button" onClick={onHandleCreate}>
                Create new lottery
            </button>
            <LotteryList />
        </>
    );
}

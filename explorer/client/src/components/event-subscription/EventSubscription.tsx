// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiEventFilter } from '@mysten/sui.js';
import React, { useState, useCallback, useContext } from 'react';

import { ReactComponent as SearchIcon } from '../../assets/search.svg';
import { NetworkContext } from '../../context';
import { DefaultRpcClient as rpc } from '../../utils/api/DefaultRpcClient';

import styles from './EventSubscription.module.css';

//const DEVNFT_FILTER = '{"All": [{"EventType": "MoveEvent"}, {"Package": "0x2"}, {"Module": "devnet_nft"}]}';

function EventSubscription() {
    const [input, setInput] = useState('');
    const [network] = useContext(NetworkContext);
    const [pleaseWaitMode, setPleaseWaitMode] = useState(false);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();
            // Prevent empty search
            if (!input.length) return;
            setPleaseWaitMode(true);

            // remove empty char from input
            let query = input.trim();

            try {
                let filter: SuiEventFilter = JSON.parse(query);
                console.log('filter', filter);

                let rpcProvider = rpc(network);
                console.log('rpc provider', rpcProvider);

                rpcProvider.subscribeEvent(filter, (event) => {
                    console.log(event);
                });
            } catch (e) {
                console.error(e);
            }
        },
        [input, network]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value),
        [setInput]
    );

    return (
        <div>
            <h3>Event Subscription Tester</h3>
            this
            <form
                className={styles.form}
                onSubmit={handleSubmit}
                aria-label="event subscription form"
            >
                <input
                    className={styles.searchtextdesktop}
                    id="searchText"
                    placeholder="Subscribe to an event"
                    value={input}
                    onChange={handleTextChange}
                    autoFocus
                    type="text"
                />
                <input
                    className={styles.searchtextmobile}
                    id="searchText"
                    placeholder="Subscribe event"
                    value={input}
                    onChange={handleTextChange}
                    autoFocus
                    type="text"
                />
                <button
                    id="searchBtn"
                    type="submit"
                    disabled={pleaseWaitMode}
                    className={`${styles.button} ${
                        pleaseWaitMode && styles.disabled
                    }`}
                >
                    {pleaseWaitMode ? (
                        'Please Wait'
                    ) : (
                        <SearchIcon className={styles.searchicon} />
                    )}
                </button>
            </form>
        </div>
    );
}

export default EventSubscription;

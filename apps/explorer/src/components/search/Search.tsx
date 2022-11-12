// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useCallback, useContext } from 'react';

import { ReactComponent as SearchIcon } from '../../assets/search.svg';
import { NetworkContext } from '../../context';
import { navigateWithUnknown } from '../../utils/searchUtil';

import styles from './Search.module.css';

import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';

function Search() {
    const [input, setInput] = useState('');
    const [network] = useContext(NetworkContext);
    const navigate = useNavigateWithQuery();

    const [pleaseWaitMode, setPleaseWaitMode] = useState(false);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();
            // Prevent empty search
            if (!input.length) return;
            setPleaseWaitMode(true);

            // remove empty char from input
            let query = input.trim();

            navigateWithUnknown(query, navigate, network).then(() => {
                setInput('');
                setPleaseWaitMode(false);
            });
        },
        [input, navigate, setInput, network]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value),
        [setInput]
    );

    return (
        <form
            className={styles.form}
            onSubmit={handleSubmit}
            aria-label="search form"
        >
            <input
                className={styles.searchtextdesktop}
                id="searchText"
                data-testid="search"
                placeholder="Search by Addresses / Objects / Transactions"
                value={input}
                onChange={handleTextChange}
                autoFocus
                type="text"
            />
            <input
                className={styles.searchtextmobile}
                id="searchText"
                placeholder="Search Anything"
                value={input}
                onChange={handleTextChange}
                autoFocus
                type="text"
            />
            <button
                id="searchBtn"
                type="submit"
                disabled={pleaseWaitMode}
                className={`${styles.searchbtn} ${
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
    );
}

export default Search;

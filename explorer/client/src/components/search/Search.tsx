// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useCallback, useContext } from 'react';
import { useNavigate } from 'react-router-dom';

import { NetworkContext } from '../../context';
import { navigateWithUnknown } from '../../utils/searchUtil';

import styles from './Search.module.css';

function Search() {
    const [input, setInput] = useState('');
    const [network] = useContext(NetworkContext);
    const navigate = useNavigate();

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
                className={styles.searchtext}
                id="searchText"
                placeholder="Search by ID"
                value={input}
                onChange={handleTextChange}
                type="text"
            />

            <input
                type="submit"
                id="searchBtn"
                value={pleaseWaitMode ? 'Please Wait' : 'Search'}
                disabled={pleaseWaitMode}
                className={`${styles.searchbtn} ${
                    pleaseWaitMode && styles.disabled
                }`}
            />
        </form>
    );
}

export default Search;

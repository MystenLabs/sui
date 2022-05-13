// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { navigateWithUnknown } from '../../utils/searchUtil';

import styles from './Search.module.css';

type SearchCategory = 'all' | 'transactions' | 'addresses' | 'objects';
function getPlaceholderText(category: SearchCategory) {
    switch (category) {
        case 'addresses':
            return 'Search by address';
        case 'transactions':
            return 'Search by digest';
        case 'objects':
        case 'all':
            return 'Search by ID';
    }
}

function Search() {
    const [input, setInput] = useState('');
    const [category, setCategory] = useState('all' as SearchCategory);
    const navigate = useNavigate();

    const [pleaseWaitMode, setPleaseWaitMode] = useState(false);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();
            // Prevent empty search
            if (!input.length) return;
            setPleaseWaitMode(true);

            if (category === 'all') {
                // remove empty char from input
                navigateWithUnknown(input.trim(), navigate).then(() => {
                    setInput('');
                    setPleaseWaitMode(false);
                });
            } else {
                navigate(`../${category}/${input.trim()}`);
                setInput('');
                setPleaseWaitMode(false);
                setCategory('all');
            }
        },
        [input, navigate, category, setInput]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value),
        [setInput]
    );
    const handleCategoryChange = useCallback(
        (e: React.ChangeEvent<HTMLSelectElement>) =>
            setCategory(e.currentTarget.value as SearchCategory),
        [setCategory]
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
                placeholder={getPlaceholderText(category)}
                value={input}
                onChange={handleTextChange}
                type="text"
            />
            <select
                className={styles.categorydropdown}
                onChange={handleCategoryChange}
                value={category}
            >
                <option value="all">All</option>
                <option value="transactions">Transactions</option>
                <option value="objects">Objects</option>
                <option value="addresses">Addresses</option>
            </select>
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

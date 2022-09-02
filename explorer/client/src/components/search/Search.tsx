// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import React, { useState, useCallback, useContext, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { ReactComponent as SearchIcon } from '../../assets/search.svg';
import { NetworkContext } from '../../context';
import {
    navigateWithCategory,
    SEARCH_CATEGORIES,
} from '../../utils/searchUtil';
import { type ResultType } from './SearchResultType';
import SearchResults from './SearchResults';

import styles from './Search.module.css';

function Search() {
    const navigate = useNavigate();
    const [network] = useContext(NetworkContext);
    const [query, setQuery] = useState('');
    const [selectedResult, setSelectedResult] = useState<ResultType | null>(
        null
    );

    const [resultList, setResultList] = useState<ResultType[]>([]);

    // Whenever the query changes,
    // the list of results is updated with
    // data fetched from the network

    useEffect(() => {
        if (!query) {
            setResultList([]);
        } else if (navigateWithCategory) {
            Promise.all(
                SEARCH_CATEGORIES.map((category) =>
                    navigateWithCategory(query, category, network)
                )
            ).then((res) => {
                setResultList(res.filter((el) => el));
            });
        }
    }, [query, network]);

    // Handle query change
    const handleQueryChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setQuery(e.currentTarget.value),
        [setQuery]
    );

    const navigateToPage = useCallback(
        (selected: ResultType | null) => {
            if (selected) {
                navigate(
                    `../${selected.category}/${encodeURIComponent(
                        selected.input
                    )}`,
                    {
                        state: selected.result,
                    }
                );
                setQuery('');
                setSelectedResult(null);
            }
        },
        [navigate]
    );

    const handleClickSubmit = useCallback(() => {
        if (selectedResult) {
            navigateToPage(selectedResult);
        } else {
            navigateToPage(resultList[0]);
        }
    }, [selectedResult, navigateToPage, resultList]);

    useEffect(() => {
        navigateToPage(selectedResult);
    }, [selectedResult, navigateToPage]);

    return (
        <div className={styles.form}>
            <Combobox value={selectedResult} onChange={setSelectedResult}>
                <Combobox.Input
                    className={styles.searchtextdesktop}
                    id="searchText"
                    placeholder="Search by Addresses / Objects / Transactions"
                    onChange={handleQueryChange}
                    autoFocus
                    type="text"
                    autoComplete="off"
                />
                <Combobox.Input
                    className={styles.searchtextmobile}
                    id="searchText"
                    placeholder="Search Anything"
                    autoComplete="off"
                    onChange={handleQueryChange}
                />
                <SearchResults result={resultList} />
            </Combobox>
            <button
                id="searchBtn"
                type="submit"
                className={styles.searchbtn}
                onClick={handleClickSubmit}
            >
                <SearchIcon className={styles.searchicon} />
            </button>
        </div>
    );
}

export default Search;

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import React, {
    useState,
    useCallback,
    useContext,
    useRef,
    useEffect,
} from 'react';
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
    const wrapperref = useRef<HTMLDivElement>(null);
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

    // Whenever the list of results changes, we change the selected result

    useEffect(() => {
        resultList.length >= 1
            ? setSelectedResult(resultList[0])
            : setSelectedResult(null);
    }, [resultList]);

    // Handle query change
    const handleQueryChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setQuery(e.currentTarget.value),
        [setQuery]
    );

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();

            if (selectedResult) {
                navigate(
                    `../${selectedResult.category}/${encodeURIComponent(
                        selectedResult.input
                    )}`,
                    {
                        state: selectedResult.result,
                    }
                );
                setResultList([]);
            }
        },
        [navigate, selectedResult]
    );

    return (
        <div ref={wrapperref}>
            <form
                className={styles.form}
                onSubmit={handleSubmit}
                aria-label="search form"
            >
                <Combobox
                    value={selectedResult}
                    name="web search"
                    onChange={setSelectedResult}
                >
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
                >
                    <SearchIcon className={styles.searchicon} />
                </button>
            </form>
        </div>
    );
}

export default Search;

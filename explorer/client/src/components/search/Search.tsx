// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    const [input, setInput] = useState('');

    const [result, setResult] = useState<ResultType[] | null>(null);
    const [resultIndex, setResultIndex] = useState(0);

    const handleClickOutside = (event: MouseEvent): void => {
        if (
            wrapperref.current &&
            !wrapperref.current.contains(event.target as Node)
        ) {
            setResult(null);
            setInput('');
        }
    };

    const handleKeyPress = useCallback(
        (event: KeyboardEvent): void => {
            // Press Down Key or Tab
            if (event.keyCode === 40 || event.keyCode === 9) {
                event.preventDefault();
                setResultIndex((prevIndex) =>
                    result?.length && prevIndex < result.length - 1
                        ? prevIndex + 1
                        : 0
                );
            }

            // Press Up Key
            if (event.keyCode === 38) {
                event.preventDefault();
                setResultIndex((prevIndex) =>
                    prevIndex > 0
                        ? prevIndex - 1
                        : result?.length && result.length > 1
                        ? result.length - 1
                        : prevIndex
                );
            }
        },
        [result]
    );

    // Clicking Outside the Search Bar and Results should clear the search

    useEffect(() => {
        setResultIndex(0);
        document.addEventListener('click', handleClickOutside, false);
        document.addEventListener('keydown', handleKeyPress, false);

        return () => {
            document.removeEventListener('click', handleClickOutside, false);
            document.removeEventListener('keydown', handleKeyPress, false);
        };
    }, [handleKeyPress]);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();

            if (result?.length && result.length >= 1) {
                navigate(
                    `../${result[resultIndex].category}/${encodeURIComponent(
                        result[resultIndex].input
                    )}`,
                    {
                        state: result[resultIndex].result,
                    }
                );

                setResult(null);
                setInput('');
            }
        },
        [navigate, result, resultIndex]
    );
    const handleOptionClick = useCallback(
        (entry: ResultType) => () => {
            navigate(
                `../${entry.category}/${encodeURIComponent(entry.input)}`,
                {
                    state: entry.result,
                }
            );
            setResult(null);
            setInput('');
        },
        [navigate]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) => {
            setInput(e.currentTarget.value);
            if (!e.currentTarget.value) {
                setResult(null);
            } else {
                Promise.all(
                    SEARCH_CATEGORIES.map((category) =>
                        navigateWithCategory(
                            e.currentTarget.value.trim(),
                            category,
                            network
                        )
                    )
                ).then((res) => {
                    setResult(res.filter((el) => el));
                });
            }
        },
        [network]
    );

    return (
        <div ref={wrapperref}>
            <form
                className={styles.form}
                onSubmit={handleSubmit}
                aria-label="search form"
            >
                <input
                    className={styles.searchtextdesktop}
                    id="searchText"
                    placeholder="Search by Addresses / Objects / Transactions"
                    value={input}
                    onChange={handleTextChange}
                    autoFocus
                    type="text"
                    autoComplete="off"
                />
                <input
                    className={styles.searchtextmobile}
                    id="searchText"
                    placeholder="Search Anything"
                    value={input}
                    onChange={handleTextChange}
                    autoFocus
                    type="text"
                    autoComplete="off"
                />
                <button
                    id="searchBtn"
                    type="submit"
                    className={styles.searchbtn}
                >
                    <SearchIcon className={styles.searchicon} />
                </button>
            </form>
            <SearchResults
                result={result}
                resultIndex={resultIndex}
                setResultIndex={setResultIndex}
                optionClick={handleOptionClick}
            />
        </div>
    );
}

export default Search;

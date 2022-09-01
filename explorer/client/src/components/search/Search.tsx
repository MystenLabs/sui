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
import { Combobox } from '@headlessui/react';

import styles from './Search.module.css';

function Search() {
    const navigate = useNavigate();
    const wrapperref = useRef<HTMLDivElement>(null);
    const [network] = useContext(NetworkContext);
    const [input, setInput] = useState('');

    const [result, setResult] = useState<ResultType[] | null>(null);
    const [resultIndex, setResultIndex] = useState(0);

    const [resultOpen, setResultOpen] = useState(false);

    const handleClickOutside = (event: MouseEvent): void => {
        if (
            wrapperref.current &&
            !wrapperref.current.contains(event.target as Node)
        ) {
            setResultOpen(false);
        }
    };

    const handleFocus = useCallback(() => {
        if (!resultOpen) {
            setResultOpen(true);
        }
    }, [resultOpen]);

    const handleKeyPress = useCallback(
        (event: KeyboardEvent): void => {
            // If event already done, then do nothing
            if (event.defaultPrevented) {
                return;
            }

            // Press Down Key or Tab
            if (['ArrowDown', 'Down', 'Tab'].includes(event.key)) {
                event.preventDefault();
                setResultIndex((prevIndex) =>
                    result?.length && prevIndex < result.length - 1
                        ? prevIndex + 1
                        : 0
                );
            }

            // Press Up Key
            if (['ArrowUp', 'Up'].includes(event.key)) {
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

    // Whenever input changes, the result is updated with data fetched from the network

    useEffect(() => {
        if (!input) {
            setResult(null);
        } else if (navigateWithCategory) {
            Promise.all(
                SEARCH_CATEGORIES.map((category) =>
                    navigateWithCategory(input, category, network)
                )
            ).then((res) => {
                setResult(res.filter((el) => el));
            });
        }
    }, [input, network]);

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
            setInput(e.currentTarget.value.trim());
        },
        []
    );

    return (
        <div ref={wrapperref}>
            <form
                className={styles.form}
                onSubmit={handleSubmit}
                aria-label="search form"
            >
            <Combobox 
                    value={input}
      name="web search"
            >
                <Combobox.Input
                    className={styles.searchtextdesktop}
                    id="searchText"
                    placeholder="Search by Addresses / Objects / Transactions"
                    onChange={handleTextChange}
                    onFocus={handleFocus}
                    autoFocus
                    type="text"
                    autoComplete="off"
                />
                <Combobox.Input
                    className={styles.searchtextmobile}
                    id="searchText"
                    placeholder="Search Anything"
                    value={input}
                    onChange={handleTextChange}
                    onFocus={handleFocus}
                    autoFocus
                    type="text"
                    autoComplete="off"
                />
      <SearchResults
                    result={result}
                    resultIndex={resultIndex}
                    setResultIndex={setResultIndex}
                    optionClick={handleOptionClick}
                />

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

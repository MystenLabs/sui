// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useCallback, useContext } from 'react';
import { useNavigate } from 'react-router-dom';

import { ReactComponent as SearchIcon } from '../../assets/search.svg';
import { NetworkContext } from '../../context';
import {
    navigateWithCategory,
    SEARCH_CATEGORIES,
} from '../../utils/searchUtil';

import styles from './Search.module.css';

function Search() {
    const navigate = useNavigate();
    const [network] = useContext(NetworkContext);
    const [input, setInput] = useState('');

    const [result, setResult] = useState<
        | {
              input: string;
              category: typeof SEARCH_CATEGORIES[number];
              result: object | null;
          }[]
        | null
    >(null);

    const handleSubmit = useCallback(
        (e: React.FormEvent<HTMLFormElement>) => {
            e.preventDefault();

            if (result?.length === 1) {
                navigate(`../${result[0].category}/${result[0].input}`, {
                    state: result[0].result,
                });

                setResult(null);
                setInput('');
            }
        },
        [navigate, result]
    );

    const handleOptionClick = useCallback(
        (entry) => () => {
            navigate(`../${entry.category}/${entry.input}`, {
                state: entry.result,
            });
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
        <>
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
                    className={styles.searchbtn}
                >
                    <SearchIcon className={styles.searchicon} />
                </button>
            </form>
            {result && (
                <div>
                    {result.map((el, index) => (
                        <div key={index} onClick={handleOptionClick(el)}>
                            <h3>{el.category}</h3>
                            <p>{el.input}</p>
                        </div>
                    ))}
                </div>
            )}
        </>
    );
}

export default Search;

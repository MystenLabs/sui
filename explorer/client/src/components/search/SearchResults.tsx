// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ResultType } from './SearchResultType';

import styles from './SearchResults.module.css';

function SearchResults({
    result,
    resultIndex,
    setResultIndex,
    optionClick,
}: {
    result: ResultType[] | null;
    resultIndex: number;
    setResultIndex: (index: number) => void;
    optionClick: (el: ResultType) => () => void;
}) {
    const categoryLabels = {
        objects: 'object',
        transactions: 'transaction',
        addresses: 'address',
    };

    if (!result) return <></>;
    return (
        <div className={styles.results}>
            {result.length === 0 && (
                <p className={styles.noresults}>No Results</p>
            )}
            {result.map((el, index) => (
                <dl key={index}>
                    <dt>{categoryLabels[el.category]}</dt>
                    <dd
                        className={
                            index === resultIndex ? styles.selectedoption : ''
                        }
                        onClick={optionClick(el)}
                    >
                        {el.input}
                    </dd>
                </dl>
            ))}
        </div>
    );
}

export default SearchResults;

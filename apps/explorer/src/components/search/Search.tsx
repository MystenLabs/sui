// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useCallback } from 'react';

import { useSearch } from '~/hooks/useSearch';
import { Search as SearchBox } from '~/ui/Search';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';

function Search() {
    const [input, setInput] = useState('');
    const { isLoading, isError, data: results, error } = useSearch(input);

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value),
        [setInput]
    );

    const navigate = useNavigateWithQuery();
    const handleSelectResult = useCallback(
        (result: any) => {
            navigate(`/transaction/${encodeURIComponent(result.id)}`, {});
        },
        [navigate]
    );

    return (
        <div className="flex h-full w-[500px] flex-shrink-0 flex-col items-center">
            <SearchBox
                query={input}
                onChange={handleTextChange}
                onSelectResult={handleSelectResult}
                placeholder={
                    isLoading ? 'Loading...' : 'Search by whatever lol'
                }
                results={results}
            />
        </div>
    );
}

export default Search;

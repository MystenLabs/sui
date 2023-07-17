// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState, useCallback, useEffect } from 'react';

import { useDebouncedValue } from '~/hooks/useDebouncedValue';
import { useSearch } from '~/hooks/useSearch';
import { Search as SearchBox, type SearchResult } from '~/ui/Search';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';
import { ampli } from '~/utils/analytics/ampli';

function Search() {
	const [query, setQuery] = useState('');
	const debouncedQuery = useDebouncedValue(query);
	const { isLoading, data: results } = useSearch(debouncedQuery);
	const navigate = useNavigateWithQuery();
	const handleSelectResult = useCallback(
		(result: SearchResult) => {
			if (result) {
				ampli.clickedSearchResult({
					searchQuery: result.id,
					searchCategory: result.type,
				});
				navigate(`/${result?.type}/${encodeURIComponent(result?.id)}`, {});
				setQuery('');
			}
		},
		[navigate],
	);

	useEffect(() => {
		if (debouncedQuery) {
			ampli.completedSearch({
				searchQuery: debouncedQuery,
			});
		}
	}, [debouncedQuery]);

	return (
		<div className="max-w flex">
			<SearchBox
				queryValue={query}
				onChange={(value) => setQuery(value?.trim() ?? '')}
				onSelectResult={handleSelectResult}
				placeholder="Search"
				isLoading={isLoading || debouncedQuery !== query}
				options={results}
			/>
		</div>
	);
}

export default Search;

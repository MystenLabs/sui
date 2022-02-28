import useSWR from 'swr';

import { request } from './Api';

function useSearch(term: string | null) {
    return useSWR(term ? `GET search/${term}` : null, () =>
        request({ path: `search/${term}` as 'search/{term}', method: 'get' })
    );
}

export default useSearch;

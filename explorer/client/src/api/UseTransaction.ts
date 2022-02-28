import useSWR from 'swr';

import { request } from './Api';

function useTransaction(id: string | null) {
    return useSWR(id ? `GET transactions/${id}` : null, () =>
        request({
            path: `transactions/${id}` as 'transactions/{id}',
            method: 'get',
        })
    );
}

export default useTransaction;

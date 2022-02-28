import useSWR from 'swr';

import { request } from './Api';

function useTransactions() {
    return useSWR(`GET transactions`, () =>
        request({
            path: `transactions`,
            method: 'get',
        })
    );
}

export default useTransactions;

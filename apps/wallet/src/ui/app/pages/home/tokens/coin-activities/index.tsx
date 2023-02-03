// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CoinActivityCard } from './CoinActivityCard';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { useAppSelector, useGetTransactionsByAddress } from '_hooks';
import Alert from '_src/ui/app/components/alert';

export function CoinActivities({ coinType }: { coinType: string }) {
    const activeAddress = useAppSelector(({ account: { address } }) => address);
    const {
        data: txns,
        isLoading,
        isError,
        error,
    } = useGetTransactionsByAddress(activeAddress);

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="font-semibold">
                        {error?.message || 'Something went wrong'}
                    </div>
                </Alert>
            </div>
        );
    }
    return (
        <div className="flex flex-col flex-nowrap ">
            <div className="flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
                <Loading
                    loading={isLoading}
                    className="flex justify-center items-center h-full"
                >
                    {txns?.length && activeAddress
                        ? txns.map((txn) => (
                              <ErrorBoundary
                                  key={txn.certificate.transactionDigest}
                              >
                                  <CoinActivityCard
                                      txn={txn}
                                      activeCoinType={coinType}
                                      address={activeAddress}
                                  />
                              </ErrorBoundary>
                          ))
                        : null}
                </Loading>
            </div>
        </div>
    );
}

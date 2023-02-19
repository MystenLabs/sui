// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import { useAppSelector, useObjectsOwnedByAddress } from '_hooks';
import PageTitle from '_src/ui/app/shared/PageTitle';

function NftsPage() {
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading } = useObjectsOwnedByAddress(accountAddress);
    const nfts = useMemo(
        () => data?.filter((obj) => !Coin.isCoin(obj)),
        [data]
    );

    return (
        <div className="flex flex-col flex-nowrap items-center gap-4 flex-1">
            <PageTitle title="NFTs" />
            <Loading loading={isLoading}>
                {nfts ? (
                    <div className="grid grid-cols-2 gap-x-3.5 gap-y-4 w-full">
                        {nfts.map(({ objectId }) => (
                            <Link
                                to={`/nft-details?${new URLSearchParams({
                                    objectId,
                                }).toString()}`}
                                key={objectId}
                                className="no-underline"
                            >
                                <ErrorBoundary>
                                    <NFTDisplayCard
                                        objectId={objectId}
                                        size="md"
                                        showlabel
                                        animateHover
                                        borderRadius="xl"
                                    />
                                </ErrorBoundary>
                            </Link>
                        ))}
                    </div>
                ) : (
                    <div className="text-steel-darker font-semibold text-caption flex-1 self-center flex items-center">
                        No NFTs found
                    </div>
                )}
            </Loading>
        </div>
    );
}

export default NftsPage;

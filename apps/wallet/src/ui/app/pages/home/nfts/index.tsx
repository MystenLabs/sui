// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { Link } from 'react-router-dom';

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import { useObjectsOwnedByAddress } from '_hooks';
import PageTitle from '_src/ui/app/shared/PageTitle';

import type { SuiObjectResponse, SuiObjectData } from '@mysten/sui.js';

function NftsPage() {
    const accountAddress = useActiveAddress();
    const { data, isLoading, error, isError } = useObjectsOwnedByAddress(
        accountAddress,
        { options: { showType: true } }
    );
    const sui_object_responses = data?.data as SuiObjectResponse[];
    const nft_objects = sui_object_responses?.filter(
        (obj) => !Coin.isCoin(obj)
    );
    const nfts = nft_objects?.map((nft) => {
        const nft_details = nft.details as SuiObjectData;
        return nft_details;
    });

    return (
        <div className="flex flex-col flex-nowrap items-center gap-4 flex-1">
            <PageTitle title="NFTs" />
            <Loading loading={isLoading}>
                {isError ? (
                    <Alert>
                        <div>
                            <strong>Sync error (data might be outdated)</strong>
                        </div>
                        <small>{(error as Error).message}</small>
                    </Alert>
                ) : null}
                {nfts?.length ? (
                    <div className="grid grid-cols-2 gap-x-3.5 gap-y-4 w-full h-full">
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
                                        showLabel
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

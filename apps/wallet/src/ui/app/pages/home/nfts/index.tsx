// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectDisplay, type SuiObjectData } from '@mysten/sui.js';
import { Link } from 'react-router-dom';

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Alert from '_components/alert';
import { ErrorBoundary } from '_components/error-boundary';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import { useObjectsOwnedByAddress } from '_hooks';
import PageTitle from '_src/ui/app/shared/PageTitle';

function NftsPage() {
    const accountAddress = useActiveAddress();
    const { data, isLoading, error, isError } = useObjectsOwnedByAddress(
        accountAddress,
        { options: { showType: true, showDisplay: true } }
    );
    const nfts = data?.data
        ?.filter((resp) => !!getObjectDisplay(resp).data)
        .map(({ data }) => data as SuiObjectData);
    return (
        <div className="flex flex-1 flex-col flex-nowrap items-center gap-4">
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
                    <div className="grid w-full grid-cols-2 gap-x-3.5 gap-y-4">
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
                    <div className="flex flex-1 items-center self-center text-caption font-semibold text-steel-darker">
                        No NFTs found
                    </div>
                )}
            </Loading>
        </div>
    );
}

export default NftsPage;

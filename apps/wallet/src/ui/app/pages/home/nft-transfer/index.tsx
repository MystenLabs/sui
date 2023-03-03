// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectId,
    hasPublicTransfer,
    is,
    SuiObject,
    getObjectOwner,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { TransferNFTForm } from './TransferNFTForm';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import Overlay from '_components/overlay';
import { useAppSelector, useGetObject } from '_hooks';

function NftTransferPage() {
    const { nftId } = useParams();
    const address = useAppSelector(({ account: { address } }) => address);

    // verify that the nft is owned by the user and is transferable
    const { data: objectData, isLoading } = useGetObject(nftId!);

    const selectedNft = useMemo(() => {
        if (
            !objectData ||
            !is(objectData.details, SuiObject) ||
            !hasPublicTransfer(objectData.details)
        )
            return null;
        const owner = getObjectOwner(objectData) as { AddressOwner: string };
        return owner.AddressOwner === address ? objectData.details : null;
    }, [address, objectData]);

    const objectId = selectedNft ? getObjectId(selectedNft.reference) : null;
    const navigate = useNavigate();

    return (
        <Overlay
            showModal={true}
            title="Send NFT"
            closeOverlay={() => navigate('/nfts')}
        >
            <div className="flex w-full flex-col h-full">
                <Loading loading={isLoading}>
                    {objectId && nftId ? (
                        <>
                            <div className="mb-7.5">
                                <NFTDisplayCard
                                    objectId={nftId}
                                    wideView
                                    size="sm"
                                />
                            </div>
                            <TransferNFTForm objectId={objectId} />
                        </>
                    ) : (
                        <Navigate to="/" replace />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}

export default NftTransferPage;

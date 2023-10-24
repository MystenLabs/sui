// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import Overlay from '_components/overlay';
import { useOwnedNFT } from '_hooks';
import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { TransferNFTForm } from './TransferNFTForm';

function NftTransferPage() {
	const { nftId } = useParams();
	const address = useActiveAddress();
	// verify that the nft is owned by the user and is transferable
	const { data: ownedNFT, isPending: isNftLoading } = useOwnedNFT(nftId || '', address);
	const navigate = useNavigate();
	const isGuardLoading = useUnlockedGuard();
	const isPending = isNftLoading || isGuardLoading;
	return (
		<Overlay showModal={true} title="Send NFT" closeOverlay={() => navigate('/nfts')}>
			<div className="flex w-full flex-col h-full">
				<Loading loading={isPending}>
					{ownedNFT &&
					nftId &&
					ownedNFT.content?.dataType === 'moveObject' &&
					ownedNFT.content.hasPublicTransfer ? (
						<>
							<div className="mb-7.5">
								<NFTDisplayCard objectId={nftId} wideView size="sm" />
							</div>
							<TransferNFTForm objectId={nftId} objectType={ownedNFT.type} />
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

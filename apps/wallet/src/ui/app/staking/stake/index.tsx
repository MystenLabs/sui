// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSearchParams, useNavigate } from 'react-router-dom';

import StakingCard from './StakingCard';
import { SelectValidatorCard } from '../validators/SelectValidatorCard';
import Overlay from '_components/overlay';

function StakePage() {
	const [searchParams] = useSearchParams();
	const validatorAddress = searchParams.get('address');
	const unstake = searchParams.get('unstake') === 'true';

	const navigate = useNavigate();
	const stakingTitle = unstake ? 'Unstake SUI' : 'Stake SUI';

	return (
		<Overlay
			showModal={true}
			title={validatorAddress ? stakingTitle : 'Select a Validator'}
			closeOverlay={() => navigate('/')}
		>
			{validatorAddress ? <StakingCard /> : <SelectValidatorCard />}
		</Overlay>
	);
}

export default StakePage;

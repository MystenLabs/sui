// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetTransaction } from '~/hooks/useGetTransaction';
import { trimStdLibPrefix } from '~/utils/stringUtils';
import { getOwnerStr } from '~/utils/objectUtils';
import { type DataType } from '~/pages/object-result/ObjectResultType';

export const GENESIS_TX_DIGEST = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';

export function usePackageViewedData({ data }: { data: DataType }) {
	const { data: txnData } = useGetTransaction(data.data.tx_digest!);

	return {
		...data,
		objType: trimStdLibPrefix(data.objType),
		tx_digest: data.data.tx_digest,
		owner: getOwnerStr(data.owner),
		publisherAddress:
			data.data.tx_digest === GENESIS_TX_DIGEST ? 'Genesis' : txnData?.transaction?.data.sender,
	};
}

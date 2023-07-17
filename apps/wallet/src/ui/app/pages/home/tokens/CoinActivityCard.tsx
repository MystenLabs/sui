// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function CoinActivitiesCard({ coinType }: { coinType: string }) {
	// TODO: Re-build this with Programmable Transactions
	// const {
	//     data: txns,
	//     isLoading,
	//     error,
	//     isError,
	// } = useQueryTransactionsByAddress(activeAddress);

	// filter txns by coinType
	// const txnByCoinType = useMemo(() => {
	//     if (!txns || !activeAddress) return null;
	//     return [];
	// }, [txns, activeAddress]);

	// if (isError) {
	//     return (
	//         <div className="p-2">
	//             <Alert mode="warning">
	//                 <div className="font-semibold">
	//                     {(error as Error).message}
	//                 </div>
	//             </Alert>
	//         </div>
	//     );
	// }

	return (
		<div className="flex flex-col flex-nowrap ">
			<div className="flex-grow overflow-y-auto px-5 -mx-5 divide-y divide-solid divide-gray-45 divide-x-0">
				{/* <Loading loading={isLoading}>
                    {txnByCoinType?.length && activeAddress
                        ? txnByCoinType.map((txn) => (
                              <ErrorBoundary key={getTransactionDigest(txn)}>
                                  <TransactionCard
                                      txn={txn}
                                      address={activeAddress}
                                  />
                              </ErrorBoundary>
                          ))
                        : null}
                </Loading> */}
			</div>
		</div>
	);
}

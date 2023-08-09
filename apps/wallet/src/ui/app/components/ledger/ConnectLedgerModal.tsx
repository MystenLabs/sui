// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { Link } from '../../shared/Link';
import { ampli } from '_src/shared/analytics/ampli';
import { useSuiLedgerClient } from '_src/ui/app/components/ledger/SuiLedgerClientProvider';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { ModalDialog } from '_src/ui/app/shared/ModalDialog';
import { Text } from '_src/ui/app/shared/text';

type ConnectLedgerModalProps = {
	onClose: () => void;
	onConfirm: () => void;
	onError: (error: unknown) => void;
};

export function ConnectLedgerModal({ onClose, onConfirm, onError }: ConnectLedgerModalProps) {
	const [isConnectingToLedger, setConnectingToLedger] = useState(false);
	const { connectToLedger } = useSuiLedgerClient();

	const onContinueClick = async () => {
		try {
			setConnectingToLedger(true);
			await connectToLedger(true);
			onConfirm();
		} catch (error) {
			onError(error);
		} finally {
			setConnectingToLedger(false);
		}
	};

	return (
		<ModalDialog
			isOpen
			title="Connect Ledger Wallet"
			onClose={onClose}
			body={
				<div className="flex flex-col items-center">
					<div className="mt-4.5">
						<LedgerLogo />
					</div>
					<div className="break-words text-center mt-4.5">
						<Text variant="pBodySmall" color="steel-darker" weight="normal">
							Connect your ledger to your computer, unlock it, and launch the Sui app. Click
							Continue when done.
						</Text>
						<div className="flex items-center justify-center mt-2">
							<Text variant="pBodySmall" color="steel-dark" weight="normal">
								Need more help?&nbsp;
							</Text>
							<span>
								<Link
									underline="hover"
									href="https://support.ledger.com/hc/articles/10136570195101"
									onClick={() => ampli.viewedLedgerTutorial()}
									text="View tutorial."
									color="heroDark"
								/>
							</span>
						</div>
					</div>
				</div>
			}
			footer={
				<div className="w-full flex flex-row self-center gap-3">
					<Button variant="outline" size="tall" text="Cancel" onClick={onClose} />
					<Button
						variant="outline"
						size="tall"
						text="Continue"
						onClick={onContinueClick}
						loading={isConnectingToLedger}
					/>
				</div>
			}
		/>
	);
}

// TODO: We should probably use a loader like @svgr/webpack so that we can provide SVG files
// and have them be automatically importable in React components. From playing around with
// this, there seems to be an issue where TypeScript bindings aren't correctly generated
// (see https://github.com/gregberge/svgr/pull/573)
function LedgerLogo() {
	return (
		<svg
			width="144"
			height="48"
			viewBox="0 0 144 48"
			fill="none"
			xmlns="http://www.w3.org/2000/svg"
		>
			<path
				d="M123.049 44.9775V47.9993H143.812V34.3706H140.787V44.9775H123.049ZM123.049 0V3.02191H140.787V13.6294H143.812V0H123.049ZM112.341 23.3779V16.3559H117.087C119.401 16.3559 120.231 17.1261 120.231 19.2301V20.4743C120.231 22.6371 119.43 23.3779 117.087 23.3779H112.341ZM119.875 24.6221C122.04 24.0592 123.552 22.0441 123.552 19.6446C123.552 18.1336 122.96 16.7704 121.832 15.674C120.409 14.3107 118.51 13.6294 116.048 13.6294H109.374V34.3698H112.341V26.1036H116.79C119.074 26.1036 119.994 27.0517 119.994 29.4225V34.3706H123.019V29.8965C123.019 26.6372 122.248 25.393 119.875 25.0373V24.6221ZM94.8999 25.3034H104.036V22.5776H94.8999V16.3552H104.925V13.6294H91.874V34.3698H105.371V31.6441H94.8999V25.3034ZM84.9624 26.3998V27.8218C84.9624 30.8144 83.8647 31.7925 81.1066 31.7925H80.4541C77.6949 31.7925 76.3602 30.9033 76.3602 26.7849V21.2144C76.3602 17.0666 77.7545 16.2068 80.513 16.2068H81.1059C83.8051 16.2068 84.6654 17.2143 84.6946 19.9996H87.9575C87.6612 15.9106 84.9324 13.3333 80.8389 13.3333C78.8517 13.3333 77.1905 13.9557 75.9447 15.1404C74.0761 16.8888 73.0377 19.8519 73.0377 23.9997C73.0377 27.9997 73.928 30.9628 75.7666 32.7993C77.0124 34.0141 78.7332 34.666 80.4237 34.666C82.2035 34.666 83.8355 33.9546 84.6654 32.4143H85.0801V34.3698H87.809V23.6741H79.7705V26.3998H84.9624ZM58.8009 16.3552H62.0345C65.09 16.3552 66.7512 17.1254 66.7512 21.2739V26.7254C66.7512 30.8732 65.09 31.6441 62.0345 31.6441H58.8009V16.3552ZM62.3007 34.3706C67.9666 34.3706 70.0722 30.0743 70.0722 24.0004C70.0722 17.8375 67.8177 13.6302 62.2411 13.6302H55.8339V34.3706H62.3007ZM41.5077 25.3034H50.6439V22.5776H41.5077V16.3552H51.5334V13.6294H38.4815V34.3698H51.9785V31.6441H41.5077V25.3034ZM24.007 13.6294H20.9817V34.3698H34.6264V31.6441H24.007V13.6294ZM0.187988 34.3706V48H20.9516V44.9775H3.21329V34.3706H0.187988ZM0.187988 0V13.6294H3.21329V3.02191H20.9516V0H0.187988Z"
				fill="black"
			/>
		</svg>
	);
}

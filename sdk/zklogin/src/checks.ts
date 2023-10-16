// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JSONProcessor } from './jsonprocessor.js';
import { NONCE_LENGTH } from './nonce.js';
import { MAX_AUD_VALUE_LENGTH, MAX_KEY_CLAIM_VALUE_LENGTH } from './utils.js';

const MAX_HEADER_LEN_B64 = 248;
const MAX_PADDED_UNSIGNED_JWT_LEN = 64 * 25;
const MAX_EXTENDED_KEY_CLAIM_LEN = 126;
const MAX_EXTENDED_EV_LEN = 53;
const MAX_EXTENDED_NONCE_LEN = 44;
const MAX_EXTENDED_AUD_LEN = 160;
const MAX_EXTENDED_ISS_LEN_B64 = 224;

export function lengthChecks(
	header: string,
	payload: string,
	keyClaimName: string,
	processor: JSONProcessor,
) {
	/// Is the header length small enough
	const header_len = header.length;
	if (header_len > MAX_HEADER_LEN_B64) {
		throw new Error(`Header is too long`);
	}

	/// Is the combined length of the header and payload small enough
	const unsigned_jwt = header + '.' + payload;
	const L = unsigned_jwt.length * 8;
	const K = (512 + 448 - ((L % 512) + 1)) % 512;
	if ((L + 1 + K + 64) % 512 !== 0) {
		throw new Error('This should never happen');
	}

	// The SHA2 padding is 1 followed by K zeros, followed by the length of the message
	const padded_unsigned_jwt_len = (L + 1 + K + 64) / 8;

	// The padded unsigned JWT must be less than the max_padded_unsigned_jwt_len
	if (padded_unsigned_jwt_len > MAX_PADDED_UNSIGNED_JWT_LEN) {
		throw new Error(`The JWT is too long`);
	}

	const keyClaimDetails = processor.process(keyClaimName); // throws an error if key claim name is not found
	const keyClaimValue = processor.getRawClaimValue(keyClaimName);
	const keyClaimValueLen = keyClaimValue.length;
	if (keyClaimValueLen > MAX_KEY_CLAIM_VALUE_LENGTH) {
		throw new Error('Key claim value is too long');
	}
	// Note: Key claim name length is being checked in genAddressSeed.

	/// Are the extended claims small enough (key claim, email_verified)
	const extendedKeyClaimLen = keyClaimDetails.ext_claim.length;
	if (extendedKeyClaimLen > MAX_EXTENDED_KEY_CLAIM_LEN) {
		throw new Error(`Extended key claim length is too long`);
	}

	if (keyClaimName === 'email') {
		const evClaimDetails = processor.process('email_verified');
		const value = evClaimDetails.value;
		if (!(value === true || value === 'true')) {
			throw new Error(`Unexpected email_verified claim value ${value}`);
		}
		const extEVClaimLen = evClaimDetails.ext_claim.length;
		if (extEVClaimLen > MAX_EXTENDED_EV_LEN) {
			throw new Error('Extended email_verified claim length is too long');
		}
	}

	/// Check that nonce extended nonce length is as expected.
	const nonce_claim_details = processor.process('nonce');
	const nonce_value_len = nonce_claim_details.offsets.value_length;
	const NONCE_LEN_WITH_QUOTES = NONCE_LENGTH + 2;
	if (nonce_value_len !== NONCE_LEN_WITH_QUOTES) {
		throw new Error(`Nonce value length is not ${NONCE_LEN_WITH_QUOTES}`);
	}
	const extended_nonce_claim_len = nonce_claim_details.ext_claim.length;
	if (extended_nonce_claim_len < 38) {
		throw new Error(`Extended nonce claim is too short`);
	}
	if (extended_nonce_claim_len > MAX_EXTENDED_NONCE_LEN) {
		throw new Error('Extended nonce claim is too long');
	}

	/// 5. Check if aud value is small enough
	const aud_claim_details = processor.process('aud');
	const aud_value = processor.getRawClaimValue('aud');
	const aud_value_len = aud_value.length;
	if (aud_value_len > MAX_AUD_VALUE_LENGTH) {
		throw new Error(`aud is too long`);
	}

	const extended_aud_claim_len = aud_claim_details.ext_claim.length;
	if (extended_aud_claim_len > MAX_EXTENDED_AUD_LEN) {
		throw new Error(`Extended aud is too long`);
	}

	/// 6. Check if iss is small enough
	const iss_claim_details = processor.process('iss');
	// A close upper bound of the length of the extended iss claim (in base64)
	const iss_claim_len_b64 = 4 * (1 + Math.floor(iss_claim_details.offsets.ext_length / 3));
	if (iss_claim_len_b64 > MAX_EXTENDED_ISS_LEN_B64) {
		throw new Error(`Extended iss is too long`);
	}
}

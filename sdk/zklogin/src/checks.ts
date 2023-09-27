// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JSONVisitor, ParseError, ParseErrorCode, visit } from 'jsonc-parser';

import { MAX_AUD_VALUE_LENGTH, MAX_KEY_CLAIM_VALUE_LENGTH } from './utils.js';

const MAX_HEADER_LEN_B64 = 248;
const MAX_PADDED_UNSIGNED_JWT_LEN = 64 * 25;
const MAX_EXTENDED_KEY_CLAIM_LEN = 126;
const MAX_EXTENDED_EV_LEN = 53;
const MAX_EXTENDED_NONCE_LEN = 44;
const MAX_EXTENDED_AUD_LEN = 160;
const MAX_EXTENDED_ISS_LEN_B64 = 224;

// JSON parsing code inspired from https://github.com/microsoft/node-jsonc-parser/blob/main/src/test/json.test.ts#L69
interface VisitorCallback {
	id: keyof JSONVisitor;
	offset: number;
	length: number;
	// Not expecting any claim that is not a string or a boolean (boolean for email_verified)...
	arg?: string | boolean;
}

interface VisitorError extends ParseError {
	startLine: number;
	startCharacter: number;
}

export interface ClaimDetails {
	name: string; // e.g., "sub"
	// Not expecting any claim that is not a string or a boolean (boolean for email_verified)...
	value: string | boolean; // e.g., "1234567890"
	ext_claim: string; // e.g., "sub": "1234567890",
	offsets: {
		start: number; // start index
		colon: number; // index of the colon (within the ext_claim)
		value: number; // index of the value (within the ext_claim)
		value_length: number; // length of the value
		length: number; // ext_claim.length
	};
}

export class JSONProcessor {
	decoded_payload: string;
	processed: Record<string, ClaimDetails>;
	events: VisitorCallback[];

	constructor(decoded_payload: string) {
		this.decoded_payload = decoded_payload;
		this.events = this.visit();
		this.processed = {};
	}

	visit(): VisitorCallback[] {
		const errors: VisitorError[] = [];
		const actuals: VisitorCallback[] = [];
		const noArgHolder = (id: keyof JSONVisitor) => (offset: number, length: number) =>
			actuals.push({ id, offset, length });
		const oneArgHolder =
			(id: keyof JSONVisitor) => (arg: string | boolean, offset: number, length: number) =>
				actuals.push({ id, offset, length, arg });
		visit(this.decoded_payload, {
			onObjectProperty: oneArgHolder('onObjectProperty'),
			onObjectEnd: noArgHolder('onObjectEnd'),
			onLiteralValue: oneArgHolder('onLiteralValue'),
			onSeparator: oneArgHolder('onSeparator'), // triggers on both : and ,
			onError: (
				error: ParseErrorCode,
				offset: number,
				length: number,
				startLine: number,
				startCharacter: number,
			) => {
				errors.push({ error, offset, length, startLine, startCharacter });
			},
		});
		if (errors.length > 0) {
			throw new Error(`Parse errors encountered ${JSON.stringify(errors)}`);
		}
		return actuals;
	}

	process(name: string): ClaimDetails {
		if (Object.prototype.hasOwnProperty.call(this.processed, name)) {
			return this.processed[name];
		}

		const name_event_idx = this.events.findIndex(
			(e) => e.id === 'onObjectProperty' && e.arg === name,
		);
		if (name_event_idx === -1) {
			throw new Error('Claim ' + name + ' not found in ' + this.decoded_payload);
		}

		const name_event = this.events[name_event_idx];

		const colon_event_idx = name_event_idx + 1;
		const colon_event = this.events[colon_event_idx];
		if (
			this.events[colon_event_idx].id !== 'onSeparator' ||
			this.events[colon_event_idx].arg !== ':'
		) {
			throw new Error(`Unexpected error: Colon not found. 
                             Event: ${JSON.stringify(colon_event)}`);
		}

		const value_event_idx = colon_event_idx + 1;
		const value_event = this.events[value_event_idx];
		if (value_event.id !== 'onLiteralValue') {
			throw new Error(`Unexpected error: Unexpected value. 
                             Event: ${JSON.stringify(value_event)}`);
		}

		const ext_claim_end_event_idx = value_event_idx + 1;
		const ext_claim_end_event = this.events[ext_claim_end_event_idx];
		if (ext_claim_end_event.id !== 'onSeparator' && ext_claim_end_event.id !== 'onObjectEnd') {
			throw new Error(`Unexpected error: Unexpected ext_claim_end_event. 
                             Event: ${JSON.stringify(ext_claim_end_event)}`);
		}

		if (value_event.arg === undefined) {
			throw new Error(`Unexpected error: Undefined value_event.arg?. 
                             Event: ${JSON.stringify(value_event)}`);
		}
		this.processed[name] = {
			name: name,
			value: value_event.arg,
			ext_claim: this.decoded_payload.slice(name_event.offset, ext_claim_end_event.offset + 1),
			offsets: {
				start: name_event.offset,
				colon: colon_event.offset - name_event.offset,
				value: value_event.offset - name_event.offset,
				value_length: value_event.length,
				length: ext_claim_end_event.offset - name_event.offset + 1,
			},
		};

		// The number of whitespaces is equal to the length of extended claim
		//  minus the length of the value minus the length of the name minus 2.
		//  (2 for the colon and either comma or a close brace)
		const num_whitespaces =
			this.processed[name].offsets.length - value_event.length - name_event.length - 2;
		if (num_whitespaces > 0) {
            // TODO: This is an interesting event to note.
            //       Note sure if the console.info is the right way to log it.
			console.info(`[Rare event] Non-zero whitespace detected: 
                          Claim ${name} has ${num_whitespaces} whitespaces`);
		}

		return this.processed[name];
	}

	/**
	 * Returns the claim value exactly as it appears in the JWT.
	 * So, if it has escapes, no unescaping is done.
	 * Assumes that the claim value is a string.
	 * (reasonable as aud and common key claims like sub, email and username are JSON strings)
	 *
	 * @param name claim name
	 * @returns claim value as it appears in the JWT without the quotes. The quotes are omitted to faciliate address derivation.
	 *
	 * NOTE: This function is only used to obtain claim values for address generation.
	 * Do not use it elsewhere unless you know what you're doing.
	 */
	getRawClaimValue(name: string): string {
		if (!Object.prototype.hasOwnProperty.call(this.processed, name)) {
			throw new Error('Claim ' + name + ' not processed');
		}
		const details = this.processed[name];

		const value_index = details.offsets.value + details.offsets.start;
		const value_length = details.offsets.value_length;
		if (this.decoded_payload[value_index] !== '"') {
			throw new Error(
				`Claim ${name} does not have a string value. Details: ${JSON.stringify(details)}`,
			);
		}
		if (this.decoded_payload[value_index + value_length - 1] !== '"') {
			throw new Error(
				`Claim ${name} does not have a string value. Details: ${JSON.stringify(details)}`,
			);
		}

		const raw_value = this.decoded_payload.slice(value_index + 1, value_index + value_length - 1); // omit the quotes
		if (raw_value !== details.value) {
            // TODO: This is an interesting event to note.
            //       Note sure if the console.info is the right way to log it.
			console.info(
				`Claim value ${raw_value} of length ${
					raw_value.length
				} has escapes. Details: ${JSON.stringify(details)}`,
			);
		}
		return raw_value;
	}
}

export function lengthChecks(header: string, payload: string, keyClaimName: string, processor: JSONProcessor) {
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
    if (nonce_value_len !== 27) {
        throw new Error(`Nonce value length is not 27`);
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
	const iss_claim_len_b64 = 4 * (1 + Math.floor(iss_claim_details.offsets.length / 3));
	if (iss_claim_len_b64 > MAX_EXTENDED_ISS_LEN_B64) {
		throw new Error(`Extended iss is too long`);
	}
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { JSONVisitor, ParseError, ParseErrorCode } from 'jsonc-parser';
import { visit } from 'jsonc-parser';

// JSON parsing code inspired from https://github.com/microsoft/node-jsonc-parser/blob/main/src/test/json.test.ts#L69
interface VisitorCallback {
	id: keyof JSONVisitor;
	offset: number;
	length: number;
	// Not expecting any claim that is not a string or a boolean (email_verified is sometimes a boolean).
	// Ensuring that key claim and aud are strings is done in getRawClaimValue
	arg?: string | boolean | number;
}

interface VisitorError extends ParseError {
	startLine: number;
	startCharacter: number;
}

export interface ClaimDetails {
	name: string; // e.g., "sub"
	// Not expecting any claim that is not a string or a boolean (boolean for email_verified)...
	value: string | boolean | number; // e.g., "1234567890"
	ext_claim: string; // e.g., "sub": "1234567890",
	offsets: {
		start: number; // start index
		colon: number; // index of the colon (within the ext_claim)
		value: number; // index of the value (within the ext_claim)
		value_length: number; // length of the value
		name_length: number; // length of the name
		ext_length: number; // ext_claim.length
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
		visit(
			this.decoded_payload,
			{
				onObjectBegin: noArgHolder('onObjectBegin'),
				onObjectProperty: oneArgHolder('onObjectProperty'),
				onObjectEnd: noArgHolder('onObjectEnd'),
				onLiteralValue: oneArgHolder('onLiteralValue'),
				onSeparator: oneArgHolder('onSeparator'), // triggers on both : and ,
				onArrayBegin: noArgHolder('onArrayBegin'),
				// Of all the events, the ones that we do not listen to are
				//  onArrayEnd (as onArrayBegin allows us to throw errors if arrays are seen)
				//  and onComment (as we disallow comments anyway)
				onError: (
					error: ParseErrorCode,
					offset: number,
					length: number,
					startLine: number,
					startCharacter: number,
				) => {
					errors.push({ error, offset, length, startLine, startCharacter });
				},
			},
			{
				disallowComments: true,
			},
		);
		if (errors.length > 0) {
			console.error(JSON.stringify(errors));
			throw new Error(`Parse errors encountered`);
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
			throw new Error('Claim ' + name + ' not found');
		}

		const name_event = this.events[name_event_idx];

		const colon_event_idx = name_event_idx + 1;
		const colon_event = this.events[colon_event_idx];
		if (
			this.events[colon_event_idx].id !== 'onSeparator' ||
			this.events[colon_event_idx].arg !== ':'
		) {
			throw new Error(`Unexpected error: Colon not found`);
		}

		const value_event_idx = colon_event_idx + 1;
		const value_event = this.events[value_event_idx];
		if (value_event.id !== 'onLiteralValue') {
			throw new Error(`Unexpected JSON value type: ${value_event.id}`);
		}

		const ext_claim_end_event_idx = value_event_idx + 1;
		const ext_claim_end_event = this.events[ext_claim_end_event_idx];
		if (ext_claim_end_event.id !== 'onSeparator' && ext_claim_end_event.id !== 'onObjectEnd') {
			throw new Error(`Unexpected ext_claim_end_event ${ext_claim_end_event.id}`);
		}

		if (value_event.arg === undefined) {
			throw new Error(`Undefined type for ${name}`);
		}
		if (
			typeof value_event.arg !== 'string' &&
			typeof value_event.arg !== 'boolean' &&
			typeof value_event.arg !== 'number'
		) {
			throw new Error(`Unexpected type for ${name}: ${typeof value_event.arg}`);
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
				name_length: name_event.length,
				ext_length: ext_claim_end_event.offset - name_event.offset + 1,
			},
		};
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
		if (typeof details.value !== 'string') {
			throw new Error(`Claim ${name} does not have a string value.`);
		}

		const value_index = details.offsets.value + details.offsets.start;
		const value_length = details.offsets.value_length;
		if (this.decoded_payload[value_index] !== '"') {
			throw new Error(`Claim ${name} does not have a string value.`);
		}
		if (this.decoded_payload[value_index + value_length - 1] !== '"') {
			throw new Error(`Claim ${name} does not have a string value.`);
		}

		const raw_value = this.decoded_payload.slice(value_index + 1, value_index + value_length - 1); // omit the quotes
		return raw_value;
	}
}

import { sleep } from 'k6';
import { SharedArray } from 'k6/data';
import http from 'k6/http';
import { Counter } from 'k6/metrics';

// Custom metrics for JSON-RPC errors
const jsonRpcErrors = new Counter('jsonrpc_errors');
const jsonRpcErrorsByCode = new Counter('jsonrpc_errors_by_code');

// ================================
// Configuration from envs
// ================================
const CONFIG = {
	endpoint: __ENV.ENDPOINT || 'http://localhost:9000',
	concurrency: parseInt(__ENV.CONCURRENCY) || 1,
	duration: __ENV.DURATION || '120s',
	requestsFile: __ENV.REQUESTS_FILE || './requests.jsonl',
	methodsToSkip: (__ENV.METHODS_TO_SKIP || '').split(',').filter((m) => m),
};

// ================================
// k6 options
// ================================
export const options = {
	scenarios: {
		json_rpc_benchmark_ramping_vus: {
			executor: 'ramping-vus',
			startVUs: 5,
			stages: [
				{ duration: '2s', target: CONFIG.concurrency },
				{ duration: CONFIG.duration, target: CONFIG.concurrency },
				{ duration: '2s', target: 0 },
			],
		},
		// Sample scenario for constant rate of requests
		// json_rpc_benchmark_rps: {
		// 	executor: 'constant-arrival-rate',
		// 	rate: 10,
		// 	preAllocatedVUs: 10,
		// 	timeUnit: '1s',
		// 	maxVUs: 20,
		// 	duration: CONFIG.duration,
		// },
	},
	thresholds: {
		// Global thresholds
		http_req_duration: ['p(95)<2000'],
		http_req_failed: ['rate<0.1'],
	},
	// Tells k6 cloud to make requests from multiple regions
	cloud: {
		// Available zones: https://grafana.com/docs/grafana-cloud/testing/k6/author-run/use-load-zones/#list-of-public-load-zones
		distribution: {
			distributionLabel1: { loadZone: 'amazon:us:ashburn', percent: 50 },
			distributionLabel2: { loadZone: 'amazon:ie:dublin', percent: 50 },
		},
	},
};

// ================================
// Pagination support
// ================================
const METHOD_CURSOR_POSITIONS = {
	suix_getOwnedObjects: 2,
	suix_queryTransactionBlocks: 1,
	suix_getCoins: 2,
	suix_getAllCoins: 1,
};
const METHOD_LENGTHS = {
	suix_getOwnedObjects: 4,
	suix_queryTransactionBlocks: 4,
	suix_getCoins: 4,
	suix_getAllCoins: 3,
};

const paginationState = new Map();

function getMethodCursorIndex(method) {
	return METHOD_CURSOR_POSITIONS[method];
}

function getMethodKey(method, params) {
	const cursorIdx = METHOD_CURSOR_POSITIONS[method];
	if (cursorIdx === undefined) return null;

	const keyParams = [...params];
	if (keyParams[cursorIdx] !== undefined) {
		keyParams[cursorIdx] = null;
	} else {
		const methodLength = METHOD_LENGTHS[method];
		while (keyParams.length < methodLength) {
			keyParams.push(null);
		}
	}
	return JSON.stringify([method, keyParams]);
}

function updateParamsCursor(body, cursorIdx, newCursor, method) {
	const params = body.params;
	if (!Array.isArray(params)) return false;

	// Extend array if needed
	const methodLength = METHOD_LENGTHS[method];
	while (params.length < methodLength) {
		params.push(null);
	}

	if (params[cursorIdx] !== undefined) {
		params[cursorIdx] = newCursor;
		return true;
	}
	return false;
}

function processPagination(request, response) {
	const method = request.method;
	const cursorIdx = getMethodCursorIndex(method);

	if (!cursorIdx) return;

	const params = request.body.params;
	if (!params || params.length === 0) return;

	const methodKey = getMethodKey(method, params);
	if (!methodKey) return;

	// Update cursor from response
	if (response && response.result) {
		const result = response.result;
		if (result.hasNextPage && result.nextCursor) {
			paginationState.set(methodKey, result.nextCursor);
		} else {
			paginationState.delete(methodKey);
		}
	}
}

// ================================
// Load requests from JSONL file
// ================================
const requests = new SharedArray('requests', function () {
	const data = open(CONFIG.requestsFile);
	return data
		.split('\n')
		.filter((line) => line.trim())
		.map((line) => {
			const parsed = JSON.parse(line);
			// Handle the actual format from sampled_read_requests.jsonl
			return {
				method: parsed.method,
				body_json: parsed.body, // The body field contains the actual JSON-RPC request
			};
		})
		.filter((request) => !CONFIG.methodsToSkip.includes(request.method))
		.filter((request) => {
			// Skip suix_getOwnedObjects with MatchAny & MatchAll filters
			if (request.method === 'suix_getOwnedObjects') {
				const params = request.body_json.params;
				if (params && params[1] && params[1].filter) {
					const filter = params[1].filter;
					if (filter.MatchAny || filter.MatchAll) {
						return false;
					}
				}
			}
			return true;
		});
});

// ================================
// Main function for k6 to execute
// ================================
export default function () {
	if (requests.length === 0) {
		console.warn('No requests loaded from file');
		return;
	}

	// Select a random request
	const requestLine = requests[Math.floor(Math.random() * requests.length)];
	const method = requestLine.method;

	// Clone the request body to avoid modifying the original
	const requestBody = JSON.parse(JSON.stringify(requestLine.body_json));

	// Process pagination if needed
	const cursorIdx = getMethodCursorIndex(method);
	if (cursorIdx !== undefined) {
		const params = requestBody.params;
		if (params && params.length > 0) {
			const methodKey = getMethodKey(method, params);
			if (methodKey) {
				const storedCursor = paginationState.get(methodKey);
				if (storedCursor) {
					updateParamsCursor(requestBody, cursorIdx, storedCursor, method);
				}
			}
		}
	}

	const response = http.post(CONFIG.endpoint, JSON.stringify(requestBody), {
		headers: {
			'Content-Type': 'application/json',
		},
		tags: { name: method },
	});

	const isSuccess = response.status === 200;

	if (isSuccess) {
		try {
			const responseBody = JSON.parse(response.body);
			// Track JSON-RPC errors
			if (responseBody.error) {
				jsonRpcErrors.add(1, { method: method });
				jsonRpcErrorsByCode.add(1, {
					method: method,
					error_code: responseBody.error.code.toString(),
					error_message: responseBody.error.message
				});
				console.warn(`JSON-RPC Error for ${method}: ${responseBody.error.code} - ${responseBody.error.message}`);
			}

			processPagination({ method, body: requestBody }, responseBody);
		} catch (e) {
			console.warn(`Failed to parse response for method ${method}:`, e);
		}
	}

	if (!isSuccess) {
		console.error(`Request failed for method ${method}:`, {
			status: response.status,
			body: response.body,
			request: requestBody,
		});
	}

	sleep(0.1);
}

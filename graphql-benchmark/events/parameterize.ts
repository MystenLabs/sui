// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

function* emitTypes(packages: string[], modules: string[], types: string[]): Generator<string> {
	for (let [i, package_] of packages.entries()) {
		yield* emitModulesHelper(package_, modules[i]);
		yield* emitTypesHelper(package_, modules[i], types[i]);
	}
}

function* emitModules(packages: string[], modules: string[]): Generator<string> {
    for (let [i, package_] of packages.entries()) {
        yield* emitModulesHelper(package_, modules[i]);
    }
}

function* emitModulesHelper(eventPackage: string, module: string): Generator<string> {
	yield eventPackage;
	yield eventPackage + '::' + module;
}

function* emitTypesHelper(eventPackage: string, module: string, type: string): Generator<string> {
	yield eventPackage + '::' + module + '::' + type;
}

function* emitSenders(senders: string[]): Generator<string> {
    for (let sender of senders) {
        yield sender;
    }
}

type Filter = { eventType?: string; sender?: string; emittingModule?: string };

/**
 * Generates all combinations of the values in the given arrays.
 *
 * @generator
 * @param {string[][]} params - An array of arrays, each containing strings.
 * @param {string[]} [prefix=[]] - An array of strings that represents the current combination of values.
 * @yields {string[]} - The next combination of values.
 *
 * @example
 *
 * const params = [['a', 'b'], ['1', '2'], ['x', 'y']];
 * for (let combination of generateCombinations(params)) {
 *   console.log(combination);
 * }
 * // Output: ['a', '1', 'x'], ['a', '1', 'y'], ['a', '2', 'x'], ['a', '2', 'y'], ['b', '1', 'x'], ['b', '1', 'y'], ['b', '2', 'x'], ['b', '2', 'y']
 */
function* generateCombinations(params: string[][], prefix: string[] = []): Generator<string[]> {
    if (params.length === 0) {
      yield prefix;
    } else {
      const [first, ...rest] = params;
      for (let value of first) {
        yield* generateCombinations(rest, [...prefix, value]);
      }
      yield* generateCombinations(rest, prefix);
    }
  }


export interface EventFilterParameters {
    eventPackages?: string[];
    eventModules?: string[];
    eventTypes?: string[];
    senders?: string[];
    emittingPackages?: string[];
    emittingModules?: string[];
}

function get_params(data: EventFilterParameters): string[][] {
    let params = [];
    if (data['eventPackages'] && data['eventModules'] && data['eventTypes']) {
        params.push([...emitTypes(data['eventPackages'], data['eventModules'], data['eventTypes'])]);
    }
    if (data['senders']) {
        params.push([...emitSenders(data['senders'])]);
    }
    if (data['emittingPackages'] && data['emittingModules']) {
        params.push([...emitModules(data['emittingPackages'], data['emittingModules'])]);
    }
    return params;
}

export function* generateFilters(data: EventFilterParameters): Generator<Filter> {
    const params = get_params(data);

    for (let combination of generateCombinations(params)) {
        yield {
          eventType: combination[0],
          sender: combination[1],
          emittingModule: combination[2],
        };
      }
}

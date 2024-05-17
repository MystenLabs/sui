// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export class SerialQueue {
	#queue: Array<() => void> = [];

	async runTask<T>(task: () => Promise<T>): Promise<T> {
		return new Promise((resolve, reject) => {
			this.#queue.push(() => {
				const promise = task();
				promise.then(resolve, reject);

				promise.finally(() => {
					this.#queue.shift();
					if (this.#queue.length > 0) {
						this.#queue[0]();
					}
				});
			});

			if (this.#queue.length === 1) {
				this.#queue[0]();
			}
		});
	}
}

export class ParallelQueue {
	#queue: Array<() => void> = [];
	activeTasks = 0;
	maxTasks: number;

	constructor(maxTasks: number) {
		this.maxTasks = maxTasks;
	}

	runTask<T>(task: () => Promise<T>): Promise<T> {
		return new Promise<T>((resolve, reject) => {
			if (this.activeTasks < this.maxTasks) {
				this.activeTasks++;

				const promise = task();
				promise.then(resolve, reject);

				promise.finally(() => {
					if (this.#queue.length > 0) {
						this.#queue.shift()!();
					} else {
						this.activeTasks--;
					}
				});
			} else {
				this.#queue.push(() => {
					const promise = task();
					promise.then(resolve, reject);
					promise.finally(() => {
						this.#queue.shift();
						if (this.#queue.length > 0) {
							this.#queue.shift()!();
						} else {
							this.activeTasks--;
						}
					});
				});
			}
		});
	}
}

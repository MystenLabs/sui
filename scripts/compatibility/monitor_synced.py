#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import json
import os
import sys
import subprocess
import getopt
from enum import Enum
import time
from datetime import datetime


NUM_RETRIES = 10
CHECKPOINT_SLEEP_SEC = 30
STARTUP_TIMEOUT_SEC = 60
RETRY_BASE_TIME_SEC = 3
AVAILABLE_NETWORKS = ['testnet', 'devnet']


class Metric(Enum):
    CHECKPOINT = 'last_executed_checkpoint'
    EPOCH = 'current_epoch'


def get_current_network_epoch(env='testnet'):
    for i in range(NUM_RETRIES):
        cmd = ['curl', '--location', '--request', 'POST', f'https://explorer-rpc.{env}.sui.io/',
               '--header', 'Content-Type: application/json', '--data-raw',
               '{"jsonrpc":"2.0", "method":"suix_getCurrentEpoch", "params":[], "id":1}']
        try:
            result = subprocess.check_output(cmd, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            print(f'curl command failed with error {e.returncode}: {e.output}')
            time.sleep(RETRY_BASE_TIME_SEC * 2**i)  # exponential backoff
            continue

        try:
            result = json.loads(result)
            if 'error' in result:
                print(
                    f'suix_getCurrentEpoch rpc request failed: {result["error"]}')
                time.sleep(3)
                continue
            return int(result['result']['epoch'])
        except (KeyError, IndexError, json.JSONDecodeError):
            print(f'suix_getCurrentEpoch rpc request failed: {result}')
            time.sleep(RETRY_BASE_TIME_SEC * 2**i)  # exponential backoff
            continue
    print(f"Failed to get current network epoch after {NUM_RETRIES} tries")
    exit(1)


def get_local_metric(metric: Metric):
    for i in range(NUM_RETRIES):
        curl = subprocess.Popen(
            ['curl', '-s', 'http://localhost:9184/metrics'], stdout=subprocess.PIPE)
        grep_1 = subprocess.Popen(
            ['grep', metric.value], stdin=curl.stdout, stdout=subprocess.PIPE)
        try:
            result = subprocess.check_output(
                ['grep', '^[^#;]'], stdin=grep_1.stdout, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            print(f'curl command failed with error {e.returncode}: {e.output}')
            time.sleep(RETRY_BASE_TIME_SEC * 2**i)  # exponential backoff
            continue

        try:
            return int(result.split()[1])
        except (KeyError, IndexError, json.JSONDecodeError):
            print(
                f'Failed to get local metric {metric.value}: {result.stdout}')
            time.sleep(RETRY_BASE_TIME_SEC * 2**i)  # exponential backoff
            continue
    print(
        f"Failed to get local metric {metric.value} after {NUM_RETRIES} tries")
    exit(1)


def await_started(start_checkpoint):
    for i in range(STARTUP_TIMEOUT_SEC):
        if get_local_metric(Metric.CHECKPOINT) != start_checkpoint:
            print(f"sui-node started successfully after {i} seconds")
            return
        print("Awaiting sui-node startup...")
        time.sleep(1)
    print(f"sui-node failed to start after {STARTUP_TIMEOUT_SEC} seconds")


def usage():
    print(
        'Usage: monitor_synced.py [--env=<env>] [--end-epoch=<epoch>] [--epoch-timeout=<timeout>] [--verbose]')
    print(
        f'  --env=<env>            Environment to sync against (one of {AVAILABLE_NETWORKS.join(", ")}')
    print('  --end-epoch=<epoch>    Epoch to sync to (default: current network epoch)')
    print('  --epoch-timeout=<timeout>  Timeout IN MINUTES for syncing to the next epoch (default: None)')
    print('  --verbose              Print verbose output')
    print('  --help                 Print this help message')


def main(argv):
    if len(argv) > 4:
        usage()
        exit(1)

    try:
        opts, args = getopt.getopt(
            argv, '', ["help", "verbose", "env=", "end-epoch=", "epoch-timeout="])
    except getopt.GetoptError as err:
        print(err)
        usage()

    env = 'testnet'
    end_epoch = None
    epoch_timeout = None
    verbose = False
    for opt, arg in opts:
        if opt == '--help':
            usage()
            exit(0)
        elif opt == '--env':
            if arg not in AVAILABLE_NETWORKS:
                print(f'Invalid environment {arg}')
                exit(1)
            env = arg
        elif opt == '--end-epoch':
            end_epoch = int(arg)
        elif opt == '--epoch-timeout':
            epoch_timeout = int(arg)
        elif opt == '--verbose':
            verbose = True
        else:
            usage()
            exit(1)

    if end_epoch is None:
        end_epoch = get_current_network_epoch(env)
    print(f'Will attempt to sync to epoch {end_epoch}')

    current_epoch = get_local_metric(Metric.EPOCH)
    print(f'Current local epoch: {current_epoch}')
    start_epoch = current_epoch

    current_checkpoint = get_local_metric(Metric.CHECKPOINT)
    print(f'Locally highest executed checkpoint: {current_checkpoint}')
    start_checkpoint = current_checkpoint

    await_started(start_checkpoint)

    current_time = datetime.now()
    start_time = current_time
    progress_check_iteration = 1
    while current_epoch < end_epoch:
        # check that we are making progress
        time.sleep(CHECKPOINT_SLEEP_SEC)
        new_checkpoint = get_local_metric(Metric.CHECKPOINT)

        if new_checkpoint == current_checkpoint:
            print(
                f'WARNING: Checkpoint is stuck at {current_checkpoint} for over {CHECKPOINT_SLEEP_SEC * progress_check_iteration} seconds')
            progress_check_iteration += 1
        else:
            if verbose:
                print(f'New highest executed checkpoint: {new_checkpoint}')
            current_checkpoint = new_checkpoint
            progress_check_iteration = 1

        new_epoch = get_local_metric(Metric.EPOCH)
        if new_epoch > current_epoch:
            current_epoch = new_epoch
            print(f'New local epoch: {current_epoch}')
            current_time = datetime.now()
        else:
            # check if we've been stuck on the same epoch for too long
            if epoch_timeout is not None and (datetime.now() - current_time).total_seconds() // 60 > epoch_timeout:
                print(
                    f'Epoch is stuck at {current_epoch} for over {epoch_timeout} minutes')
                exit(1)

    elapsed_minutes = (datetime.now() - start_time).total_seconds() / 60
    print('-------------------------------')
    print(
        f"Successfully synced to epoch {end_epoch} from epoch {start_epoch} ({current_checkpoint - start_checkpoint} checkpoints) in {elapsed_minutes:.2f} minutes")
    exit(0)


if __name__ == "__main__":
    main(sys.argv[1:])

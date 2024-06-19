#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import subprocess
import concurrent.futures
import sys
import os
import random
import argparse

parser = argparse.ArgumentParser(description='Run the simulator with different seeds')
parser.add_argument('testname', type=str, help='Name of test to run')
parser.add_argument('--test', type=str, help='Name of the test binary run', required=True)
parser.add_argument('--exact', action='store_true', help='Use exact matching for test name', default=False)
parser.add_argument('--num-seeds', type=int, help='Number of seeds to run', default=200)
parser.add_argument(
    '--seed-start',
    type=int, help='Starting seed value (defaults to seconds since epoch)',
    default=int(subprocess.check_output(["date", "+%s"]).decode("utf-8").strip()) * 1000
)
parser.add_argument('--concurrency', type=int, help='Number of concurrent tests to run', default=os.cpu_count())
parser.add_argument('--no-build', type=bool, help='Skip building the test binary', default=False)
args = parser.parse_args()

def run_command(command, env_vars):
    """Run a single command using subprocess with specific environment variables."""
    try:
        # Merge the new environment variables with the current environment
        env = os.environ.copy()
        env.update(env_vars)

        print("running seed: " + env_vars["MSIM_TEST_SEED"])
        process = subprocess.Popen(command, shell=True, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=os.setsid)
        stdout, stderr = process.communicate()
        exit_code = process.returncode
        if exit_code != 0:
            print(f"Command '{command}' failed with exit code {exit_code} for seed: " + env_vars["MSIM_TEST_SEED"])
            print(f"stdout:\n=========================={stdout.decode('utf-8')}\n==========================")
            if stderr:
              print(f"stderr:\n=========================={stderr.decode('utf-8')}\n==========================")
        else:
          print("-- seed passed %s" % env_vars["MSIM_TEST_SEED"])

        return exit_code
    except subprocess.CalledProcessError as e:
        print(f"Command '{e.cmd}' failed with exit code {e.returncode} for seed: " + env_vars["MSIM_TEST_SEED"])
        return e.returncode

def main(commands):
    """Execute a list of commands with specific environment variables and a concurrency limit of 20."""
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        # Start the subprocesses
        future_to_command = {}
        for cmd, env_vars in commands:
            future = executor.submit(run_command, cmd, env_vars)
            future_to_command[future] = cmd

        all_passed = True
        for future in concurrent.futures.as_completed(future_to_command):
            cmd = future_to_command[future]
            exit_code = future.result()
            if exit_code != 0:
                all_passed = False
                print(f"Command '{cmd}' failed with exit code {exit_code}")
                sys.exit(1)

        if all_passed:
            print("\033[92mAll tests passed successfully!\033[0m")

if __name__ == "__main__":
    repo_root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"]).decode("utf-8").strip()

    if not args.no_build:
        os.chdir(repo_root)
        subprocess.run(["cargo", "simtest", "build", "--test", args.test], check=True)

    # if binary contains no slashes, search for it in <repo_root>/target/simulator/deps/
    # otherwise, use the pathname as is
    if "/" not in args.test:
        binary = os.path.join(repo_root, "target/simulator/deps", args.test)
        # binary is a prefix of some test file, find the most recent one that matches the prefix
        if not os.path.isfile(binary):
            path = os.path.join(repo_root, "target/simulator/deps", args.test + "*")
            binary = subprocess.getstatusoutput(f"ls -ltr {path} | tail -n 1")[1].split()[-1]
            print(f"Found binary: {binary}")

    # check that binary is an executable file
    if not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        print(f"Error: {args.test} is not an executable file")
        print(f"run: `$ ls -ltr target/simulator/deps/ | tail` to find recent test binaries");
        sys.exit(1)

    commands = []

    for i in range(1, args.num_seeds + 1):
        next_seed = args.seed_start + i
        commands.append(("%s %s %s" % (binary, '--exact' if args.exact else '', args.testname), {
          "MSIM_TEST_SEED": "%d" % next_seed,
          "RUST_LOG": "off",
        }))

    # register clean up code to kill all child processes when we exit
    import atexit
    import signal
    def kill_child_processes(*args):
        print("Killing child processes")
        os.killpg(0, signal.SIGKILL)
        sys.exit(0)
    atexit.register(kill_child_processes)
    signal.signal(signal.SIGINT, kill_child_processes)

    main(commands)

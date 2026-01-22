#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import subprocess
import concurrent.futures
import sys
import os
import random
import argparse
import threading
import tempfile
import shutil

print_lock = threading.Lock()
def safe_print(*args, **kwargs):
    with print_lock:
        try:
            print(*args, **kwargs)
            sys.stdout.flush()
        except BlockingIOError:
            sys.exit(1)

def collect_satisfied_assertions(log_dir):
    """Collect all satisfied assertions from log files in the directory.

    Returns (reached_set, sometimes_set) where:
    - reached_set: locations of reachable assertions that were reached
    - sometimes_set: locations of sometimes assertions where condition was true
    """
    reached = set()
    sometimes = set()
    if not os.path.isdir(log_dir):
        return reached, sometimes
    for filename in os.listdir(log_dir):
        filepath = os.path.join(log_dir, filename)
        if filename.endswith(".reached"):
            with open(filepath, "r") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        reached.add(line)
        elif filename.endswith(".sometimes"):
            with open(filepath, "r") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        sometimes.add(line)
    return reached, sometimes

def print_reachability_summary(binary_path, reached_assertions, sometimes_assertions):
    """Print summary of reached vs unreached assertions."""
    try:
        # Import the reachpoints module
        script_dir = os.path.dirname(os.path.abspath(__file__))
        sys.path.insert(0, script_dir)
        from reachpoints import extract_reach_points, thin_if_universal

        path_to_use, tmp_handle = thin_if_universal(binary_path, "arm64")
        try:
            all_points = extract_reach_points(path_to_use)
        finally:
            if tmp_handle is not None:
                try:
                    os.unlink(tmp_handle.name)
                except OSError:
                    pass

        # Separate by type and deduplicate by location
        reachable_points = {p.loc: p.msg for p in all_points if p.assertion_type == "reachable"}
        sometimes_points = {p.loc: p.msg for p in all_points if p.assertion_type == "sometimes"}

        reached_locs = reached_assertions & set(reachable_points.keys())
        unreached_locs = set(reachable_points.keys()) - reached_assertions
        satisfied_locs = sometimes_assertions & set(sometimes_points.keys())
        unsatisfied_locs = set(sometimes_points.keys()) - sometimes_assertions

        total = len(reachable_points) + len(sometimes_points)
        satisfied = len(reached_locs) + len(satisfied_locs)

        # Build the full summary as a single string to avoid partial output
        lines = []
        lines.append("\n" + "=" * 60)
        lines.append(f"REACHABILITY: {satisfied}/{total} assertions satisfied")
        lines.append("=" * 60)

        for loc in sorted(reachable_points.keys()):
            msg = reachable_points[loc]
            status = "\033[92m[+]\033[0m" if loc in reached_locs else "\033[93m[-]\033[0m"
            lines.append(f"{status} reachable {loc}: {msg}")

        for loc in sorted(sometimes_points.keys()):
            msg = sometimes_points[loc]
            status = "\033[92m[+]\033[0m" if loc in satisfied_locs else "\033[93m[-]\033[0m"
            lines.append(f"{status} sometimes {loc}: {msg}")

        # Print all at once and flush
        output = "\n".join(lines)
        print(output, flush=True)

    except Exception as e:
        safe_print(f"Warning: Could not analyze reachability assertions: {e}")

parser = argparse.ArgumentParser(description='Run the simulator with different seeds')
parser.add_argument('testname', type=str, help='Name of test to run')
parser.add_argument('--test', type=str, help='Name of the test binary run', required=True)
parser.add_argument('--no-capture', action='store_true', help='Whether to output logs as the test runs', default=False)
parser.add_argument('--exact', action='store_true', help='Use exact matching for test name', default=False)
parser.add_argument('--num-seeds', type=int, help='Number of seeds to run', default=200)
parser.add_argument(
    '--seed-start',
    type=int, help='Starting seed value (defaults to seconds since epoch)',
    default=int(subprocess.check_output(["date", "+%s"]).decode("utf-8").strip()) * 1000
)
parser.add_argument('--concurrency', type=int, help='Number of concurrent tests to run', default=os.cpu_count())
parser.add_argument('--no-build', action='store_true', help='Skip building the test binary')
parser.add_argument('--no-reachability', action='store_true', help='Disable reachability assertion tracking')
args = parser.parse_args()

def run_command(command, env_vars):
    """Run a single command using subprocess with specific environment variables."""
    try:
        # Merge the new environment variables with the current environment
        env = os.environ.copy()
        env.update(env_vars)

        safe_print("running seed: " + env_vars["MSIM_TEST_SEED"])
        process = subprocess.Popen(command, shell=True, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=os.setsid)
        stdout, stderr = process.communicate()
        exit_code = process.returncode
        if exit_code != 0:
            safe_print(f"Command '{command}' failed with exit code {exit_code} for seed: " + env_vars["MSIM_TEST_SEED"])
            if not args.no_capture:
                safe_print(f"Run the script with --no-capture to see more details including error logs from simtest framework.")
            safe_print(f"stdout:\n=========================={stdout.decode('utf-8')}\n==========================")
            if stderr:
                safe_print(f"stderr:\n=========================={stderr.decode('utf-8')}\n==========================")
        else:
          safe_print("-- seed passed %s" % env_vars["MSIM_TEST_SEED"])

        return exit_code
    except subprocess.CalledProcessError as e:
        safe_print(f"Command '{e.cmd}' failed with exit code {e.returncode} for seed: " + env_vars["MSIM_TEST_SEED"])
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

        return all_passed

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
            safe_print(f"Found binary: {binary}")

    # check that binary is an executable file
    if not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        safe_print(f"Error: {args.test} is not an executable file")
        safe_print(f"run: `$ ls -ltr target/simulator/deps/ | tail` to find recent test binaries");
        sys.exit(1)

    # Create temp directory for reachable assertion logs
    reach_log_dir = None
    if not args.no_reachability:
        reach_log_dir = tempfile.mkdtemp(prefix="reach_assertions_")
        safe_print(f"Reachability log directory: {reach_log_dir}")

    commands = []

    for i in range(1, args.num_seeds + 1):
        next_seed = args.seed_start + i
        env_vars = {
          "MSIM_TEST_SEED": "%d" % next_seed,
          "RUST_LOG": "error",
        }
        if reach_log_dir:
            env_vars["MSIM_LOG_REACHABLE_ASSERTIONS"] = reach_log_dir
        commands.append(("%s --test-threads 1 %s %s %s" % (binary, '--no-capture' if args.no_capture else '', '--exact' if args.exact else '', args.testname), env_vars))

    # register clean up code to kill all child processes on Ctrl+C
    import signal
    def kill_child_processes(*args):
        safe_print("Killing child processes")
        os.killpg(0, signal.SIGKILL)
        sys.exit(0)
    signal.signal(signal.SIGINT, kill_child_processes)

    try:
        all_passed = main(commands)

        # Collect and report reachability results
        if reach_log_dir:
            reached, sometimes = collect_satisfied_assertions(reach_log_dir)
            print_reachability_summary(binary, reached, sometimes)

        if all_passed:
            safe_print("\033[92mAll tests passed successfully!\033[0m")

        # Ensure all output is flushed before exit
        sys.stdout.flush()
        sys.stderr.flush()

        if not all_passed:
            sys.exit(1)
    finally:
        # Clean up temp directory
        if reach_log_dir:
            shutil.rmtree(reach_log_dir, ignore_errors=True)

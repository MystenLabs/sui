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
import fcntl

def reset_stdout_blocking():
    """Reset stdout to blocking mode (child processes may set it non-blocking)."""
    fd = sys.stdout.fileno()
    flags = fcntl.fcntl(fd, fcntl.F_GETFL)
    fcntl.fcntl(fd, fcntl.F_SETFL, flags & ~os.O_NONBLOCK)

print_lock = threading.Lock()
is_tty = sys.stdout.isatty()

class Colors:
    CYAN = "\033[96m"
    GREEN = "\033[92m"
    RED = "\033[91m"
    DIM = "\033[2m"
    BOLD = "\033[1m"
    RESET = "\033[0m"

class ProgressTracker:
    def __init__(self, total):
        self.total = total
        self.running = 0
        self.passed = 0
        self.failed = 0
        self.failed_seeds = []
        self.lock = threading.Lock()

    @property
    def remaining(self):
        return self.total - self.passed - self.failed

    def start_seed(self):
        with self.lock:
            self.running += 1

    def finish_seed(self, success, seed=None):
        with self.lock:
            self.running -= 1
            if success:
                self.passed += 1
            else:
                self.failed += 1
                if seed:
                    self.failed_seeds.append(seed)

    def format_header(self):
        c = Colors
        return (
            f"{c.CYAN}Running{c.RESET}  "
            f"{c.GREEN}Passed{c.RESET}  "
            f"{c.RED}Failed{c.RESET}  "
            f"{c.DIM}Remaining{c.RESET}"
        )

    def format_progress(self):
        c = Colors
        with self.lock:
            return (
                f"{c.CYAN}{c.BOLD}{self.running:>7}{c.RESET}  "
                f"{c.GREEN}{c.BOLD}{self.passed:>6}{c.RESET}  "
                f"{c.RED}{c.BOLD}{self.failed:>6}{c.RESET}  "
                f"{c.DIM}{self.remaining:>9}{c.RESET}"
            )

progress_tracker = None

def redraw_progress_display():
    """Redraw both the header and progress lines."""
    sys.stdout.write(progress_tracker.format_header() + "\n")
    sys.stdout.write(progress_tracker.format_progress())
    sys.stdout.flush()

def safe_print(*args, **kwargs):
    with print_lock:
        try:
            if is_tty and progress_tracker:
                # Clear both progress and header lines (move up, clear, move up, clear)
                sys.stdout.write("\033[2K\033[A\033[2K\r")
            print(*args, **kwargs)
            sys.stdout.flush()
            if is_tty and progress_tracker:
                redraw_progress_display()
        except BlockingIOError:
            reset_stdout_blocking()
            if is_tty and progress_tracker:
                sys.stdout.write("\033[2K\033[A\033[2K\r")
            print(*args, **kwargs)
            sys.stdout.flush()
            if is_tty and progress_tracker:
                redraw_progress_display()

def update_progress():
    if not is_tty or not progress_tracker:
        return
    with print_lock:
        try:
            sys.stdout.write("\033[2K\r" + progress_tracker.format_progress())
            sys.stdout.flush()
        except BlockingIOError:
            reset_stdout_blocking()
            sys.stdout.write("\033[2K\r" + progress_tracker.format_progress())
            sys.stdout.flush()

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

        seed = env_vars["MSIM_TEST_SEED"]
        if not is_tty:
            safe_print(f"running seed: {seed}")
        progress_tracker.start_seed()
        update_progress()
        process = subprocess.Popen(command, shell=True, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=os.setsid)
        stdout, stderr = process.communicate()
        exit_code = process.returncode
        success = exit_code == 0
        progress_tracker.finish_seed(success, seed)
        if exit_code != 0:
            safe_print(f"Command '{command}' failed with exit code {exit_code} for seed: {seed}")
            if not args.no_capture:
                safe_print(f"Run the script with --no-capture to see more details including error logs from simtest framework.")
            safe_print(f"stdout:\n=========================={stdout.decode('utf-8')}\n==========================")
            if stderr:
                safe_print(f"stderr:\n=========================={stderr.decode('utf-8')}\n==========================")
        else:
            if not is_tty:
                safe_print(f"-- seed passed {seed}")
            update_progress()

        return exit_code
    except subprocess.CalledProcessError as e:
        progress_tracker.finish_seed(False, env_vars["MSIM_TEST_SEED"])
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

    # Verify that the test filter matches at least one test
    list_cmd = [binary, "--list", args.testname]
    if args.exact:
        list_cmd.insert(2, "--exact")
    try:
        result = subprocess.run(list_cmd, capture_output=True, text=True)
        # --list output shows tests as "test_name: test" lines
        test_lines = [line for line in result.stdout.strip().split('\n') if line.endswith(': test')]
        if not test_lines:
            safe_print(f"Error: No tests match filter '{args.testname}'")
            safe_print(f"Run: `{binary} --list` to see available tests")
            sys.exit(1)
        safe_print(f"Found {len(test_lines)} test(s) matching filter")
    except Exception as e:
        safe_print(f"Warning: Could not verify test filter: {e}")

    # Create temp directory for reachable assertion logs
    reach_log_dir = None
    if not args.no_reachability:
        reach_log_dir = tempfile.mkdtemp(prefix="reach_assertions_")
        safe_print(f"Reachability log directory: {reach_log_dir}")

    commands = []

    if args.no_capture:
      rust_log = "error"
    else:
      rust_log = "off"

    for i in range(1, args.num_seeds + 1):
        next_seed = args.seed_start + i
        env_vars = {
          "MSIM_TEST_SEED": "%d" % next_seed,
          "RUST_LOG": rust_log,
          "MSIM_WATCHDOG_TIMEOUT_MS": "120000",
        }
        if reach_log_dir:
            env_vars["MSIM_LOG_REACHABLE_ASSERTIONS"] = reach_log_dir
        commands.append(("%s --test-threads 1 %s %s %s" % (binary, '--no-capture' if args.no_capture else '', '--exact' if args.exact else '', args.testname), env_vars))

    # register clean up code to kill all child processes on Ctrl+C
    import signal
    def kill_child_processes(*args):
        if is_tty and progress_tracker:
            sys.stdout.write("\033[2K\033[A\033[2K\r")
            sys.stdout.flush()
        print("Killing child processes")
        os.killpg(0, signal.SIGKILL)
        sys.exit(0)
    signal.signal(signal.SIGINT, kill_child_processes)

    # Initialize progress tracker
    progress_tracker = ProgressTracker(args.num_seeds)

    if is_tty:
        print(progress_tracker.format_header())

    try:
        all_passed = main(commands)

        # Clear progress display before final output
        if is_tty:
            sys.stdout.write("\033[2K\033[A\033[2K\r")
            sys.stdout.flush()

        # Reset stdout to blocking mode (child processes may have changed it)
        reset_stdout_blocking()

        # Print final summary
        c = Colors
        print(f"{c.GREEN}{c.BOLD}{progress_tracker.passed} passed{c.RESET}  "
              f"{c.RED}{c.BOLD}{progress_tracker.failed} failed{c.RESET}")
        if progress_tracker.failed_seeds:
            print(f"{c.RED}Failed seeds:{c.RESET} {', '.join(progress_tracker.failed_seeds)}")

        # Collect and report reachability results
        if reach_log_dir:
            reached, sometimes = collect_satisfied_assertions(reach_log_dir)
            print_reachability_summary(binary, reached, sometimes)

        if all_passed:
            print("\033[92mAll tests passed successfully!\033[0m")

        # Ensure all output is flushed before exit
        sys.stdout.flush()
        sys.stderr.flush()

        if not all_passed:
            sys.exit(1)
    finally:
        # Clean up temp directory
        if reach_log_dir:
            shutil.rmtree(reach_log_dir, ignore_errors=True)

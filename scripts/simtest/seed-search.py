#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

import subprocess
import concurrent.futures
import sys
import os
import argparse
import threading
import tempfile
import shutil
import fcntl
import json
import re
import glob
import signal

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
        self.failures = []
        self.lock = threading.Lock()

    @property
    def failed(self):
        return len(self.failures)

    @property
    def remaining(self):
        return self.total - self.passed - self.failed

    def start_seed(self):
        with self.lock:
            self.running += 1

    def finish_seed(self, success, failure_record=None):
        with self.lock:
            self.running -= 1
            if success:
                self.passed += 1
            elif failure_record is not None:
                self.failures.append(failure_record)

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

def parse_package_name(package_id):
    """Extract the package name from a cargo `package_id` string.

    Cargo emits several `package_id` shapes:
      - `path+file:///abs/path#name@version`  (name differs from dir basename)
      - `path+file:///abs/path#version`       (name matches dir basename — shortened)
      - `registry+https://...#name@version`
      - legacy `name version (source)`
    """
    if not package_id:
        return None
    # `#name@version` form — name is the group between '#' and '@'.
    m = re.search(r'#([^@/]+)@', package_id)
    if m:
        return m.group(1)
    # `path+file:///abs/path#version` form — fall back to the directory basename.
    m = re.match(r'^path\+file://(.+?)#', package_id)
    if m:
        basename = os.path.basename(m.group(1).rstrip('/'))
        if basename:
            return basename
    # Legacy `name version (source)` form.
    m = re.match(r'^([^\s]+)\s', package_id)
    if m:
        return m.group(1)
    return package_id

def strip_hash(basename):
    """Strip cargo's trailing -<16-hex-chars> hash from a binary basename."""
    return re.sub(r'-[0-9a-f]{16,}$', '', basename)

def sanitize_for_filename(name):
    """Make a string safe to embed in a log filename."""
    return re.sub(r'[^A-Za-z0-9_.-]', '_', name)

def find_binary_by_name(name, repo_root):
    """Look up a test binary by name in target/simulator/deps/."""
    direct = os.path.join(repo_root, "target/simulator/deps", name)
    if os.path.isfile(direct) and os.access(direct, os.X_OK):
        return direct
    pattern = os.path.join(repo_root, "target/simulator/deps", name + "*")
    matches = [p for p in glob.glob(pattern)
               if os.path.isfile(p) and os.access(p, os.X_OK) and not p.endswith(".d")]
    if matches:
        matches.sort(key=os.path.getmtime)
        return matches[-1]
    return None

def discover_binaries(args, repo_root):
    """Build (if needed) and return a list of (package_name, binary_name, executable_path, manifest_dir) tuples.

    binary_name is the binary basename with cargo's hash stripped (e.g. `batch_verification_tests`).
    package_name may be `None` when not derivable (e.g. an explicit binary path passed under `--no-build`).
    manifest_dir is the directory containing the test crate's Cargo.toml — tests are launched
    with this as cwd to mirror `cargo nextest`. Falls back to repo_root when not derivable.
    """
    test_paths = []
    test_names_in_build = []
    for t in args.test or []:
        if "/" in t:
            test_paths.append(t)
        else:
            test_names_in_build.append(t)

    discovered = []

    if not args.no_build and (args.package or test_names_in_build):
        cmd = ["cargo", "simtest", "build", "--tests"]
        for pkg in args.package or []:
            cmd.extend(["--package", pkg])
        for name in test_names_in_build:
            cmd.extend(["--test", name])
        cmd.append("--message-format=json")

        safe_print("Building: " + " ".join(cmd))
        process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=None, text=True, cwd=repo_root
        )
        seen = set()
        # `--message-format=json` routes ALL compiler diagnostics into stdout JSON
        # rather than letting them flow to stderr in human-readable form, so we
        # have to collect and re-print them ourselves on failure — otherwise a
        # build error vanishes and there's nothing to debug from. Warnings get
        # dropped (they only matter when the build succeeds, and we don't echo
        # them then either).
        errors = []
        try:
            for line in process.stdout:
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                reason = obj.get("reason")
                if reason == "compiler-message":
                    msg = obj.get("message") or {}
                    if msg.get("level") == "error":
                        errors.append(msg.get("rendered") or "")
                    continue
                if reason != "compiler-artifact":
                    continue
                if not obj.get("profile", {}).get("test"):
                    continue
                executable = obj.get("executable")
                if not executable:
                    continue
                if executable in seen:
                    continue
                seen.add(executable)
                target_name = obj.get("target", {}).get("name", os.path.basename(executable))
                package_name = parse_package_name(obj.get("package_id", ""))
                manifest_path = obj.get("manifest_path")
                manifest_dir = os.path.dirname(manifest_path) if manifest_path else repo_root
                discovered.append((package_name, target_name, executable, manifest_dir))
        finally:
            process.wait()
        if process.returncode != 0:
            safe_print(f"Error: build failed with exit code {process.returncode}; "
                       f"{len(errors)} compiler error(s):")
            for rendered in errors:
                safe_print(rendered.rstrip())
            sys.exit(process.returncode)

    # `--no-build` with a NAME form: locate the most recent matching binary.
    if args.no_build:
        for name in test_names_in_build:
            binary = find_binary_by_name(name, repo_root)
            if binary is None:
                safe_print(f"Error: could not find binary for --test {name} in target/simulator/deps/")
                sys.exit(1)
            discovered.append((None, strip_hash(os.path.basename(binary)), binary, repo_root))

    # Explicit binary paths (always include, no build needed).
    for path in test_paths:
        if not os.path.isfile(path) or not os.access(path, os.X_OK):
            safe_print(f"Error: --test {path} is not an executable file")
            sys.exit(1)
        discovered.append((None, strip_hash(os.path.basename(path)), path, repo_root))

    return discovered

def list_tests_in_binary(binary, name_filter, exact):
    """Return the list of test function names exposed by a built test binary.

    Uses `<binary> --list [--exact] [filter]` to verify a filter matches at
    least one test.
    """
    cmd = [binary, "--list"]
    if exact and name_filter:
        cmd.append("--exact")
    if name_filter:
        cmd.append(name_filter)
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        safe_print(f"Warning: `{' '.join(cmd)}` exited {result.returncode}; stderr: {result.stderr.strip()}")
        return []
    tests = []
    for line in result.stdout.splitlines():
        line = line.rstrip()
        if line.endswith(": test"):
            tests.append(line[: -len(": test")])
    return tests

parser = argparse.ArgumentParser(description='Run the simulator with different seeds')
parser.add_argument('testname', type=str, nargs='?', default=None,
                    help='Test name filter (optional; when omitted, every test in each binary is run)')
parser.add_argument('--test', action='append', default=[],
                    help='Test binary path or name (repeatable). Composes with --package.')
parser.add_argument('--package', action='append', default=[],
                    help='Build all test binaries in this package and run them (repeatable). '
                         'Mutually exclusive with --no-build.')
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
parser.add_argument('--exclude', type=str, default=None,
                    help='Regex; skip any <binary>::<test> matching this pattern (re.search semantics).')
parser.add_argument('--watchdog-timeout-ms', type=int, default=120000,
                    help='Per-iteration watchdog timeout in milliseconds (sets MSIM_WATCHDOG_TIMEOUT_MS).')
parser.add_argument('--log-dir', type=str, default=None,
                    help='Directory for per-job log files plus failures.ndjson. '
                         'When set, per-job stdout+stderr is captured to disk instead of memory.')
args = parser.parse_args()

if args.no_build and args.package:
    print("Error: --no-build is mutually exclusive with --package", file=sys.stderr)
    sys.exit(2)
if not args.test and not args.package:
    print("Error: must supply at least one --test or --package", file=sys.stderr)
    sys.exit(2)

# Cap on per-process wall time. Without this, a hung test process would block
# the whole sweep forever; matches `simtestnightly`'s nextest slow-timeout
# (period=30m, terminate-after=3 = 90 minutes).
PROCESS_TIMEOUT_SECS = 90 * 60

def run_command(job):
    cmd = job["cmd"]
    env_vars = job["env"]
    seed = env_vars["MSIM_TEST_SEED"]
    log_path = job.get("log_path")
    cwd = job.get("cwd")

    env = os.environ.copy()
    env.update(env_vars)
    env.pop("MSIM_TEST_NUM", None)

    try:
        if not is_tty:
            safe_print(f"running seed: {seed}")
        progress_tracker.start_seed()
        update_progress()

        timed_out = False
        log_file = open(log_path, "wb") if log_path else None
        if log_file is not None:
            popen_kwargs = {"stdout": log_file, "stderr": subprocess.STDOUT}
        else:
            popen_kwargs = {"stdout": subprocess.PIPE, "stderr": subprocess.PIPE}

        try:
            process = subprocess.Popen(cmd, env=env, cwd=cwd, preexec_fn=os.setsid, **popen_kwargs)
            try:
                captured_stdout, captured_stderr = process.communicate(timeout=PROCESS_TIMEOUT_SECS)
            except subprocess.TimeoutExpired:
                timed_out = True
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                captured_stdout, captured_stderr = process.communicate()
        finally:
            if log_file is not None:
                log_file.close()
        exit_code = process.returncode

        success = exit_code == 0 and not timed_out
        failure_record = None
        if not success:
            status = "TIMEOUT" if timed_out else "FAIL"
            failure_record = {
                "status": status,
                "package": job.get("package"),
                "binary": job["binary_name"],
                "test": job["test"],
                "seed": seed,
            }
            if log_path:
                failure_record["log"] = os.path.basename(log_path)

        progress_tracker.finish_seed(success, failure_record)

        if not success:
            label = "TIMEOUT" if timed_out else f"exit code {exit_code}"
            safe_print(f"FAIL {job['binary_name']}::{job['test']} seed={seed} ({label})")
            if log_path:
                safe_print(f"  log: {log_path}")
            else:
                if not args.no_capture:
                    safe_print("Run the script with --no-capture to see more details "
                               "including error logs from simtest framework.")
                safe_print(f"stdout:\n=========================={captured_stdout.decode('utf-8', errors='replace')}\n==========================")
                if captured_stderr:
                    safe_print(f"stderr:\n=========================={captured_stderr.decode('utf-8', errors='replace')}\n==========================")
        else:
            if not is_tty:
                safe_print(f"-- seed passed {seed}")
            update_progress()

        return exit_code if not timed_out else 124
    except Exception as e:
        progress_tracker.finish_seed(False, {
            "status": "FAIL",
            "package": job.get("package"),
            "binary": job["binary_name"],
            "test": job["test"],
            "seed": seed,
            "error": repr(e),
        })
        safe_print(f"Error running {job['binary_name']}::{job['test']} seed={seed}: {e!r}")
        return 1

def main(jobs):
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        future_to_job = {executor.submit(run_command, job): job for job in jobs}

        all_passed = True
        for future in concurrent.futures.as_completed(future_to_job):
            exit_code = future.result()
            if exit_code != 0:
                all_passed = False

        return all_passed

if __name__ == "__main__":
    repo_root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"]).decode("utf-8").strip()

    if not args.no_build:
        os.chdir(repo_root)

    binaries = discover_binaries(args, repo_root)
    if not binaries:
        safe_print("Error: no test binaries discovered")
        sys.exit(1)

    exclude_re = re.compile(args.exclude) if args.exclude else None

    # Enumerate (binary, test) pairs. List-tests is one subprocess per binary
    # (each loads the test binary just to dump its --list output); parallelize
    # so 42 simtest binaries don't take ~minute of serial startup.
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        listings = list(executor.map(
            lambda b: list_tests_in_binary(b[2], args.testname, args.exact),
            binaries,
        ))

    bin_tests = []
    total_tests = 0
    for (package_name, binary_name, binary_path, manifest_dir), tests in zip(binaries, listings):
        if not tests:
            if args.testname:
                safe_print(f"Warning: no tests in {binary_name} match filter '{args.testname}'")
            else:
                safe_print(f"Warning: no tests found in {binary_name}")
            continue
        if exclude_re is not None:
            tests = [t for t in tests if not exclude_re.search(f"{binary_name}::{t}")]
            if not tests:
                continue
        bin_tests.append((package_name, binary_name, binary_path, manifest_dir, tests))
        total_tests += len(tests)

    if not bin_tests:
        safe_print("Error: no tests to run after filtering")
        sys.exit(1)

    safe_print(f"Found {len(bin_tests)} binary/binaries with {total_tests} test(s) total; "
               f"running {args.num_seeds} seed(s) per test "
               f"({total_tests * args.num_seeds} jobs)")

    # Reachability assumes a single binary; auto-disable when multiple are loaded.
    do_reachability = (not args.no_reachability) and len(bin_tests) == 1
    reach_log_dir = None
    if do_reachability:
        reach_log_dir = tempfile.mkdtemp(prefix="reach_assertions_")
        safe_print(f"Reachability log directory: {reach_log_dir}")

    # Set up log directory, if requested.
    if args.log_dir:
        os.makedirs(args.log_dir, exist_ok=True)

    rust_log = "error" if (args.no_capture or args.log_dir) else "off"

    # SIMTEST_STATIC_INIT_MOVE is normally exported by cargo-simtest at runtime;
    # since we invoke test binaries directly, we have to set it ourselves.
    simtest_static_init_move = os.path.join(repo_root, "examples/move/basics")

    jobs = []
    for package_name, binary_name, binary_path, manifest_dir, tests in bin_tests:
        for test_name in tests:
            for i in range(1, args.num_seeds + 1):
                seed = args.seed_start + i
                env_vars = {
                    "MSIM_TEST_SEED": "%d" % seed,
                    "RUST_LOG": rust_log,
                    "MSIM_WATCHDOG_TIMEOUT_MS": str(args.watchdog_timeout_ms),
                    "SIMTEST_STATIC_INIT_MOVE": simtest_static_init_move,
                }
                if reach_log_dir:
                    env_vars["MSIM_LOG_REACHABLE_ASSERTIONS"] = reach_log_dir

                cmd = [binary_path, "--test-threads", "1"]
                if args.no_capture:
                    cmd.append("--no-capture")
                cmd.extend(["--exact", test_name])

                log_path = None
                if args.log_dir:
                    fname = (
                        f"{sanitize_for_filename(binary_name)}__"
                        f"{sanitize_for_filename(test_name.replace('::', '__'))}__"
                        f"{seed}.log"
                    )
                    log_path = os.path.join(args.log_dir, fname)

                jobs.append({
                    "cmd": cmd,
                    "env": env_vars,
                    "cwd": manifest_dir,
                    "package": package_name,
                    "binary_name": binary_name,
                    "binary_path": binary_path,
                    "test": test_name,
                    "seed": str(seed),
                    "log_path": log_path,
                })

    # register clean up code to kill all child processes on Ctrl+C
    def kill_child_processes(*_):
        if is_tty and progress_tracker:
            sys.stdout.write("\033[2K\033[A\033[2K\r")
            sys.stdout.flush()
        print("Killing child processes")
        os.killpg(0, signal.SIGKILL)
        sys.exit(0)
    signal.signal(signal.SIGINT, kill_child_processes)

    progress_tracker = ProgressTracker(len(jobs))

    if is_tty:
        print(progress_tracker.format_header())

    try:
        all_passed = main(jobs)

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
        if progress_tracker.failures:
            failed_summary = ", ".join(
                sorted({f"{f['binary']}::{f['test']}#{f['seed']}" for f in progress_tracker.failures})
            )
            print(f"{c.RED}Failed:{c.RESET} {failed_summary}")

        # Always emit failures.ndjson when --log-dir is set; an empty file means no failures.
        if args.log_dir:
            failures_path = os.path.join(args.log_dir, "failures.ndjson")
            with open(failures_path, "w") as f:
                for record in progress_tracker.failures:
                    f.write(json.dumps(record) + "\n")

        # Collect and report reachability results
        if reach_log_dir:
            reached, sometimes = collect_satisfied_assertions(reach_log_dir)
            # Reachability only runs in single-binary mode; safe to index.
            print_reachability_summary(bin_tests[0][2], reached, sometimes)

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

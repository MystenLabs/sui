#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
#
# Reduces a nextest JUnit report to one tab-separated line per test: binary, test, duration in
# seconds, outcome, and number of attempts. CI publishes this file instead of the report, because
# the report can contain test output, which is masked only in the job log.
#
# Usage: junit-timings.py <junit.xml> <output.tsv>
# The output file is removed first, and is not written when the report does not exist.

import os
import sys
import xml.etree.ElementTree as ET


def outcome(testcase):
    if testcase.find("skipped") is not None:
        return "skipped"
    if testcase.find("failure") is not None or testcase.find("error") is not None:
        return "failed"
    if testcase.find("flakyFailure") is not None or testcase.find("flakyError") is not None:
        return "flaky"
    return "passed"


def attempts(testcase):
    retries = ("rerunFailure", "rerunError", "flakyFailure", "flakyError")
    return 1 + sum(len(testcase.findall(tag)) for tag in retries)


def main():
    report, output = sys.argv[1], sys.argv[2]
    if os.path.exists(output):
        os.remove(output)
    if not os.path.exists(report):
        print(f"{report} does not exist; no timings written")
        return
    root = ET.parse(report).getroot()
    lines = ["binary\ttest\tseconds\toutcome\tattempts"]
    for suite in root.iter("testsuite"):
        for testcase in suite.iter("testcase"):
            lines.append(
                "\t".join(
                    [
                        suite.get("name", ""),
                        testcase.get("name", ""),
                        testcase.get("time", ""),
                        outcome(testcase),
                        str(attempts(testcase)),
                    ]
                )
            )
    with open(output, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {len(lines) - 1} test timings to {output}")


if __name__ == "__main__":
    main()

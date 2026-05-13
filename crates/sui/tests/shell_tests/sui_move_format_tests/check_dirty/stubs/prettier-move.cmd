@REM Copyright (c) Mysten Labs, Inc.
@REM SPDX-License-Identifier: Apache-2.0
@echo off

if "%~1"=="--version" (
    echo stub-prettier-move 0.0.0
    exit /b 0
)

>&2 echo [warn] sources/foo.move
>&2 echo Code style issues found in 1 file.
exit /b 1

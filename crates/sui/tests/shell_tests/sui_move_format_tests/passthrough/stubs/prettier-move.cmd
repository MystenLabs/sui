@REM Copyright (c) Mysten Labs, Inc.
@REM SPDX-License-Identifier: Apache-2.0
@echo off

if "%~1"=="--version" (
    echo stub-prettier-move 0.0.0
    exit /b 0
)

echo prettier-move called with: %*
exit /b 0

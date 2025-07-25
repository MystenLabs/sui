# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

from __future__ import annotations

import os
from typing import Callable, Sequence


def run_as_main(
    main_module: str,
    main_function: str | None,
    main_function_hooks: Sequence[Callable[[], None]] = (),
) -> None:
    """
    Run the specified module or function as the main program.

    `main_module` is the fully qualified import path to the module to be run as
    main. If `main_function` is not None, it's the attribute on `main_module`
    that should be called after importing it.

    This function supports overriding the main module when the `PAR_MAIN_OVERRIDE`
    env variable is set.

    When `main_function_hooks` is set, the hooks are called in sequence after the
    main module has been imported, just before the main function is invoked. This
    parameter has no effect if `main_function` is `None`.
    """

    # Allow users to decorate the main module. In normal Python invocations this
    # can be done by prefixing the arguments with `-m decoratingmodule`. It's not
    # that easy for par files. The startup script sets up `sys.path` from
    # within the Python interpreter. Enable decorating the main module after
    # `sys.path` has been setup by setting the PAR_MAIN_OVERRIDE environment
    # variable.
    decorate_main_module = os.environ.pop("PAR_MAIN_OVERRIDE", None)
    is_decorated_module = "PAR_MAIN_ORIGINAL" in os.environ
    if decorate_main_module:
        # Pass the original main module as environment variable for the process.
        # Allowing the decorating module to pick it up.
        os.environ["PAR_MAIN_ORIGINAL"] = main_module
        main_module = decorate_main_module

    # Also pass the main function if set:
    decorate_main_function = os.environ.pop("PAR_MAIN_FUNCTION_OVERRIDE", None)
    if main_function and (decorate_main_module or is_decorated_module):
        os.environ["PAR_MAIN_FUNCTION_ORIGINAL"] = main_function
        main_function = decorate_main_function

    if not main_function:
        import runpy

        # pyre-fixme[16]: Module `runpy` has no attribute `_run_module_as_main`.
        runpy._run_module_as_main(main_module, alter_argv=False)
        return

    from importlib import import_module

    mod = import_module(main_module)
    main = getattr(mod, main_function)

    # This is normally done by `runpy._run_module_as_main`, and is
    # important to make multiprocessing work
    import sys

    sys.modules["__main__"] = mod

    # Pretend we're executing `main()` directly
    if hasattr(main, "__globals__") and isinstance(main.__globals__, dict):
        main.__globals__["__name__"] = "__main__"
    for hook in main_function_hooks:
        hook()
    main()

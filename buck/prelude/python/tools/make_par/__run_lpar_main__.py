# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

#
# Put everything inside an __invoke_main() function.
# This way anything we define won't pollute globals(), since runpy
# will propagate our globals() as to the user's main module.
# pyre-fixme[3]: Return type must be annotated.
def __invoke_main():
    import os
    import runpy
    import sys

    module = os.getenv("FB_PAR_MAIN_MODULE")
    main_function = os.getenv("FB_PAR_MAIN_FUNCTION")

    sys.argv[0] = os.getenv("FB_LPAR_INVOKED_NAME", sys.argv[0])
    del sys.path[0]

    main_runner_module = os.getenv("FB_PAR_MAIN_RUNNER_MODULE")
    main_runner_function = os.getenv("FB_PAR_MAIN_RUNNER_FUNCTION")

    if main_runner_module and main_runner_function:
        from importlib import import_module

        mod = import_module(main_runner_module)
        run_as_main = getattr(mod, main_runner_function)
        run_as_main(module, main_function)
        return

    #### BUCK1-ONLY CODE FOLLOWS ####

    # Allow users to decorate the main module. In normal Python invocations
    # this can be done by prefixing the arguments with `-m decoratingmodule`.
    # It's not that easy for par files. The startup script sets up `sys.path`
    # from within the Python interpreter. Enable decorating the main module
    # after `sys.path` has been setup by setting the PAR_MAIN_OVERRIDE
    # environment variable.
    decorate_main_module = os.environ.pop("PAR_MAIN_OVERRIDE", None)
    if decorate_main_module:
        # Pass the original main module as environment variable for the process.
        # Allowing the decorating module to pick it up.
        # pyre-fixme[6]: For 2nd argument expected `str` but got `Optional[str]`.
        os.environ["PAR_MAIN_ORIGINAL"] = module
        module = decorate_main_module

    if main_function:
        assert module
        from importlib import import_module

        mod = import_module(module)
        main = getattr(mod, main_function)
        # This is normally done by `runpy._run_module_as_main`, and is
        # important to make multiprocessing work
        sys.modules["__main__"] = mod
        main()
        return

    del os
    del sys

    # pyre-fixme[16]: Module `runpy` has no attribute `_run_module_as_main`.
    runpy._run_module_as_main(module, False)


__invoke_main()

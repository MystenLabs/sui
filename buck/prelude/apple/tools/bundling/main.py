# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import cProfile
import json
import logging
import pstats
import shlex
import sys
from pathlib import Path
from typing import List, Optional

from apple.tools.code_signing.apple_platform import ApplePlatform
from apple.tools.code_signing.codesign_bundle import (
    AdhocSigningContext,
    codesign_bundle,
    CodesignConfiguration,
    non_adhoc_signing_context,
)
from apple.tools.code_signing.list_codesign_identities_command_factory import (
    ListCodesignIdentitiesCommandFactory,
)

from apple.tools.re_compatibility_utils.writable import make_dir_recursively_writable

from .action_metadata import action_metadata_if_present

from .assemble_bundle import assemble_bundle
from .assemble_bundle_types import BundleSpecItem, IncrementalContext
from .incremental_state import (
    IncrementalState,
    IncrementalStateItem,
    IncrementalStateJSONEncoder,
    parse_incremental_state,
)
from .swift_support import run_swift_stdlib_tool, SwiftSupportArguments


_METADATA_PATH_KEY = "ACTION_METADATA"


def _args_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Tool which assembles the Apple bundle."
    )
    parser.add_argument(
        "--output",
        metavar="</path/to/app.bundle>",
        type=Path,
        required=True,
        help="Absolute path to Apple bundle result.",
    )
    parser.add_argument(
        "--spec",
        metavar="<Spec.json>",
        type=Path,
        required=True,
        help="Path to file with JSON representing the bundle contents. It should contain a dictionary which maps bundle relative destination paths to source paths.",
    )
    parser.add_argument(
        "--codesign",
        action="store_true",
        help="Should the final bundle be codesigned.",
    )
    parser.add_argument(
        "--codesign-tool",
        metavar="</usr/bin/codesign>",
        type=Path,
        required=False,
        help="Path to code signing utility. If not provided standard `codesign` tool will be used.",
    )
    parser.add_argument(
        "--info-plist-source",
        metavar="</prepared/Info.plist>",
        type=Path,
        required=False,
        help="Path to Info.plist source file which is used only to make code signing decisions (to be bundled `Info.plist` should be present in spec parameter). Required if code signing is requested.",
    )
    parser.add_argument(
        "--info-plist-destination",
        metavar="<Info.plist>",
        type=Path,
        required=False,
        help="Required if code signing is requested. Bundle relative destination path to Info.plist file if it is present in bundle.",
    )
    parser.add_argument(
        "--entitlements",
        metavar="<Entitlements.plist>",
        type=Path,
        required=False,
        help="Path to file with entitlements to be used during code signing. If it's not provided the minimal entitlements are going to be generated.",
    )
    parser.add_argument(
        "--profiles-dir",
        metavar="</provisioning/profiles/directory>",
        type=Path,
        required=False,
        help="Required if non-ad-hoc code signing is requested. Path to directory with provisioning profile files.",
    )
    parser.add_argument(
        "--codesign-identities-command",
        metavar='<"/signing/identities --available">',
        type=str,
        required=False,
        help="Command listing available code signing identities. If it's not provided `security` utility is assumed to be available and is used.",
    )
    parser.add_argument(
        "--ad-hoc",
        action="store_true",
        help="Perform ad-hoc signing if set.",
    )
    parser.add_argument(
        "--ad-hoc-codesign-identity",
        metavar="<identity>",
        type=str,
        required=False,
        help="Codesign identity to use when ad-hoc signing is performed.",
    )
    parser.add_argument(
        "--codesign-configuration",
        required=False,
        type=CodesignConfiguration,
        choices=[e.value for e in CodesignConfiguration],
        help=f"""
            Augments how code signing is run.
            Pass `{CodesignConfiguration.fastAdhoc}` to skip adhoc signing bundles if the executables are already adhoc signed.
            Pass `{CodesignConfiguration.dryRun}` for code signing to be run in dry mode (instead of actual signing only .plist
            files with signing parameters will be generated in the root of each signed bundle).
        """,
    )
    parser.add_argument(
        "--platform",
        metavar="<apple platform>",
        type=ApplePlatform,
        required=False,
        help="Required if code signing or Swift support is requested. Apple platform for which the bundle is built.",
    )
    parser.add_argument(
        "--incremental-state",
        metavar="<IncrementalState.json>",
        type=Path,
        required=False,
        help="Required if script is run in incremental mode. Path to file with JSON which describes the contents of bundle built previously.",
    )
    parser.add_argument(
        "--profile-output",
        metavar="<ProfileOutput.txt>",
        type=Path,
        required=False,
        help="Path to the profiling output. If present profiling will be enabled.",
    )
    parser.add_argument(
        "--log-level-stderr",
        choices=["debug", "info", "warning", "error", "critical"],
        type=str,
        required=False,
        default="warning",
        help="Logging level for messages written to stderr.",
    )
    parser.add_argument(
        "--log-level-file",
        choices=["debug", "info", "warning", "error", "critical"],
        type=str,
        required=False,
        default="info",
        help="Logging level for messages written to a log file.",
    )
    parser.add_argument(
        "--log-file",
        type=Path,
        required=False,
        help="Path to a log file. If present logging will be directed to this file in addition to stderr.",
    )
    parser.add_argument(
        "--binary-destination",
        metavar="<Binary>",
        type=Path,
        required=False,
        help="Required if swift support was requested. Bundle relative destination path to bundle binary.",
    )
    parser.add_argument(
        "--frameworks-destination",
        metavar="<Frameworks>",
        type=Path,
        required=False,
        help="Required if swift support was requested. Bundle relative destination path to frameworks directory.",
    )
    parser.add_argument(
        "--plugins-destination",
        metavar="<Plugins>",
        type=Path,
        required=False,
        help="Required if swift support was requested. Bundle relative destination path to plugins directory.",
    )
    parser.add_argument(
        "--appclips-destination",
        metavar="<AppClips>",
        type=Path,
        required=False,
        help="Required if swift support was requested. Bundle relative destination path to appclips directory.",
    )
    parser.add_argument(
        "--sdk-root",
        metavar="<path/to/SDK>",
        type=Path,
        required=False,
        help="Required if swift support was requested. Path to SDK root.",
    )
    parser.add_argument(
        "--swift-stdlib-command",
        metavar='<"/swift/stdlib/tool --foo bar/qux">',
        type=str,
        required=False,
        help="Swift stdlib command prefix. If present, output bundle will contain needed Swift standard libraries (to support the lack of ABI stability or certain backports usage).",
    )
    parser.add_argument(
        "--check-conflicts",
        action="store_true",
        help="Check there are no path conflicts between different source parts of the bundle if enabled.",
    )
    return parser


def _main() -> None:
    args_parser = _args_parser()
    args = args_parser.parse_args()

    if args.log_file:
        with open(args.log_file, "w") as _:
            # We need to open the log file for two reasons:
            # - Ensure it exists after action runs, as it's an output and thus required
            # - It gets erased, so that we get new logs when doing incremental bundling
            pass

    _setup_logging(
        stderr_level=getattr(logging, args.log_level_stderr.upper()),
        file_level=getattr(logging, args.log_level_file.upper()),
        log_path=args.log_file,
    )

    pr = cProfile.Profile()
    profiling_enabled = args.profile_output is not None
    if profiling_enabled:
        pr.enable()

    if args.codesign:
        assert args.info_plist_source and args.info_plist_destination and args.platform
        if args.ad_hoc:
            signing_context = AdhocSigningContext(
                codesign_identity=args.ad_hoc_codesign_identity
            )
            selected_identity_argument = args.ad_hoc_codesign_identity
        else:
            assert (
                args.profiles_dir
            ), "Path to directory with provisioning profile files should be set when signing is not ad-hoc."
            signing_context = non_adhoc_signing_context(
                info_plist_source=args.info_plist_source,
                info_plist_destination=args.info_plist_destination,
                provisioning_profiles_dir=args.profiles_dir,
                entitlements_path=args.entitlements,
                platform=args.platform,
                list_codesign_identities_command_factory=ListCodesignIdentitiesCommandFactory.override(
                    shlex.split(args.codesign_identities_command)
                )
                if args.codesign_identities_command
                else None,
                log_file_path=args.log_file,
            )
            selected_identity_argument = (
                signing_context.selected_profile_info.identity.fingerprint
            )
    else:
        signing_context = None
        selected_identity_argument = None

    with args.spec.open(mode="rb") as spec_file:
        spec = json.load(spec_file, object_hook=lambda d: BundleSpecItem(**d))

    incremental_context = _incremental_context(
        incremenatal_state_path=args.incremental_state,
        codesigned=args.codesign,
        codesign_configuration=args.codesign_configuration,
        codesign_identity=selected_identity_argument,
    )

    incremental_state = assemble_bundle(
        spec=spec,
        bundle_path=args.output,
        incremental_context=incremental_context,
        check_conflicts=args.check_conflicts,
    )

    swift_support_args = _swift_support_arguments(
        args_parser,
        args,
    )

    if swift_support_args:
        swift_stdlib_paths = run_swift_stdlib_tool(
            bundle_path=args.output,
            signing_identity=selected_identity_argument,
            args=swift_support_args,
        )
    else:
        swift_stdlib_paths = []

    if args.codesign:
        # Vendored frameworks/bundles could already be pre-signed, in which case,
        # re-signing them requires modifying them. On RE, the umask is such that
        # copied files (when constructing the bundle) are not writable.
        make_dir_recursively_writable(args.output)
        if signing_context is None:
            raise RuntimeError(
                "Expected signing context to be created before bundling is done if codesign is requested."
            )
        codesign_bundle(
            bundle_path=args.output,
            signing_context=signing_context,
            entitlements_path=args.entitlements,
            platform=args.platform,
            codesign_on_copy_paths=[i.dst for i in spec if i.codesign_on_copy],
            codesign_tool=args.codesign_tool,
            codesign_configuration=args.codesign_configuration,
        )

    if incremental_state:
        _write_incremental_state(
            spec=spec,
            items=incremental_state,
            path=args.incremental_state,
            codesigned=args.codesign,
            codesign_configuration=args.codesign_configuration,
            selected_codesign_identity=selected_identity_argument,
            swift_stdlib_paths=swift_stdlib_paths,
        )

    if profiling_enabled:
        pr.disable()
        with open(args.profile_output, "w") as s:
            sortby = pstats.SortKey.CUMULATIVE
            ps = pstats.Stats(pr, stream=s).sort_stats(sortby)
            ps.print_stats()


def _incremental_context(
    incremenatal_state_path: Optional[Path],
    codesigned: bool,
    codesign_configuration: CodesignConfiguration,
    codesign_identity: Optional[str],
) -> Optional[IncrementalContext]:
    action_metadata = action_metadata_if_present(_METADATA_PATH_KEY)
    if action_metadata is None:
        # Environment variable not set, running in non-incremental mode.
        return None
    # If there is no incremental state or we failed to parse it (maybe because of a format change)
    # do a clean (non-incremental) assemble right now but generate proper state for next run.
    incremental_state = (
        _read_incremental_state(incremenatal_state_path)
        if incremenatal_state_path
        else None
    )
    return IncrementalContext(
        metadata=action_metadata,
        state=incremental_state,
        codesigned=codesigned,
        codesign_configuration=codesign_configuration,
        codesign_identity=codesign_identity,
    )


def _read_incremental_state(path: Path) -> Optional[IncrementalState]:
    logging.getLogger(__name__).info(f"Will read incremental state from `{path}`.")
    if not path.exists():
        logging.getLogger(__name__).warning(
            f"File with incremental state doesn't exist at `{path}`."
        )
        return None
    try:
        with path.open() as f:
            return parse_incremental_state(f)
    except Exception:
        logging.getLogger(__name__).exception("Failed to read incremental state")
        return None
    finally:
        # If something goes wrong and we don't delete the file
        # we probably end up in faulty state where incremental state
        # doesn't match the output. Hence delete it early.
        path.unlink()


def _swift_support_arguments(
    parser: argparse.ArgumentParser,
    args: argparse.Namespace,
) -> Optional[SwiftSupportArguments]:
    if not args.swift_stdlib_command:
        return None
    if not args.binary_destination:
        parser.error(
            "Expected `--binary-destination` argument to be specified when `--swift-stdlib-command` is present."
        )
    if not args.appclips_destination:
        parser.error(
            "Expected `--appclips-destination` argument to be specified when `--swift-stdlib-command` is present."
        )
    if not args.frameworks_destination:
        parser.error(
            "Expected `--frameworks-destination` argument to be specified when `--swift-stdlib-command` is present."
        )
    if not args.plugins_destination:
        parser.error(
            "Expected `--plugins-destination` argument to be specified when `--swift-stdlib-command` is present."
        )
    if not args.platform:
        parser.error(
            "Expected `--platform` argument to be specified when `--swift-stdlib-command` is present."
        )
    if not args.sdk_root:
        parser.error(
            "Expected `--sdk-root` argument to be specified when `--swift-stdlib-command` is present."
        )
    return SwiftSupportArguments(
        swift_stdlib_command=args.swift_stdlib_command,
        binary_destination=args.binary_destination,
        appclips_destination=args.appclips_destination,
        frameworks_destination=args.frameworks_destination,
        plugins_destination=args.plugins_destination,
        platform=args.platform,
        sdk_root=args.sdk_root,
    )


def _write_incremental_state(
    spec: List[BundleSpecItem],
    items: List[IncrementalStateItem],
    path: Path,
    codesigned: bool,
    codesign_configuration: CodesignConfiguration,
    selected_codesign_identity: Optional[str],
    swift_stdlib_paths: List[Path],
):
    state = IncrementalState(
        items,
        codesigned=codesigned,
        codesign_configuration=codesign_configuration,
        codesign_on_copy_paths=[Path(i.dst) for i in spec if i.codesign_on_copy],
        codesign_identity=selected_codesign_identity,
        swift_stdlib_paths=swift_stdlib_paths,
    )
    path.touch()
    try:
        with path.open(mode="w") as f:
            json.dump(state, f, cls=IncrementalStateJSONEncoder)
    except Exception:
        path.unlink()
        raise


def _setup_logging(
    stderr_level: int, file_level: int, log_path: Optional[Path]
) -> None:
    stderr_handler = logging.StreamHandler()
    stderr_handler.setLevel(stderr_level)
    log_format = (
        "%(asctime)s - %(name)s - %(levelname)s - %(message)s (%(filename)s:%(lineno)d)"
    )
    stderr_handler.setFormatter(
        ColoredLogFormatter(log_format)
        if sys.stderr.isatty()
        else logging.Formatter(log_format)
    )

    handlers: List[logging.Handler] = [stderr_handler]

    if log_path:
        file_handler = logging.FileHandler(log_path, encoding="utf-8")
        file_handler.setFormatter(logging.Formatter(log_format))
        file_handler.setLevel(file_level)
        handlers.append(file_handler)

    logging.basicConfig(level=logging.DEBUG, handlers=handlers)


class ColoredLogFormatter(logging.Formatter):

    _colors = {
        logging.DEBUG: "\x1b[m",
        logging.INFO: "\x1b[37m",
        logging.WARNING: "\x1b[33m",
        logging.ERROR: "\x1b[31m",
        logging.CRITICAL: "\x1b[1;31m",
    }
    _reset_color = "\x1b[0m"

    def __init__(self, text_format: str):
        self.text_format = text_format

    def format(self, record: logging.LogRecord):
        colored_format = (
            self._colors[record.levelno] + self.text_format + self._reset_color
        )
        formatter = logging.Formatter(colored_format)
        return formatter.format(record)


if __name__ == "__main__":
    _main()

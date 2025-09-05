load(":native.bzl", prelude = "native")

oncall("build_infra")

# Done to avoid triggering a lint rule that replaces glob with an fbcode macro
globby = glob

srcs = globby(
    ["**"],
    # Context: https://fb.workplace.com/groups/buck2users/posts/3121903854732641/
    exclude = ["**/.pyre_configuration.local"],
)

# Re-export filegroups that are behind package boundary violations for
# Buck2.
prelude.filegroup(
    name = "files",
    srcs = srcs,
    visibility = ["PUBLIC"],
)

# Tests want BUCK.v2 instead of TARGETS.v2
prelude.genrule(
    name = "copy_android_constraint",
    out = "BUCK.v2",
    cmd = "cp $(location prelude//android/constraints:files)/TARGETS.v2 $OUT",
    visibility = ["PUBLIC"],
)

prelude.filegroup(
    name = "prelude",
    srcs = {
        "": ":files",
        "android/constraints/BUCK.v2": ":copy_android_constraint",
    },
    visibility = ["PUBLIC"],
)

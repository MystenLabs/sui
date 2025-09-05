MyPkgToolchainInfo = provider(
    fields={
        "bin": provider_field(typing.Any, default=""),
    }
)


def _mypkg_toolchain_impl(ctx: AnalysisContext):
    return [
        DefaultInfo(),
        MyPkgToolchainInfo(
            bin=ctx.attrs.bin,
        ),
    ]


system_mypkg_toolchain = rule(
    impl=_mypkg_toolchain_impl,
    attrs={
        "bin": attrs.string(),
    },
    is_toolchain_rule=True,
)

GcloudToolchainInfo = provider(
    fields={
        "bin": provider_field(typing.Any, default=""),
    }
)


def _gcloud_toolchain_impl(ctx: AnalysisContext):
    return [
        DefaultInfo(),
        GcloudToolchainInfo(
            bin=ctx.attrs.bin,
        ),
    ]


system_gcloud_toolchain = rule(
    impl=_gcloud_toolchain_impl,
    attrs={
        "bin": attrs.string(),
    },
    is_toolchain_rule=True,
)

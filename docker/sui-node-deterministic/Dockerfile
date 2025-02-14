FROM stagex/pallet-rust:sx2025.02.0@sha256:3c0bef86b8b1325f74aea5ecf4ab26bc529f7f61fc440e5b1ab877e338ca0a06 AS pallet-rust
FROM stagex/core-cross-x86_64-gnu-gcc:sx2025.02.0@sha256:d04f7b231f5137de6ba910702eedb1ef316064e6e5bf8e539f7c615c5be5de93 AS cross-x86_64-gnu-gcc
FROM stagex/core-cross-x86_64-gnu-rust:sx2025.02.0@sha256:24fb0288c7570c3975ac3f72159cdf5c90cb2d66bb09ccc74d80beb731e7a6fd AS cross-x86_64-gnu-rust
FROM stagex/user-glibc:sx2025.02.0@sha256:e5bd3fe25abda77183dba03978270b92bdc800e9382870b948c8008ba4b21d4d AS glibc

FROM pallet-rust AS build
COPY --from=cross-x86_64-gnu-gcc . /
COPY --from=cross-x86_64-gnu-rust . /
COPY --from=glibc . /
ARG PROFILE
ENV RUST_BACKTRACE=1
ENV RUSTFLAGS="-C codegen-units=1"
ENV RUSTFLAGS="${RUSTFLAGS} -C target-feature=+crt-static"
ENV RUSTFLAGS="${RUSTFLAGS} -C linker=/usr/bin/x86_64-linux-gnu-gcc"
ENV PROFILE=${PROFILE}
WORKDIR sui
COPY . .
RUN cargo fetch
RUN --network=none <<-EOF
	cargo build \
		--target x86_64-unknown-linux-gnu \
		--frozen \
		--profile ${PROFILE} \
		--bin sui-node
	mkdir -p /rootfs/opt/sui/bin /rootfs/usr/local/bin
	cp \
		target/x86_64-unknown-linux-gnu/release/sui-node \
		/rootfs/opt/sui/bin/sui-node
	ln -s \
		/opt/sui/bin/sui-node \
		/rootfs/usr/local/bin/sui-node
EOF

FROM scratch AS package
ARG PROFILE
ARG GIT_REVISION
LABEL build-date=${BUILD_DATE}
LABEL git-revision=${GIT_REVISION}
COPY --from=build /rootfs /

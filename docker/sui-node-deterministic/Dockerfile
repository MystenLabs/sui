ARG PROFILE=release
ARG BUILD_DATE
ARG GIT_REVISION

FROM stagex/pallet-clang-cmake-busybox@sha256:d83c2f27ccacbc20efff56f8d46f53146cae1b3e2271f13800e36869bef6616d AS pallet-clang-cmake-busybox
FROM stagex/pallet-clang-gnu-busybox@sha256:5ec4a554d07f5aed890dcdf2aa31b5d94ba0132425d71feb231866c7bacebc1a AS pallet-clang-gnu-busybox
FROM stagex/core-cross-x86_64-gnu-gcc@sha256:72382cbb0267dd86e83fe25abe5d0f12fd51e494e0bf8a45974d3b15bc4e317f AS core-cross-x86_64-gnu-gcc
FROM stagex/pallet-rust@sha256:4062550919db682ebaeea07661551b5b89b3921e3f3a2b0bc665ddea7f6af1ca AS pallet-rust
FROM stagex/core-ncurses@sha256:35d72c3b114e43b754efaa7888b4998bc1871aa8fac9bad5c085e9c30689419f AS core-ncurses
FROM stagex/core-bash@sha256:5b598c14eef61148baf3f5a2830a214a5985b5d3544b019e3d0ed53c6b66989a AS core-bash
FROM stagex/core-make@sha256:2e75f157275b517738d0ae6af1b3cd0aca0a61d0aff1402f25d6754065e65c7a AS core-make
FROM stagex/core-coreutils@sha256:d53c34b74dd99125fbdd8a274d750e7758cd4bb4d5d3d528f9f07a694333d2e7 AS core-coreutils
FROM stagex/core-openssl@sha256:4ecf4f42ad958a25d4622b246aacdbccd523aacbb8ce036f387d39e8a17b9840 AS core-openssl
FROM stagex/core-busybox@sha256:d608daa946e4799cf28b105aba461db00187657bd55ea7c2935ff11dac237e27 AS core-busybox

FROM scratch as base
COPY --from=core-cross-x86_64-gnu-gcc . /
ENV ZSTD_VERSION=1.5.7
ENV ZSTD_HASH=37d7284556b20954e56e1ca85b80226768902e2edabd3b649e9e72c0c9012ee3
ENV ROCKSDB_VERSION=9.10.0
ENV ROCKSDB_HASH=fdccab16133c9d927a183c2648bcea8d956fb41eb1df2aacaa73eb0b95e43724
ENV ZLIB_VERSION=1.3.1
ENV ZLIB_HASH=9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23
ENV ARCH=x86_64
ENV TARGET=${ARCH}-linux-gnu
ENV CARGO_TARGET=${ARCH}-unknown-linux-gnu
ENV GCC_VERSION=15.2.0
ENV GCC_INSTALL_DIR=/opt/cross/lib/gcc/${TARGET}/${GCC_VERSION}
ARG PROFILE
ENV PROFILE=${PROFILE}
ARG BUILD_DATE
ENV BUILD_DATE=${BUILD_DATE}
ARG GIT_REVISION
ENV GIT_REVISION=${GIT_REVISION}
ENV PREFIX=/opt/cross/${TARGET}
ENV CC=clang
ENV CXX=clang++
ENV LDFLAGS="--ld-path=/usr/bin/ld.lld"
ENV PREFIX=${PREFIX}
ENV DESTDIR=/rootfs
ENV LIBDIR=${PREFIX}/lib
COPY <<-EOF /etc/clang/clang.cfg
--target=${TARGET} --gcc-install-dir=${GCC_INSTALL_DIR} -rtlib=libgcc -unwindlib=libgcc -stdlib=libstdc++
EOF
COPY <<-EOF /etc/clang/clang++.cfg
--target=${TARGET} --gcc-install-dir=${GCC_INSTALL_DIR} -rtlib=libgcc -unwindlib=libgcc -stdlib=libstdc++
EOF
WORKDIR /build

FROM base as build-rocksdb
COPY --from=pallet-clang-gnu-busybox . /
COPY --from=core-bash . /
COPY --from=core-ncurses . /
COPY --from=core-openssl . /
COPY --from=core-coreutils . /
ADD --checksum=sha256:${ROCKSDB_HASH} https://github.com/facebook/rocksdb/archive/refs/tags/v${ROCKSDB_VERSION}.tar.gz .
ENV EXTRA_CXXFLAGS="-include cstdint -Wno-error=nontrivial-memcall -Wno-error=unused-parameter -Wno-error=unused-command-line-argument -Wno-error=deprecated-declarations"
RUN --network=none <<-EOF
	tar -xf v${ROCKSDB_VERSION}.tar.gz
	cd rocksdb-${ROCKSDB_VERSION}
	make static_lib shared_lib install
EOF
FROM scratch as package-rocksdb
COPY --from=build-rocksdb /rootfs/ /

FROM base as build-zlib
COPY --from=pallet-clang-gnu-busybox . /
ADD --checksum=sha256:${ZLIB_HASH} https://www.zlib.net/zlib-${ZLIB_VERSION}.tar.gz .
RUN --network=none <<-EOF
	set -eu
	tar -xf zlib-${ZLIB_VERSION}.tar.gz
	cd zlib-${ZLIB_VERSION}
	./configure --prefix=${PREFIX} --shared
	make install
EOF
FROM scratch as package-zlib
COPY --from=build-zlib /rootfs/ /

FROM base as build-zstd
COPY --from=pallet-clang-cmake-busybox . /
COPY --from=core-make . /
COPY --from=package-zlib . /
ADD --checksum=sha256:${ZSTD_HASH} https://github.com/facebook/zstd/archive/v${ZSTD_VERSION}.tar.gz .
RUN --network=none <<-EOF
	set -eu
	tar -xf v${ZSTD_VERSION}.tar.gz
	cd zstd-${ZSTD_VERSION}/lib
	make DESTDIR=${DESTDIR} PREFIX=${PREFIX} install
EOF
FROM scratch as package-zstd
COPY --from=build-zstd /rootfs/ /

FROM base AS build
COPY --from=pallet-rust . /
COPY --from=package-rocksdb . /
COPY --from=package-zlib . /
COPY --from=package-zstd . /
COPY . .
RUN rm -rf /etc/clang
ENV RUSTFLAGS="-C codegen-units=1 -Clink-arg=--target=${TARGET} -Clink-arg=--gcc-install-dir=${GCC_INSTALL_DIR} -Clink-arg=-rtlib=libgcc -Clink-arg=-unwindlib=libgcc -Clink-arg=-stdlib=libstdc++"
ENV RUSTC_BOOTSTRAP=1
ENV CARGO_ARGS="-Zbuild-std=std,alloc,proc_macro,panic_abort -Zbuild-std-features=compiler-builtins-mem -Zunstable-options"
ENV ROCKSDB_LIB_DIR=${PREFIX}/lib
RUN cargo fetch ${CARGO_ARGS}
RUN --network=none <<-EOF
	cargo build \
		${CARGO_ARGS} \
		--target=${CARGO_TARGET} \
		--profile=${PROFILE} \
		--frozen \
		--artifact-dir=/rootfs \
		--bin sui-node
	install -D ${PREFIX}/lib64/libgcc_s.so.1 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib64/libstdc++.so.6.0.34 ${DESTDIR}/usr/lib/libstdc++.so.6
	install -D ${PREFIX}/lib/ld-linux-x86-64.so.2 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib/libz.so.1 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib/libzstd.so.1 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib/librocksdb.so.9.10 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib/libm.so.6 -t ${DESTDIR}/usr/lib/
	install -D ${PREFIX}/lib/libc.so.6 -t ${DESTDIR}/usr/lib/
	ln -s libc.so.6 ${DESTDIR}/usr/lib/libc.so
EOF
# HACK: sui-node seems to have a hard dependency on /bin/exec for no obvious reason.
# For now we include busybox until this is fixed in sui-node
COPY --from=core-busybox . ${DESTDIR}/

FROM scratch AS package
COPY --from=build /rootfs/ /
ENV LD_LIBRARY_PATH=/usr/lib
ENTRYPOINT ["/sui-node"]

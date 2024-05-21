#!/usr/bin/env bash
# this script toggles the llvm used on your machine. it's intended for linux only (ubuntu likely)
# 
#     --slave /usr/bin/$1 $1 /usr/bin/$1-\${version} \\

function register_clang_version {
    local version=$1
    local priority=$2

    update-alternatives \
       	--verbose \
        --install /usr/bin/llvm-config          llvm-config          /usr/bin/llvm-config-${version} ${priority} \
        --slave   /usr/bin/llvm-ar              llvm-ar              /usr/bin/llvm-ar-${version} \
        --slave   /usr/bin/llvm-as              llvm-as              /usr/bin/llvm-as-${version} \
        --slave   /usr/bin/llvm-bcanalyzer      llvm-bcanalyzer      /usr/bin/llvm-bcanalyzer-${version} \
        --slave   /usr/bin/llvm-c-test          llvm-c-test          /usr/bin/llvm-c-test-${version} \
        --slave   /usr/bin/llvm-cat             llvm-cat             /usr/bin/llvm-cat-${version} \
        --slave   /usr/bin/llvm-cfi-verify      llvm-cfi-verify      /usr/bin/llvm-cfi-verify-${version} \
        --slave   /usr/bin/llvm-cov             llvm-cov             /usr/bin/llvm-cov-${version} \
        --slave   /usr/bin/llvm-cvtres          llvm-cvtres          /usr/bin/llvm-cvtres-${version} \
        --slave   /usr/bin/llvm-cxxdump         llvm-cxxdump         /usr/bin/llvm-cxxdump-${version} \
        --slave   /usr/bin/llvm-cxxfilt         llvm-cxxfilt         /usr/bin/llvm-cxxfilt-${version} \
        --slave   /usr/bin/llvm-diff            llvm-diff            /usr/bin/llvm-diff-${version} \
        --slave   /usr/bin/llvm-dis             llvm-dis             /usr/bin/llvm-dis-${version} \
        --slave   /usr/bin/llvm-dlltool         llvm-dlltool         /usr/bin/llvm-dlltool-${version} \
        --slave   /usr/bin/llvm-dwarfdump       llvm-dwarfdump       /usr/bin/llvm-dwarfdump-${version} \
        --slave   /usr/bin/llvm-dwp             llvm-dwp             /usr/bin/llvm-dwp-${version} \
        --slave   /usr/bin/llvm-exegesis        llvm-exegesis        /usr/bin/llvm-exegesis-${version} \
        --slave   /usr/bin/llvm-extract         llvm-extract         /usr/bin/llvm-extract-${version} \
        --slave   /usr/bin/llvm-lib             llvm-lib             /usr/bin/llvm-lib-${version} \
        --slave   /usr/bin/llvm-link            llvm-link            /usr/bin/llvm-link-${version} \
        --slave   /usr/bin/llvm-lto             llvm-lto             /usr/bin/llvm-lto-${version} \
        --slave   /usr/bin/llvm-lto2            llvm-lto2            /usr/bin/llvm-lto2-${version} \
        --slave   /usr/bin/llvm-mc              llvm-mc              /usr/bin/llvm-mc-${version} \
        --slave   /usr/bin/llvm-mca             llvm-mca             /usr/bin/llvm-mca-${version} \
        --slave   /usr/bin/llvm-modextract      llvm-modextract      /usr/bin/llvm-modextract-${version} \
        --slave   /usr/bin/llvm-mt              llvm-mt              /usr/bin/llvm-mt-${version} \
        --slave   /usr/bin/llvm-nm              llvm-nm              /usr/bin/llvm-nm-${version} \
        --slave   /usr/bin/llvm-objcopy         llvm-objcopy         /usr/bin/llvm-objcopy-${version} \
        --slave   /usr/bin/llvm-objdump         llvm-objdump         /usr/bin/llvm-objdump-${version} \
        --slave   /usr/bin/llvm-opt-report      llvm-opt-report      /usr/bin/llvm-opt-report-${version} \
        --slave   /usr/bin/llvm-pdbutil         llvm-pdbutil         /usr/bin/llvm-pdbutil-${version} \
        --slave   /usr/bin/llvm-PerfectShuffle  llvm-PerfectShuffle  /usr/bin/llvm-PerfectShuffle-${version} \
        --slave   /usr/bin/llvm-profdata        llvm-profdata        /usr/bin/llvm-profdata-${version} \
        --slave   /usr/bin/llvm-ranlib          llvm-ranlib          /usr/bin/llvm-ranlib-${version} \
        --slave   /usr/bin/llvm-rc              llvm-rc              /usr/bin/llvm-rc-${version} \
        --slave   /usr/bin/llvm-readelf         llvm-readelf         /usr/bin/llvm-readelf-${version} \
        --slave   /usr/bin/llvm-readobj         llvm-readobj         /usr/bin/llvm-readobj-${version} \
        --slave   /usr/bin/llvm-rtdyld          llvm-rtdyld          /usr/bin/llvm-rtdyld-${version} \
        --slave   /usr/bin/llvm-size            llvm-size            /usr/bin/llvm-size-${version} \
        --slave   /usr/bin/llvm-split           llvm-split           /usr/bin/llvm-split-${version} \
        --slave   /usr/bin/llvm-stress          llvm-stress          /usr/bin/llvm-stress-${version} \
        --slave   /usr/bin/llvm-strings         llvm-strings         /usr/bin/llvm-strings-${version} \
        --slave   /usr/bin/llvm-strip           llvm-strip           /usr/bin/llvm-strip-${version} \
        --slave   /usr/bin/llvm-symbolizer      llvm-symbolizer      /usr/bin/llvm-symbolizer-${version} \
        --slave   /usr/bin/llvm-tblgen          llvm-tblgen          /usr/bin/llvm-tblgen-${version} \
        --slave   /usr/bin/llvm-undname         llvm-undname         /usr/bin/llvm-undname-${version} \
        --slave   /usr/bin/llvm-xray            llvm-xray            /usr/bin/llvm-xray-${version}
        

    update-alternatives \
       	--verbose \
        --install /usr/bin/clang					clang					/usr/bin/clang-${version} ${priority} \
        --slave   /usr/bin/clang++					clang++					/usr/bin/clang++-${version}  \
        --slave   /usr/bin/clang-format				clang-format			/usr/bin/clang-format-${version}  \
        --slave   /usr/bin/clang-cpp				clang-cpp				/usr/bin/clang-cpp-${version} \
        --slave   /usr/bin/asan_symbolize			asan_symbolize			/usr/bin/asan_symbolize-${version} \
        --slave   /usr/bin/bugpoint					bugpoint				/usr/bin/bugpoint-${version} \
        --slave   /usr/bin/dsymutil					dsymutil				/usr/bin/dsymutil-${version} \
        --slave   /usr/bin/lld						lld						/usr/bin/lld-${version} \
        --slave   /usr/bin/ld.lld					ld.lld					/usr/bin/ld.lld-${version} \
        --slave   /usr/bin/lld-link					lld-link				/usr/bin/lld-link-${version} \
        --slave   /usr/bin/llc						llc						/usr/bin/llc-${version} \
        --slave   /usr/bin/lli						lli						/usr/bin/lli-${version} \
        --slave   /usr/bin/obj2yaml					obj2yaml				/usr/bin/obj2yaml-${version} \
        --slave   /usr/bin/opt						opt						/usr/bin/opt-${version} \
        --slave   /usr/bin/sanstats					sanstats				/usr/bin/sanstats-${version} \
        --slave   /usr/bin/verify-uselistorder		verify-uselistorder		/usr/bin/verify-uselistorder-${version} \
        --slave   /usr/bin/wasm-ld					wasm-ld					/usr/bin/wasm-ld-${version} \
        --slave   /usr/bin/yaml2obj					yaml2obj					/usr/bin/yaml2obj-${version}
        
}

register_clang_version $1 $2

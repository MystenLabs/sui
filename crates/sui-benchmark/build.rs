fn main()  {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=USE_TIDEHUNTER");
    println!("cargo::rustc-check-cfg=cfg(tidehunter)");
    if std::env::var("USE_TIDEHUNTER").is_ok() {
        println!("cargo::rustc-cfg=tidehunter");
    }
}

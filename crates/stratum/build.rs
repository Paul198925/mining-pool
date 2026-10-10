fn main() {
    println!("cargo:rerun-if-changed=c");
    // C11: neoscrypt.c defines its own `bool`, which is a keyword from C23 on.
    cc::Build::new().file("c/neoscrypt.c").std("c11").warnings(false).compile("pool_algos");

    // KawPoW: the ProgPoW implementation Ravencoin ships, behind a small C interface.
    let ethash = "c/ethash/crypto/ethash/lib";
    cc::Build::new()
        .include("c/ethash")
        .files(["keccak/keccak.c", "keccak/keccakf800.c", "keccak/keccakf1600.c", "ethash/primes.c"].map(|f| format!("{ethash}/{f}")))
        .opt_level(2)
        .warnings(false)
        .compile("pool_ethash_c");
    cc::Build::new()
        .cpp(true)
        .std("c++14")
        // Also in debug builds: unoptimized, one share check takes a noticeable part of a second.
        .opt_level(2)
        .include("c/ethash")
        .files([format!("{ethash}/ethash/ethash.cpp"), format!("{ethash}/ethash/progpow.cpp"), "c/kawpow_shim.cpp".into()])
        .warnings(false)
        .compile("pool_ethash");
}

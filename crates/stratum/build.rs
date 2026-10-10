fn main() {
    println!("cargo:rerun-if-changed=c");
    // C11: neoscrypt.c defines its own `bool`, which is a keyword from C23 on.
    cc::Build::new().file("c/neoscrypt.c").std("c11").warnings(false).compile("pool_algos");
}

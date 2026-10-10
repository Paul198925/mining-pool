fn main() {
    // sqlx::migrate! embeds the migrations at compile time; rebuild when one is added.
    println!("cargo:rerun-if-changed=../../migrations");
}

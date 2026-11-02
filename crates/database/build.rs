// sqlx::migrate! embeds migrations at compile time; without this, adding a new
// .sql file doesn't rebuild the crate and the binary ships stale migrations.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}

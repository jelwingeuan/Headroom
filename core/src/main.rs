//! The same entry point the app uses, as a plain binary for local testing.

fn main() {
    std::process::exit(headroom_core::cli::main(std::env::args().collect()));
}

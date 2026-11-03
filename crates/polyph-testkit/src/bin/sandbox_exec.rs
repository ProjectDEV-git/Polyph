//! The sandbox helper, built inside the test kit so tests can find it.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(polyph_sandbox::helper::main_with_args(args));
}

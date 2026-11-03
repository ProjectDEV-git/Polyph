//! `polyph-sandbox-exec`: the in-sandbox helper. See `polyph_sandbox::helper`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(polyph_sandbox::helper::main_with_args(args));
}

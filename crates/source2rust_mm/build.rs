mod builder;

fn main() {
    if let Err(error) = builder::run() {
        eprintln!("source2rust build failed: {error}");
        std::process::exit(1);
    }
}

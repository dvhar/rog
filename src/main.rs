pub mod cli;
fn main() {
    let mut opts = rog::cli::Opts::new();
    rog::vm_tail(&mut opts);
}

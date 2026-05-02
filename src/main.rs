pub mod cli;
fn main() {
    let mut opts = rog::cli::Opts::new();
    if opts.client_mode {
        rog::client_mode(&mut opts);
    } else if opts.server_mode {
        rog::accept_clients(&mut opts);
        if opts.fifo {
            let path = opts.srvpath.clone().expect("fifo path required with -f");
            rog::vm_fifo(path, &mut opts);
        } else {
            rog::vm_tail(&mut opts);
        }
    } else if opts.fifo {
        let path = opts.srvpath.clone().expect("fifo path required with -f");
        rog::vm_fifo(path, &mut opts);
    } else {
        rog::vm_tail(&mut opts);
    }
}

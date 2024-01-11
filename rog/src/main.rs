use std::io::{self, Read, Write};
use std::sync::mpsc::{channel,Receiver, Sender};
use std::net::{ TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::process;
use std::fs;
use getopts::{Options,Matches};
use ctrlc;
extern crate getopts;
extern crate unix_named_pipe;


const BUFSZ: usize = 1024;

struct RingBuf {
    buf: [u8; BUFSZ],
    start: usize,
    len: usize,
}

impl RingBuf {
    fn new() -> Self {
        RingBuf {
            buf: [0u8; BUFSZ],
            start: 0,
            len: 0,
        }
    }

    fn read_from(&mut self, data: &[u8]) {
        let mut len = data.len();
        if len == 0 { return; }
        let readstart = if len > BUFSZ {
            let rs = len-BUFSZ;
            len = BUFSZ; rs } else { 0 };
        let source = &data[readstart..];
        let tip = (self.start + self.len) % BUFSZ;
        let write1 = std::cmp::min(len, BUFSZ - tip);
        self.buf[tip..tip+write1].copy_from_slice(&source[..write1]);
        if write1 == len {
            if (tip >= self.start) || (tip + write1 < self.start) {
                self.len = std::cmp::min(self.len + write1, BUFSZ);
                return;
            }
            self.len = BUFSZ;
            self.start = tip + write1;
            return;
        }
        let left = len - write1;
        self.buf[..left].copy_from_slice(&source[write1..]);
        if left < self.start {
            self.len += len;
            return
        }
        self.start = tip;
        self.len = BUFSZ;
    }

    fn read_to(&mut self, data: &mut [u8]) -> usize {
        if self.len == 0 {
            return 0;
        }
        let part1 = BUFSZ - self.start;
        if self.len <= part1 {
            data[..self.len].copy_from_slice(&self.buf[self.start..self.start+self.len]);
        } else {
            data[..part1].copy_from_slice(&self.buf[self.start..]);
            data[part1..self.len].copy_from_slice(&self.buf[..part1-self.len]);
        }
        let n =self.len;
        self.len = 0;
        self.start = 0;
        n
    }
}

fn sock_serve(rb: Arc<Mutex<RingBuf>>, rx_from_server: Receiver<i32>, tx_to_server: Sender<bool>) {
    let listener = match TcpListener::bind("0.0.0.0:19888") {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Could not bind to port. Is a server already running?");
            process::exit(0);
        },
    };
    let mut buf = [0u8; BUFSZ];
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut limit = 0;
                let mut limiting = false;
                match stream.read(&mut buf[..8]) {
                    Ok(n) if n == 8 => {
                        limit = usize::from_le_bytes(||->[u8;8]{buf[..8].try_into().unwrap()}());
                        if limit > 0 {
                            limiting = true;
                        }
                    },
                    Ok(n) => eprintln!("read control message does not have 8 bytes, has:{}", n),
                    Err(e) => {
                        eprintln!("read control message error:{}", e);
                        break;
                    },
                }
                tx_to_server.send(true).expect("snd from client");
                loop {
                    if let Err(e) = rx_from_server.recv() {
                        eprintln!("recv error:{}", e);
                        continue;
                    }
                    let mut n = rb.lock().unwrap().read_to(&mut buf);
                    tx_to_server.send(true).expect("snd from client");
                    if n == 0 {
                        continue;
                    }
                    if limiting && n >= limit {
                        n = limit;
                    }
                    if let Err(_) = stream.write_all(&buf[..n]) {
                        break;
                    }
                    if limiting {
                        limit -= n;
                        if limit == 0 {
                            if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                                eprintln!("read control message error:{}", e);
                            }
                            break;
                        }
                    }
                }           
                tx_to_server.send(false).expect("snd from client");
            }
            Err(e) => eprintln!("error opening stream: {}", e),
        }
    }
}


fn read_and_serve(input: Option<String>) {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx_to_client, rx_from_server) = channel::<i32>();
    let (tx_to_server, rx_from_client) = channel::<bool>();
    thread::spawn(move || sock_serve(rb_serv, rx_from_server, tx_to_server));

    if let Some(path) = input {
        unix_named_pipe::create("rogfifo", Some(0o666)).expect("could not create fifo");
        if let Err(e) = fs::rename("rogfifo", path.as_str()) {
            eprintln!("Move fifo error:{}",e);
            process::exit(1);
        }
        let delpath = path.clone();
        ctrlc::set_handler(move || {
            if let Err(e) = fs::remove_file(delpath.as_str()) {
                eprintln!("failed to remove fifo:{}", e);
            }
            process::exit(0);
        }).expect("Error setting Ctrl-C handler");
        loop {
            let file = fs::File::open(path.as_str()).expect("could not open fifo for reading");
            read_input(file, &tx_to_client, &rx_from_client, rb.clone());
        }
    } else {
        read_input(io::stdin(), &tx_to_client, &rx_from_client, rb);
    }
}

fn read_input<T: Read>(mut reader: T, tx_to_client: &Sender<i32>, rx_from_client: &Receiver<bool>, rb: Arc<Mutex<RingBuf>>) {
    let mut has_client = false;
    let mut buf = [0u8; BUFSZ];
    loop {
        match reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                if has_client {
                    if let Ok(v) = rx_from_client.recv() {
                        eprintln!("has client1:{}", v);
                        has_client = v;
                    }
                } else if let Ok(v) = rx_from_client.try_recv() {
                    eprintln!("has client2:{}", v);
                    has_client = v;
                }
                rb.lock().unwrap().read_from(&buf[0..n]);
                if let Err(e) = tx_to_client.send(0) {
                    eprintln!("send error:{}", e);
                    continue;
                }
            },
            Ok(_) => return,
            Err(e) => {
                eprintln!("read err:{}",e);
                return;
            }
        }
    }
}

fn client(opts: &Matches) {
    let host = opts.opt_str("i").unwrap_or("127.0.0.1".to_string());
    let mut stream = match TcpStream::connect(format!("{}:19888",host)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Counld not connect to {}. Error: {}", host, e);
            process::exit(0);
        },
    };
    let mut buf = [0; BUFSZ];
    let limit: usize = opts.opt_str("l").unwrap_or("0".to_string()).parse().unwrap();
    if let Err(e) = stream.write(&limit.to_le_bytes()) {
        eprintln!("Counld not send control message. Error: {}", e);
        process::exit(0);
    }
    loop {
        match stream.read(&mut buf) {
            Ok(n) if n == 0 => return,
            Ok(n) => io::stdout().write(&buf[..n]).expect("Write failed"),
            Err(e) => {
                eprintln!("error reading from stream:{}", e);
                return;
            },
        };
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = Options::new();
    opts.optopt("i", "ip", "ip of server", "HOST");
    opts.optopt("f", "fifo", "path of fifo to create and read from instead of stdin", "PATH");
    opts.optopt("l", "bytes", "number of bytes to read before exiting", "NUM");
    opts.optflag("s", "server", "server mode");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m },
        Err(e) => {
            eprintln!("bad args:{}", e);
            process::exit(1);
        },
    };
    if matches.opt_present("h") {
        println!("{}",opts.usage(&args[0]));
        process::exit(0);
    }
    if matches.opt_present("s") || matches.opt_present("f") {
        read_and_serve(matches.opt_str("f"));
        return;
    }
    client(&matches);
}

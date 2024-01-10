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
    front: usize,
    rear: usize,
    hasdata: bool,
}

impl RingBuf {
    fn new() -> Self {
        RingBuf {
            buf: [0u8; BUFSZ],
            front: 0,
            rear: 0,
            hasdata: false,
        }
    }

    fn read_from(&mut self, data: &[u8]) {
        let mut len = data.len();
        if len == 0 { return; }
        let mut readstart: usize = 0;
        if len > BUFSZ {
            readstart = len-BUFSZ;
            len = BUFSZ;
        }
        let source = &data[readstart..];
        let writeamount = std::cmp::min(len, BUFSZ - self.front);
        self.buf[self.front..self.front+writeamount].copy_from_slice(&source[..writeamount]);
		if writeamount == len {
			let nextfront = self.front + writeamount;
			if self.rear > self.front && self.rear < nextfront {
				self.rear = nextfront;
            }
			self.front = nextfront;
			self.hasdata = true;
			return;
		}
        self.buf[..len-writeamount].copy_from_slice(&source[writeamount..]);
        let nextfront = len - writeamount;
        if self.rear < nextfront {
            self.rear = nextfront;
        }
        self.front = nextfront;
        self.hasdata = true;
    }

    fn read_to(&mut self, data: &mut [u8]) -> usize {
        if !self.hasdata {
            return 0;
        }
        let n;
        if self.rear < self.front {
            n = self.front-self.rear;
            data[..n].copy_from_slice(&self.buf[self.rear..self.front]);
        } else {
            let a = BUFSZ-self.rear;
            n = a + self.front;
            data[..a].copy_from_slice(&self.buf[self.rear..]);
            data[a..n].copy_from_slice(&self.buf[..self.front]);
        }
        self.front = self.rear;
        self.hasdata = false;
        return n;
    }
}

fn sock_serve(rb: Arc<Mutex<RingBuf>>, rx: Receiver<i32>) {
    let listener = match TcpListener::bind("0.0.0.0:19888") {
        Ok(lsnr) => lsnr,
        Err(_) => {
            eprintln!("Could not bind to port. Is a server already running?");
            process::exit(0);
        },
    };
    let mut buf = [0u8; BUFSZ];
    for stream in listener.incoming() {
        let ringbuf = rb.clone();
        match stream {
            Ok(mut stream) => {
                loop {
                    if let Err(e) = rx.recv() {
                        eprintln!("recv error:{}", e);
                        continue;
                    }
                    let n = ringbuf.lock().unwrap().read_to(&mut buf);
                    if n == 0 {
                        continue;
                    }
                    if let Err(_) = stream.write_all(&buf[..n]) {
                        break;
                    }
                }           
            }
            Err(e) => eprintln!("error opening stream: {}", e),
        }
    }
}


fn read_and_serve(input: Option<String>) {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx, rx) = channel::<i32>();
    thread::spawn(move || sock_serve(rb_serv, rx));

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
            read_input(file, &tx, rb.clone());
        }
    } else {
        read_input(io::stdin(), &tx, rb);
    }
}

fn read_input<T: Read>(mut reader: T, tx: &Sender<i32>, rb: Arc<Mutex<RingBuf>>) {
    loop {
        let mut buf = [0u8; BUFSZ];
        match reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                rb.lock().unwrap().read_from(&buf[0..n]);
                if let Err(e) = tx.send(0) {
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

use std::io::{self, Read, Write};
use std::sync::mpsc::{channel,Receiver};
use std::net::{ TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::process;

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
            let mid = BUFSZ-self.front;
            n = mid + self.rear;
            data[0..mid].copy_from_slice(&self.buf[self.front..]);
            data[mid..n].copy_from_slice(&self.buf[..self.rear]);
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
            eprintln!("Could not bind to port. Is a rog server already running?");
            process::exit(0);
        },
    };
    let mut buf = [0u8; BUFSZ];
    for stream in listener.incoming() {
        let ringbuf = rb.clone();
        match stream {
            Ok(mut stream) => {
                loop {
                    rx.recv().unwrap();
                    let n = ringbuf.lock().unwrap().read_to(&mut buf);
                    if n == 0 {
                        continue;
                    }
                    if let Err(_) = stream.write_all(&buf[..n]) {
                        break;
                    }
                }           
            }
            Err(e) => eprintln!("{}", e),
        }
    }
}

fn read_and_serve() {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx, rx) = channel::<i32>();
    thread::spawn(move || sock_serve(rb_serv, rx));
    loop {
        let mut buf = [0u8; BUFSZ];
        match io::stdin().read(&mut buf) {
            Ok(n) if n > 0 => {
                rb.lock().unwrap().read_from(&buf[0..n]);
                tx.send(0).unwrap();
            },
            Ok(_) => {},
            Err(_) => return,
        }
    }
}

fn client(host: &str) {
    let mut stream = TcpStream::connect(format!("{}:19888",host)).expect("Connection failed");
    let mut buf = [0; BUFSZ];
    stream.write(b"hi").unwrap();
    loop {
        match stream.read(&mut buf) {
            Ok(n) if n == 0 => return,
            Ok(n) => io::stdout().write(&buf[..n]).expect("Write failed"),
            Err(_) => return,
        };
    }
}

fn main() {
    let host = "127.0.0.1";
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        read_and_serve();
        return;
    }
    client(host);
}

use std::io::{self, Read, Write};
use std::sync::mpsc::{channel,Receiver,Sender};
use std::net::{ TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::str;

const BUFSZ: usize = 1024;

struct RingBuf {
    buf: [u8; BUFSZ],
    front: usize,
    rear: usize,
    hasdata: bool,
    rx: Receiver<i32>,
    tx: Sender<i32>,
}

impl RingBuf {
    fn new() -> Self {
        let (atx, arx) = channel::<i32>();
        RingBuf {
            buf: [0u8; BUFSZ],
            front: 0,
            rear: 0,
            hasdata: false,
            tx: atx,
            rx: arx,
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
            self.tx.send(0).unwrap();
			return;
		}
        self.buf[..len-writeamount].copy_from_slice(&source[writeamount..]);
        let nextfront = len - writeamount;
        if self.rear < nextfront {
            self.rear = nextfront;
        }
        self.front = nextfront;
        self.hasdata = true;
        self.tx.send(0).unwrap();
    }

    fn out(&mut self, data: &mut Vec<u8>) {
        data.clear();
        if !self.hasdata {
            self.rx.recv().unwrap();
        }
        if self.rear < self.front {
            data.extend_from_slice(&self.buf[self.rear..self.front]);
        } else {
            data.extend_from_slice(&self.buf[self.front..]);
            data.extend_from_slice(&self.buf[..self.rear]);
        }
        self.front = self.rear;
        self.hasdata = false;
    }
}

fn sock_serve(rb: Arc<Mutex<RingBuf>>) {
    let listener = TcpListener::bind("0.0.0.0:19888").expect("BIND error");
    for stream in listener.incoming() {
        let ringbuf = rb.clone();
        match stream {
            Ok(mut stream) => {
                thread::spawn(move || {
                    let (mut ipcmsg, mut data) = ([0u8], Vec::new());
                    stream.read_exact(&mut ipcmsg).unwrap();
                    match ipcmsg[0] {
                        0 => loop {
                            ringbuf.lock().unwrap().out(&mut data);
                            if data.len() > 0 {
                                if let Err(e) = stream.write_all(&data) {
                                    eprintln!("write to stream: {}", e);
                                    break;
                                }
                            }
                        },
                        1 => {
                            ringbuf.lock().unwrap().out(&mut data);
                            if let Err(e) = stream.write_all(&data) {
                                eprintln!("write to stream: {}", e);
                            }
                        }
                        _ => (),
                    }
                });
            }
            Err(e) => eprintln!("{}", e),
        }
    }
}

fn readinput() {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    thread::spawn(move || sock_serve(rb_serv));
    loop {
        let mut buf = [0u8; BUFSZ];
        match io::stdin().read(&mut buf) {
            Ok(amount) => {
                if amount > 0 {
                    rb.lock().unwrap().read_from(&buf[0..amount]);
                }
            },
            Err(_) => {
                return
            }
        }
    }
}

fn sockclient(remotehost: &str) {
    let mut stream = TcpStream::connect(format!("{}:19888", remotehost)).expect("connect");
    let msg = 0u8;
    let mut data = Vec::new();
    if let Err(e) = stream.write(&[msg]) {
        eprintln!("send ctrl message: {}", e);
    }
    loop {
        match stream.read(&mut data) {
            Ok(n) => {
                if n == 0 {
                    break;
                }
                io::stdout().write_all(&data).unwrap();
            },
            Err(e) => eprintln!("read from socket err:{}",e),
        }
    }
}

fn main() {
    let host = "127.0.0.1";
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        readinput();
        return;
    }
    sockclient(host);
}

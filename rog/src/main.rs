use std::io::{self, Read, Write};
use std::net::{ TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 4433;
const BUFSZ: usize = 1024;

struct RingBuf<'a> {
    buf: Arc<Mutex<&'a [u8; BUFSZ]>>,
    front: usize,
    rear: usize,
}

impl RingBuf<'_> {
    fn new() -> Self {
        RingBuf {
            buf: Arc::new(Mutex::new(&[0u8; BUFSZ])),
            front: 0,
            rear: 0,
        }
    }

    fn clone(&self) -> Self {
        RingBuf {
            buf: self.buf.clone(),
            front: self.front,
            rear: self.rear,
        }
    }

    fn read(&mut self, data: &mut [u8]) {
        let len = data.len();
        if len == 0 {
            return;
        }
        let mut readstart: usize = 0;
        if len > BUFSZ {
            readstart = len-BUFSZ;
        }
        let writeamount = std::cmp::min(len, BUFSZ - self.front);
        let mut buf = self.buf.lock().unwrap();
        buf[self.front..].copy_from_slice(&data[readstart..writeamount]);
    }

    fn out(&mut self, data: &mut Vec<u8>) {
        let mut buf = self.buf.lock().unwrap();
        data.clear();
        if self.rear >= self.front {
            data.extend_from_slice(&buf[self.front..self.rear]);
        } else {
            data.extend_from_slice(&buf[self.front..]);
            data.extend_from_slice(&buf[..self.rear]);
        }
        self.front = self.rear;
    }
}

fn sock_serve(mut rb: RingBuf) {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT)).unwrap();
    for stream in listener.incoming() {
        let mut rb = rb.clone();
        match stream {
            Ok(mut stream) => {
                thread::spawn(move || {
                    let (mut ipcmsg, mut data) = ([0u8], Vec::new());
                    stream.read_exact(&mut ipcmsg).unwrap();
                    match ipcmsg[0] {
                        0 => loop {
                            rb.out(&mut data);
                            if let Err(e) = stream.write_all(&data) {
                                eprintln!("{}", e);
                                break;
                            }
                        },
                        1 => {
                            rb.out(&mut data);
                            if let Err(e) = stream.write_all(&data) {
                                eprintln!("{}", e);
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
    loop {
        let mut buf = [0u8; BUFSZ];
        io::stdin().read(&mut buf).unwrap();
    }
}

fn sockclient(remotehost: &str) {
    let mut stream = TcpStream::connect(format!("{}:{}",remotehost,PORT)).unwrap();
    let mut msg = 0u8;
    let mut data = Vec::new();
    stream.write_all(&[msg]).unwrap();
    loop {
        let n = stream.read(&mut data).unwrap();
        if n == 0 {
            break;
        }
        io::stdout().write_all(&data[..n]).unwrap();
    }
}

fn main() {
    let mut host = "localhost".to_string();
    let mut follow = true;
    let mut amount = 0;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-i" => host = std::env::args().nth(2).unwrap(),
            "-c" => amount = std::env::args().nth(2).unwrap().parse().unwrap(),
            "-t" => follow = false,
            _ => (),
        }
    }
    if amount == 0 {
        let rb = RingBuf::new();
        thread::spawn(move || sock_serve(rb));
        sockclient(host.as_str());
    } else {
        for _ in 0..amount {
            sockclient(host.as_str());
            thread::sleep(Duration::from_secs(1));
        }
    }
}

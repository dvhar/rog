#![feature(linked_list_remove)]
use std::io::{self, Read, Write, SeekFrom, Seek};
use std::cell::RefCell;
use std::collections::LinkedList;
use std::sync::mpsc::{channel,Receiver, Sender};
use std::net::{ TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::process;
use std::fs;
use getopts::{Options,Matches};
use ctrlc;
use bat::{Input, PrettyPrinter};
extern crate getopts;
extern crate unix_named_pipe;


const BUFSZ: usize = 1024*1024;

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
        let source = &data[if len>BUFSZ{let rs=len-BUFSZ;len=BUFSZ;rs}else{0}..];
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

struct Client {
    stream: TcpStream,
    limit: usize, 
    limited: bool,
}

impl Client {
    fn new(s: TcpStream) -> Self {
        Client {
            stream: s,
            limit: 0,
            limited: false,
        }
    }
}

fn write_outputs(rb: Arc<Mutex<RingBuf>>, clients: Arc<Mutex<LinkedList<Client>>>, rx: Receiver<i32>) {
    let removed = RefCell::new(LinkedList::<usize>::new());
    let mut buf = [0u8; BUFSZ];
    loop {
        if let Err(e) = rx.recv() {
            eprintln!("recv error:{}", e);
            continue;
        }
        {
            let mut cls = clients.lock().unwrap();
            if cls.is_empty() {
                continue;
            }
            let n = rb.lock().unwrap().read_to(&mut buf);
            if n == 0 {
                continue;
            }
            for (i, cl) in cls.iter_mut().enumerate() {
                let write_amount = if cl.limited && n >= cl.limit {
                    removed.borrow_mut().push_back(i);
                    cl.limit
                } else if cl.limited {
                    cl.limit -= n;
                    n
                } else {
                    n
                };
                if let Err(_) = cl.stream.write_all(&buf[..write_amount]) {
                    removed.borrow_mut().push_back(i);
                }
            }
            if !removed.borrow_mut().is_empty() {
                for i in removed.borrow_mut().iter() {
                    cls.remove(*i);
                }
                removed.borrow_mut().clear();
            }
        }
    }
}

fn sock_serve(rb: Arc<Mutex<RingBuf>>, rx: Receiver<i32>) {
    let listener = match TcpListener::bind("0.0.0.0:19888") {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Could not bind to port. Is a server already running?");
            process::exit(0);
        },
    };
    let clients = Arc::new(Mutex::new(LinkedList::<Client>::new()));
    let cls = clients.clone();
    thread::spawn(move || write_outputs(rb, clients, rx));
    let mut buf = [0u8; 8];
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let mut client = Client::new(stream);
                match client.stream.read(&mut buf[..8]) {
                    Ok(n) if n == 8 => {
                        client.limit = usize::from_le_bytes(||->[u8;8]{buf[..8].try_into().unwrap()}());
                        if client.limit > 0 {
                            client.limited = true;
                        }
                    },
                    Ok(n) => eprintln!("read control message does not have 8 bytes, has:{}", n),
                    Err(e) => {
                        eprintln!("read control message error:{}", e);
                        break;
                    },
                }
                cls.lock().unwrap().push_back(client);
            }
            Err(e) => eprintln!("error opening stream: {}", e),
        }
    }
}


fn read_and_serve(opts: &Matches) {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx, rx) = channel::<i32>();
    thread::spawn(move || sock_serve(rb_serv, rx));

    let mut path = opts.opt_str("f");
    if path == None {
        path = opts.opt_str("t");
    }

    if let Some(path) = path {
        if opts.opt_present("f") {
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
            loop {
                let mut file = fs::File::open(path.as_str()).expect("could not open file for reading");
                file.seek(SeekFrom::End(0)).unwrap();
                read_input(file, &tx, rb.clone());
            }
        }
    } else {
        read_input(io::stdin(), &tx, rb);
    }
}

fn read_input<T: Read>(mut reader: T, tx: &Sender<i32>, rb: Arc<Mutex<RingBuf>>) {
    let mut buf = [0u8; BUFSZ];
    loop {
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
    let color = opts.opt_present("c");
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
            Ok(n) => {
                if color {
                    PrettyPrinter::new()
                        .inputs(vec![
                            Input::from_bytes(&buf[..n])
                                .name("embedded.log")
                                .kind("Embedded")
                                .title("a log file")])
                        .print()
                        .unwrap();
                } else {
                    io::stdout().write(&buf[..n]).expect("Write failed");
                }
            },
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
    opts.optopt("f", "fifo", "path of fifo to create and read from", "PATH");
    opts.optopt("t", "file", "path of file to from instead of stdin", "PATH");
    opts.optopt("l", "bytes", "number of bytes to read before exiting", "NUM");
    opts.optflag("c", "color", "parse syntax");
    opts.optflag("s", "server", "server mode, defaults to reading stdin");
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
    if matches.opt_present("s") || matches.opt_present("f") || matches.opt_present("t") {
        read_and_serve(&matches);
        return;
    }
    client(&matches);
}
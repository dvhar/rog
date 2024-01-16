use {
std::collections::HashMap,
std::io::{self, Read, Write, SeekFrom, Seek, IsTerminal},
std::net::{ TcpListener, TcpStream},
std::process,
std::sync::mpsc::{channel, Receiver, Sender},
std::sync::{Arc, Mutex},
std::thread,
std::vec::Vec,
std::{fs,fs::File},
regex::Regex,
ansi_term::{Colour, Colour::*},
getopts::{Options, Matches},
ctrlc,
inotify::{Inotify, WatchMask, WatchDescriptor},
bat::{assets::HighlightingAssets, config::Config, controller::Controller, Input},
};


const BUFSZ: usize = 1024*1024;

struct Colorizer<'a> {
    conf: bat::config::Config<'a>,
    assets: bat::assets::HighlightingAssets,
}
impl<'b> Colorizer<'b> {
    fn new<'a>() -> Colorizer<'a> {
        Colorizer {
            conf: Config { colored_output: true, ..Default::default() },
            assets: HighlightingAssets::from_binary(),
        }
    }
    fn string(&mut self, buf: &[u8]) -> String {
        let controller = Controller::new(&self.conf, &self.assets);
        let mut out = String::new();
        let input = Input::from_bytes(buf).name("dummy.log");
        if let Err(e) = controller.run(vec![input.into()], Some(&mut out)) {
            eprintln!("Error highlighting synatx:{}", e);
        }
        out
    }
    fn print(&mut self, buf: &[u8]) {
        let controller = Controller::new(&self.conf, &self.assets);
        let input = Input::from_bytes(buf).name("dummy.log");
        if let Err(e) = controller.run(vec![input.into()], None) {
            eprintln!("Error highlighting synatx:{}", e);
        }
    }
}

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
        let source = &data[if len > BUFSZ {
            let n = len - BUFSZ;
            len = BUFSZ;
            n } else { 0 }..];
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
        let part1size = BUFSZ - self.start;
        if self.len <= part1size {
            data[..self.len].copy_from_slice(&self.buf[self.start..self.start+self.len]);
        } else {
            data[..part1size].copy_from_slice(&self.buf[self.start..]);
            data[part1size..self.len].copy_from_slice(&self.buf[..self.len-part1size]);
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

fn write_outputs(rb: Arc<Mutex<RingBuf>>, clients: Arc<Mutex<Vec<Client>>>, rx: Receiver<i32>) {
    let mut removed = Vec::<usize>::new();
    let mut buf = [0u8; BUFSZ];
    loop {
        if let Err(e) = rx.recv() {
            eprintln!("recv error:{}", e);
            continue;
        }
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
                removed.push(i);
                cl.limit
            } else if cl.limited {
                cl.limit -= n;
                n
            } else {
                n
            };
            if let Err(_) = cl.stream.write_all(&buf[..write_amount]) {
                removed.push(i);
            }
        }
        if !removed.is_empty() {
            for i in removed.iter() {
                cls.remove(*i);
            }
            removed.clear();
        }
    }
}

fn sock_serve(rb: Arc<Mutex<RingBuf>>, rx: Receiver<i32>, tx: Sender<i32>) {
    let listener = match TcpListener::bind("0.0.0.0:19888") {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Could not bind to port. Is a server already running?");
            process::exit(0);
        },
    };
    let clients = Arc::new(Mutex::new(Vec::<Client>::new()));
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
                cls.lock().unwrap().push(client);
                if let Err(e) = tx.send(0) {
                    eprintln!("Error sending client first read channel:{}", e);
                }
            }
            Err(e) => eprintln!("error opening stream: {}", e),
        }
    }
}


fn read_and_serve(opts: &Matches) {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx, rx) = channel::<i32>();
    let tx_serv = tx.clone();
    thread::spawn(move || sock_serve(rb_serv, rx, tx_serv));

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
                let file = File::open(path.as_str()).expect("could not open fifo for reading");
                read_input(file, &tx, rb.clone());
            }
        } else {
            loop {
                let mut file = File::open(path.as_str()).expect("could not open file for reading");
                if let Err(e) = file.seek(SeekFrom::End(0)) {
                    eprintln!("Error seeking file:{}", e);
                }
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
        eprintln!("Could not send control message. Error: {}", e);
        process::exit(0);
    }
    let mut colorizer = Colorizer::new();
    loop {
        match stream.read(&mut buf) {
            Ok(n) if n == 0 => return,
            Ok(n) => {
                if color {
                    colorizer.print(&buf[..n]);
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

struct FileColor {
    file: File,
    name: String,
}
impl FileColor {
    fn new(path: &str, f: File, i: usize) -> Self {
        static COLORS: [Colour;6] = [Red, Green, Yellow, Blue, Purple, Cyan];
        FileColor {
            file: f,
            name: if io::stdout().is_terminal() {
                COLORS[i % COLORS.len()].paint(path).to_string() }
            else { path.to_string() },
        }
    }
}

fn tail(opts: &Matches) {
    let not_files: Vec<&String> = opts.free.iter().filter(|path| match fs::metadata(path) {
                                           Err(_) => true, _ => false, }).collect();
    if !not_files.is_empty() {
        eprintln!("Args are not files:{}", not_files.iter().fold(String::new(),|acc,x|acc+" "+x));
        process::exit(0);
    }
    let mut ino = Inotify::init().expect("Error while initializing inotify instance");
    let mut files = HashMap::<WatchDescriptor,FileColor>::new();
    let multi = opts.free.len() > 1;
    for (i,path) in opts.free.iter().enumerate() {
        match ino.watches().add(path, WatchMask::MODIFY) {
            Ok(wd) => {
                let mut file = match File::open(path) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Error opening file:{}", e);
                        process::exit(1);
                    },
                };
                file.seek(SeekFrom::End(0)).unwrap();
                let _ = files.insert(wd, FileColor::new(path, file, i));
            },
            Err(e) => {
                eprintln!("Error adding watch:{}", e);
                process::exit(0);
            }
        }
    }
    let grep = if let Some(reg) = opts.opt_str("g") {
        match Regex::new(reg.as_str()) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("Regex error for {}:{}", reg, e);
                process::exit(1);
            },
        }
    } else { None };
    let mut color = if opts.opt_present("c") { Some(Colorizer::new()) } else { None };
    let mut evbuf = [0; 1024];
    let mut buf = [0; BUFSZ];
    loop {
        let events = ino.read_events_blocking(&mut evbuf).expect("Error while reading events");
        for e in events {
            if let Some(filecol) = files.get_mut(&e.wd) {
                let mut n = filecol.file.read(&mut buf).unwrap();
                if n == 0 {
                    filecol.file.seek(SeekFrom::Start(0)).unwrap();
                    n = filecol.file.read(&mut buf).unwrap();
                }
                if let Some(ref mut colorizer) = color {
                    if multi {
                        colorizer.string(&buf[..n]).lines().for_each(|line|{
                            if let Some(ref re) = grep {
                                if !re.is_match(line) {
                                    return;
                                }
                            }
                            println!("{}:{}", filecol.name, line);
                        });
                    } else {
                        if let Some(ref re) = grep {
                            colorizer.string(&buf[..n]).lines().for_each(|line|{
                                if !re.is_match(line) {
                                    return;
                                }
                                println!("{}", line);
                            });
                        } else {
                            colorizer.print(&buf[..n]);
                        }
                    }
                } else {
                    if multi {
                        unsafe { std::str::from_utf8_unchecked(&buf[..n]) }.lines().for_each(|line|{
                            if let Some(ref re) = grep {
                                if !re.is_match(line) {
                                    return;
                                }
                            }
                            println!("{}:{}", filecol.name, line);
                        });
                    } else {
                        if let Some(ref re) = grep {
                            unsafe { std::str::from_utf8_unchecked(&buf[..n]) }.lines().for_each(|line|{
                                if !re.is_match(line) {
                                    return;
                                }
                                println!("{}", line);
                            });
                        } else {
                            io::stdout().write(&buf[..n]).expect("Write failed");
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = Options::new();
    opts.optopt("i", "ip", "ip of server", "HOST");
    opts.optopt("f", "fifo", "path of fifo to create and read from", "PATH");
    opts.optopt("t", "file", "path of file to tail in server mode", "PATH");
    opts.optopt("l", "bytes", "number of bytes to read before exiting", "NUM");
    opts.optopt("g", "grep", "only show lines that match a pattern", "REGEX");
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
        println!("{}\n{}",opts.usage(&args[0]), "Use files as positional paramters to function the same as tail -f");
        process::exit(0);
    }
    if matches.opt_present("s") || matches.opt_present("f") || matches.opt_present("t") {
        read_and_serve(&matches);
        return;
    }
    if matches.free.is_empty() {
        client(&matches);
    } else {
        tail(&matches);
    }
}

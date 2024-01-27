use {
std::collections::HashMap,
std::io::{self, Read, Write, SeekFrom, Seek, IsTerminal},
std::net::{ TcpListener, TcpStream},
std::process,
std::mem,
std::sync::mpsc::{channel, Receiver, Sender},
std::sync::{Arc, Mutex},
std::thread,
std::collections::VecDeque,
std::{fs,fs::File},
regex::Regex,
ansi_term::{Colour, Colour::*},
getopts::Options,
ctrlc,
inotify::{Inotify, WatchMask, WatchDescriptor},
bat::{assets::HighlightingAssets, config::Config, controller::Controller, Input},
};


const BUFSZ: usize = 1024*1024;
const THEMES: [&'static str; 15] = [
 "Visual Studio Dark+",
 "Nord",
 "1337",
 "Monokai Extended",
 "Coldark-Dark",
 "DarkNeon",
 "Dracula",
 "OneHalfDark",
 "Solarized (dark)",
 "Sublime Snazzy",
 "TwoDark",
 "ansi",
 "base16",
 "gruvbox-dark",
 "zenburn"];

struct Colorizer<'a> {
    conf: bat::config::Config<'a>,
    assets: bat::assets::HighlightingAssets,
    themes: [String; 15],
    idx: usize,
}
impl<'b> Colorizer<'b> {
    fn new<'a>(theme: String) -> Colorizer<'a> {
        Colorizer {
            conf: Config {
                colored_output: true,
                theme: theme,
                ..Default::default() },
            assets: HighlightingAssets::from_binary(),
            themes: THEMES.map(|s|s.to_string()),
            idx: 0,
        }
    }
    fn string(&mut self, buf: &[u8], theme: Option<usize>) -> String {
        if let Some(idx) = theme {
            if idx != self.idx {
                mem::swap(&mut self.conf.theme, &mut self.themes[self.idx]);
                mem::swap(&mut self.conf.theme, &mut self.themes[idx]);
                self.idx = idx;
            }
        }
        let controller = Controller::new(&self.conf, &self.assets);
        let mut out = String::new();
        let input = Input::from_bytes(buf).name("dummy.log");
        if let Err(e) = controller.run(vec![input.into()], Some(&mut out)) {
            eprintln!("Error highlighting synatx:{}", e);
        }
        out
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

fn write_outputs(rb: Arc<Mutex<RingBuf>>, clients: Arc<Mutex<HashMap<i64,Client>>>, rx: Receiver<i32>) {
    let mut removed = Vec::<i64>::new();
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
        for cl in cls.iter_mut() {
            let write_amount = if cl.1.limited && n >= cl.1.limit {
                removed.push(*cl.0);
                cl.1.limit
            } else if cl.1.limited {
                cl.1.limit -= n;
                n
            } else {
                n
            };
            if let Err(_) = cl.1.stream.write_all(&buf[..write_amount]) {
                removed.push(*cl.0);
            }
        }
        if !removed.is_empty() {
            for i in removed.iter() {
                cls.remove(i);
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
    let clients = Arc::new(Mutex::new(HashMap::<i64,Client>::new()));
    let cls = clients.clone();
    thread::spawn(move || write_outputs(rb, clients, rx));
    let mut buf = [0u8; 8];
    let mut client_id: i64 = 0;
    for stream in listener.incoming() {
        client_id += 1;
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
                let _ = cls.lock().unwrap().insert(client_id, client);
                if let Err(e) = tx.send(0) {
                    eprintln!("Error sending client first read channel:{}", e);
                }
            }
            Err(e) => eprintln!("error opening stream: {}", e),
        }
    }
}


fn read_and_serve(opts: &Opts) {
    let rb = Arc::new(Mutex::new(RingBuf::new()));
    let rb_serv = rb.clone();
    let (tx, rx) = channel::<i32>();
    let tx_serv = tx.clone();
    thread::spawn(move || sock_serve(rb_serv, rx, tx_serv));

    if let Some(path) = &opts.srvpath {
        if opts.fifo {
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

fn client(opts: &mut Opts) {
    let mut stream = match TcpStream::connect(format!("{}:19888",opts.host)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Counld not connect to {}. Error: {}", opts.host, e);
            process::exit(0);
        },
    };
    let mut buf = [0; BUFSZ];
    if let Err(e) = stream.write(&opts.limit.to_le_bytes()) {
        eprintln!("Could not send control message. Error: {}", e);
        process::exit(0);
    }
    let dummy_name: String = Default::default();
    let mut printer = Printer::new(opts);
    let finder = BufParser::new(opts);
    loop {
        match stream.read(&mut buf) {
            Ok(n) if n == 0 => return,
            Ok(n) => finder.getiter(&buf[..n]).for_each(|chunk| printer.print(chunk, &dummy_name, None)),
            Err(e) => {
                eprintln!("error reading from stream:{}", e);
                return;
            },
        };
    }
}

struct FileInfo {
    file: File,
    name: String,
    rawname: String,
    lastsize: u64,
    idx: usize,
}
impl FileInfo {
    fn new(name: &String, origpath: &String, f: File, idx: usize) -> Self {
        static COLORS: [Colour;6] = [Red, Green, Yellow, Blue, Purple, Cyan];
        let mut fi = FileInfo {
            file: f,
            rawname: origpath.clone(),
            name: if io::stdout().is_terminal() {
                COLORS[idx % COLORS.len()].paint(name).to_string() }
            else { name.to_string() },
            lastsize: 0,
            idx: idx,
        };
        fi.lastsize = fi.file.metadata().unwrap().len();
        fi
    }
    fn updatesize(&mut self) {
        let newsize = self.file.metadata().unwrap().len();
        if newsize < self.lastsize {
            eprintln!("file {} truncated", self.rawname);
            self.file.seek(SeekFrom::Start(0)).unwrap();
        }
        self.lastsize = newsize;
    }
}

fn tail(opts: &mut Opts) {
    let not_files: Vec<&String> = opts.argv.iter().filter(|path| match fs::metadata(path) {
                                           Err(_) => true, _ => false, }).collect();
    if !not_files.is_empty() {
        eprintln!("Args are not files:{}", not_files.iter().fold(String::new(),|acc,x|acc+" "+x));
        process::exit(0);
    }
    let prefixlen = opts.argv[0].chars().enumerate().take_while(|c| {
            opts.argv.iter().all(|s| match s.chars().nth(c.0) {
                    Some(k)=>k==c.1, None=>false})}).count();
    let mut trimmed: Vec<String> = opts.argv.iter().map(|s| s.chars().skip(prefixlen).collect()).collect();
    let maxlen = trimmed.iter().map(|s|s.len()).fold(0,|max,len|max.max(len));
    trimmed = trimmed.iter().map(|s|format!("{:<width$}", s, width=maxlen)).collect();
    let mut files = HashMap::<WatchDescriptor,FileInfo>::new();
    let mut ino = Inotify::init().expect("Error while initializing inotify instance");
    for (i,path) in opts.argv.iter().enumerate() {
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
                let _ = files.insert(wd, FileInfo::new(trimmed.iter().nth(i).unwrap(), path, file, i));
            },
            Err(e) => {
                eprintln!("Error adding watch:{}", e);
                process::exit(0);
            }
        }
    }
    let mut evbuf = [0; 1024];
    let mut buf = [0; BUFSZ];
    let mut printer = Printer::new(opts);
    let finder = BufParser::new(opts);
    loop {
        let events = ino.read_events_blocking(&mut evbuf).expect("Error while reading events");
        for e in events {
            if let Some(fi) = files.get_mut(&e.wd) {
                fi.updatesize();
                let n = fi.file.read(&mut buf).unwrap();
                if n == 0 {
                    continue;
                }
                finder.getiter(&buf[..n]).for_each(|chunk| printer.print(chunk, &fi.name, Some(fi.idx)));
            }
        }
    }
}


enum LineSource<'a,'b> {
    Regex(&'a Regex),
    Lines(std::str::Lines<'b>),
    NoParse,
}
struct LineSearcher<'a,'b> {
    strategy: LineSource<'a,'b>,
    buf: &'b str,
    posend: usize,
    posstart: usize,
    parent: &'a BufParser,
    group: VecDeque::<&'b str>,
}
impl<'a,'b> LineSearcher<'a,'b> {
    fn new(strategy: LineSource<'a,'b>, buf: &'b str, p: &'a BufParser) -> Self {
        LineSearcher {
            strategy: strategy,
            buf: buf,
            posend: 0,
            posstart: 0,
            parent: p,
            group: Default::default(),
        }
    }
    fn grep(&mut self) -> Option<&'b str> {
        if let LineSource::Regex(r) = self.strategy {
            match r.find_at(self.buf, self.posend) {
                Some(m) => {
                    let linestart = match self.buf[..m.start()].rfind('\n') {
                        Some(i) => i+1,
                        None => 0
                    };
                    let lineend = match self.buf[m.end()..].find('\n') {
                        Some(i) => i+m.end(),
                        None => self.buf.len()
                    };
                    self.posend = lineend;
                    self.posstart = linestart;
                    Some(&self.buf[linestart..lineend])
                },
                None => None,
            }
        } else {None}
    }
    fn grep_ctx_blob(&mut self) -> Option<&'b str> {
        self.grep()?;
        let buf = self.buf.as_bytes();
        let mut c0 = self.parent.ctx.0;
        let mut begin = self.posstart;
        while c0 > 0 && begin > 0 {
            if buf[begin-1] == '\n' as u8 { begin -= 1; continue; }
            c0 -= 1;
            begin = match self.buf[..begin].rfind('\n') { Some(i) => i, None => 0, };
        }
        if begin < buf.len() && buf[begin] == '\n' as u8 { begin += 1; }
        let mut c1 = self.parent.ctx.1;
        let mut end = self.posend;
        while c1 > 0 && end < buf.len() {
            if buf[end] == '\n' as u8 { end += 1; continue; }
            c1 -= 1;
            end = match self.buf[end..].find('\n') { Some(i) => i+end, None => buf.len(), };
        }
        if end > 0 && buf[end-1] == '\n' as u8 { end -= 1; }
        self.posend = end;
        Some(&self.buf[begin..end])
    }
    fn grep_ctx_lines(&mut self) -> Option<&'b str> {
        if self.group.is_empty() {
            let found = self.grep()?;
            self.group.push_front(found);
            let buf = self.buf.as_bytes();
            let mut c0 = self.parent.ctx.0;
            let mut begin = self.posstart;
            let mut end = begin;
            while c0 > 0 && begin > 0 {
                if buf[begin-1] == '\n' as u8 { begin -= 1; end = begin; continue; }
                c0 -= 1;
                begin = match self.buf[..begin].rfind('\n') { Some(i) => i+1, None => 0, };
                self.group.push_front(&self.buf[begin..end]);
                if begin > 0 {
                    begin -= 1;
                }
                end = begin;
            }
            end = self.posend;
            begin = end;
            let mut c1 = self.parent.ctx.1;
            while c1 > 0 && end < buf.len() {
                if buf[end] == '\n' as u8 { end += 1; begin = end; continue; }
                c1 -= 1;
                end = match self.buf[end..].find('\n') { Some(i) => i+end, None => buf.len(), };
                self.group.push_back(&self.buf[begin..end]);
            }
        }
        self.group.pop_front()
    }
}
impl<'a,'b> Iterator for LineSearcher<'a,'b> {
    type Item = &'b str;
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.strategy {
            LineSource::Regex(_) => {
                match (self.parent.ctx, self.parent.print_names) {
                    ((0,0),_) => self.grep(),
                    ((_,_),true) => self.grep_ctx_lines(),
                    ((_,_),false) => self.grep_ctx_blob(),
                }
            },
            LineSource::Lines(l) => l.next(),
            LineSource::NoParse => {
                if self.posend == 0 {
                    self.posend = 1;
                    Some(self.buf)
                } else { None }
            },
        }
    }
}

struct Printer<'c> {
    color: Option<Colorizer<'c>>,
    print_names: bool,
}
impl<'c> Printer<'c> {

    fn new(opts: &mut Opts) -> Self {
        let multi = opts.argv.len() > 1;
        Printer {
            color: match &mut opts.theme {
                Some(theme) => Some(Colorizer::new(mem::take(theme))),
                None => None,
            },
            print_names: multi,
        }
    }

    fn print<'a,'b>(self: &'a mut Self, chunk: &'b str, name: &String, idx: Option<usize>) {
        match (self.print_names, &mut self.color) {
            (true,Some(ref mut colorizer)) => println!("{}:{}", name, colorizer.string(chunk.as_bytes(), idx)),
            (false,Some(ref mut colorizer)) => {
                if chunk.ends_with('\n') {
                    print!("{}",colorizer.string(chunk.as_bytes(), idx));
                    io::stdout().flush().unwrap();
                } else {
                    println!("{}",colorizer.string(chunk.as_bytes(), idx));
                }
            },
            (true,None) => println!("{}:{}", name, chunk),
            (false,None) => {
                if chunk.ends_with('\n') {
                    print!("{}", chunk);
                    io::stdout().flush().unwrap();
                } else {
                    println!("{}", chunk);
                }
            },
        }
    }
}


struct BufParser {
    re: Option<Regex>,
    print_names: bool,
    ctx: (usize,usize),
}

impl BufParser {

    fn getiter<'a,'b>(self: &'a Self, buf: &'b [u8]) -> LineSearcher<'a,'b> {
        let buf = unsafe { std::str::from_utf8_unchecked(buf) };
        match (&self.re, self.print_names) {
            (Some(r), _) => LineSearcher::new(LineSource::Regex(&r),buf, &self),
            (None, true) => LineSearcher::new(LineSource::Lines(buf.lines()), buf, &self),
            (None, false) => LineSearcher::new(LineSource::NoParse, buf, &self),
        }
    }

    fn new<'a>(opts: &mut Opts) -> Self {
        let grep = if let Some(ref mut reg) = opts.grep {
            let search = if opts.word {
                format!("\\b{}\\b", reg)
                } else { mem::take(reg) };
            match Regex::new(search.as_str()) {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("Regex error for {}:{}", search, e);
                    process::exit(1);
                },
            }
        } else { None };
        BufParser {
            re: grep,
            print_names: opts.argv.len() > 1,
            ctx: (opts.bctx,opts.actx),
        }
    }
}

struct Opts {
    host: String,
    fifo: bool,
    srvpath: Option<String>,
    grep: Option<String>,
    word: bool,
    limit: usize,
    theme: Option<String>,
    actx: usize,
    bctx: usize,
    server: bool,
    argv: Vec<String>,
}
impl Opts {
    fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut opts = Options::new();
        opts.optopt("i", "ip", "ip of server", "HOST");
        opts.optopt("f", "fifo", "path of fifo to create and read from", "PATH");
        opts.optopt("t", "file", "path of file to tail in server mode", "PATH");
        opts.optopt("l", "bytes", "number of bytes to read before exiting", "NUM");
        opts.optopt("g", "grep", "only show lines that match a pattern", "REGEX");
        opts.optopt("w", "wgrep", "only show lines that match a pattern, with word boundary", "REGEX");
        opts.optopt("m", "theme", "colorscheme", "THEME");
        opts.optopt("C", "context", "Lines of context aroung grep results", "NUM");
        opts.optopt("A", "after", "Lines of context after grep results", "NUM");
        opts.optopt("B", "before", "Lines of context before grep results", "NUM");
        opts.optflag("c", "nocolor", "No syntax highlighting");
        opts.optflag("s", "server", "server mode, defaults to reading stdin");
        opts.optflag("h", "help", "print this help menu");
        let mut matches = match opts.parse(&args[1..]) {
            Ok(m) => { m },
            Err(e) => {
                eprintln!("bad args:{}", e);
                process::exit(1);
            },
        };
        if matches.opt_present("h") {
            println!("{}\n{}",opts.usage(&args[0]), "Use files as positional paramters to function the same as tail -f.\nUse -[f,s,t] to serve.\nOtherwise read from rog server and print.");
            process::exit(0);
        }
        let color = match (matches.opt_present("c"), matches.opt_present("m")) {
            (true,true) => panic!("Cannot use 'c' with 'm'"),
            (true,false) => None,
            (false,true) => matches.opt_str("m"),
            (false,false) =>  Some("Visual Studio Dark+".to_string()),
        };
        let c = matches.opt_str("C").unwrap_or("0".to_string());
        Opts {
            host: matches.opt_str("i").unwrap_or("127.0.0.1".to_string()),
            fifo: matches.opt_present("f"),
            srvpath: matches.opt_str("f").map_or(matches.opt_str("t"),|t|Some(t)),
            grep: matches.opt_str("w").map_or(matches.opt_str("g"),|g|Some(g)),
            word: matches.opt_present("w"),
            limit: matches.opt_str("l").unwrap_or("0".to_string()).parse().unwrap(),
            theme: color,
            actx: matches.opt_str("A").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            bctx: matches.opt_str("B").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            server: matches.opt_present("s") || matches.opt_present("f") || matches.opt_present("t"),
            argv: mem::take(&mut matches.free),
        }
    }
}

fn main() {
    let mut opts = Opts::new();
    if opts.server {
        read_and_serve(&opts);
    } else if opts.argv.is_empty() {
        client(&mut opts);
    } else {
        tail(&mut opts);
    }
}

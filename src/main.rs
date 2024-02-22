use {
std::borrow::Cow,
std::collections::{HashMap,HashSet},
std::cell::RefCell,
std::io::{self, Read, Write, SeekFrom, Seek, IsTerminal},
std::net::{ TcpListener, TcpStream},
std::process,
std::mem,
std::sync::mpsc::{channel, Receiver, Sender},
std::sync::{Arc, Mutex},
std::thread,
std::collections::VecDeque,
std::{fs,fs::File},
regex::{Regex,RegexBuilder},
termsize,
ansi_term::{Colour, Colour::*},
getopts::Options,
ctrlc,
inotify::{Inotify, WatchMask, WatchDescriptor},
bat::{assets::HighlightingAssets, config::Config, controller::Controller, Input},
};

macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

const BUFSZ: usize = 1024*1024;
const THEMES: [&'static str; 13] = [
 "Visual Studio Dark+",
 "1337",
 "DarkNeon",
 "Dracula",
 "Coldark-Dark",
 "Monokai Extended",
 "Sublime Snazzy",
 "TwoDark",
 "base16",
 "gruvbox-dark",
 "Nord",
 "zenburn",
 "ansi",
 ];

struct Colorizer<'a> {
    conf: bat::config::Config<'a>,
    assets: bat::assets::HighlightingAssets,
    themes: [String; 13],
    idx: usize,
}
impl<'b> Colorizer<'b> {
    fn new<'a>(theme: String) -> Colorizer<'a> {
        Colorizer {
            conf: Config {
                colored_output: true,
                theme,
                ..Default::default() },
            assets: HighlightingAssets::from_binary(),
            themes: THEMES.map(|s|s.to_string()),
            idx: 0,
        }
    }
    fn prep(&mut self, theme: Option<usize>) {
        if let Some(mut idx) = theme {
            idx %= THEMES.len();
            if idx != self.idx {
                mem::swap(&mut self.conf.theme, &mut self.themes[self.idx]);
                mem::swap(&mut self.conf.theme, &mut self.themes[idx]);
                self.idx = idx;
            }
        }
    }
    fn string(&mut self, buf: &[u8], theme: Option<usize>) -> String {
        self.prep(theme);
        let controller = Controller::new(&self.conf, &self.assets);
        let mut out = String::new();
        let input = Input::from_bytes(buf).name("a.log");
        if let Err(e) = controller.run(vec![input.into()], Some(&mut out)) {
            eprintln!("Error highlighting synatx:{}", e);
        }
        out
    }
    fn print(&mut self, buf: &[u8], theme: Option<usize>) {
        self.prep(theme);
        let controller = Controller::new(&self.conf, &self.assets);
        let input = Input::from_bytes(buf).name("a.log");
        if let Err(e) = controller.run(vec![input.into()], None) {
            eprintln!("Error highlighting synatx:{}", e);
        }
    }
}

struct Printer<'c> {
    color: Option<Colorizer<'c>>,
    inline_names: bool,
    header_names: bool,
    one_color: bool,
    prev_idx: usize,
    width: usize,
}
impl<'c> Printer<'c> {

    fn new(opts: &mut Opts) -> Self {
        let multi = opts.tailfiles.len() > 1;
        Printer {
            color: match &mut opts.theme {
                Some(theme) => Some(Colorizer::new(mem::take(theme))),
                None => None,
            },
            inline_names: opts.nameline && multi,
            header_names: !opts.nameline && multi,
            prev_idx: 1000,
            width: opts.width,
            one_color: opts.onetheme,
        }
    }

    fn print<'a,'b>(self: &'a mut Self, chunk: Cow<'b,str>, name: &String, mut idx: Option<usize>) {
        let mut out = match chunk {
            Cow::Borrowed(r) => r,
            Cow::Owned(ref s) => s.as_str(),
        };
        if self.width > 0 {
            let mut idx = self.width.min(out.len());
            while idx < out.len() && (out.as_bytes()[idx] & 0b11000000) == 0b10000000 {
                idx += 1;
            }
            out = &out[..idx];
        }
        match (self.header_names,idx) {
            (true,Some(i)) if i != self.prev_idx => {
                println!("\n  {}", name); 
                self.prev_idx = i;
            },
            (_,_) => {},
        }
        if self.one_color {
            idx = None;
        }
        match (self.inline_names, &mut self.color) {
            (true,Some(ref mut colorizer)) => write!(io::stdout(), "{}{}", name, colorizer.string(out.as_bytes(), idx)).unwrap(),
            (false,Some(ref mut colorizer)) => colorizer.print(out.as_bytes(), idx),
            (true,None) => write!(io::stdout(), "{}{}", name, out).unwrap(),
            (false,None) => io::stdout().write_all(out.as_bytes()).unwrap(),
        }
        io::stdout().write("\n".as_bytes()).unwrap();
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
        let write1 = len.min(BUFSZ - tip);
        self.buf[tip..tip+write1].copy_from_slice(&source[..write1]);
        if write1 == len {
            if (tip >= self.start) || (tip + write1 < self.start) {
                self.len = BUFSZ.min(self.len + write1);
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
    fn new(stream: TcpStream) -> Self {
        let mut client = Client {
            stream,
            limit: 0,
            limited: false,
        };
        let mut buf = [0u8; 8];
        match client.stream.read(&mut buf[..8]) {
            Ok(n) if n == 8 => {
                client.limit = usize::from_le_bytes(||->[u8;8]{buf[..8].try_into().unwrap()}());
                if client.limit > 0 {
                    client.limited = true;
                }
            },
            Ok(n) => eprintln!("read control message does not have 8 bytes, has:{}", n),
            Err(e) => eprintln!("read control message error:{}", e),
        }
        client
    }
}

fn write_to_clients(rb: Arc<Mutex<RingBuf>>, clients: Arc<Mutex<HashMap<i64,Client>>>, has_data: Receiver<i32>) {
    let mut removed = Vec::<i64>::new();
    let mut buf = [0u8; BUFSZ];
    loop {
        if let Err(e) = has_data.recv() {
            eprintln!("recv error:{}", e);
            continue;
        }
        let mut clientmap = clients.lock().unwrap();
        if clientmap.is_empty() {
            continue;
        }
        let n = rb.lock().unwrap().read_to(&mut buf);
        if n == 0 {
            continue;
        }
        for cl in clientmap.iter_mut() {
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
            removed.iter().for_each(|i|{clientmap.remove(i);});
            removed.clear();
        }
    }
}

fn server(rb: Arc<Mutex<RingBuf>>, has_data: Receiver<i32>, first_check: Sender<i32>) {
    let listener = match TcpListener::bind("0.0.0.0:19888") {
        Ok(l) => l,
        Err(_) => die!("Could not bind to port. Is a server already running?"),
    };
    let clients1 = Arc::new(Mutex::new(HashMap::<i64,Client>::new()));
    let clients2 = clients1.clone();
    thread::spawn(move || write_to_clients(rb, clients1, has_data));
    let mut client_id: i64 = 0;
    for stream in listener.incoming() {
        client_id += 1;
        match stream {
            Ok(stream) => {
                let _ = clients2.lock().unwrap().insert(client_id, Client::new(stream));
                if let Err(e) = first_check.send(0) {
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
    let (has_data_tx, has_data_rx) = channel::<i32>();
    let first_check = has_data_tx.clone();
    thread::spawn(move || server(rb_serv, has_data_rx, first_check));

    if let Some(path) = &opts.srvpath {
        if opts.fifo {
            unix_named_pipe::create("rogfifo", Some(0o666)).expect("could not create fifo");
            if let Err(e) = fs::rename("rogfifo", path.as_str()) {
                die!("Move fifo error:{}",e);
            }
            let watchpath = path.clone();
            thread::spawn(move || {
                while let Ok(_) = fs::metadata(&watchpath) {
                    thread::sleep(std::time::Duration::from_secs(3));
                }
                die!("Fifo deleted, exiting");
            });
            let delpath = path.clone();
            ctrlc::set_handler(move || {
                if let Err(e) = fs::remove_file(delpath.as_str()) {
                    eprintln!("failed to remove fifo:{}", e);
                }
                process::exit(0);
            }).expect("Error setting Ctrl-C handler");
            loop {
                let file = File::open(path.as_str()).expect("could not open fifo for reading");
                read_to_ringbuf(file, &has_data_tx, rb.clone());
            }
        } else {
            loop {
                let mut file = File::open(path.as_str()).expect("could not open file for reading");
                if let Err(e) = file.seek(SeekFrom::End(0)) {
                    eprintln!("Error seeking file:{}", e);
                }
                read_to_ringbuf(file, &has_data_tx, rb.clone());
            }
        }
    } else {
        read_to_ringbuf(io::stdin(), &has_data_tx, rb);
    }
}

fn read_to_ringbuf<T: Read>(mut reader: T, has_data: &Sender<i32>, rb: Arc<Mutex<RingBuf>>) {
    let mut buf = [0u8; BUFSZ];
    loop {
        match reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                rb.lock().unwrap().read_from(&buf[0..n]);
                if let Err(e) = has_data.send(0) {
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
        Err(e) => die!("Counld not connect to {}. Error: {}", opts.host, e),
    };
    let mut buf = [0; BUFSZ];
    if let Err(e) = stream.write(&opts.limit.to_le_bytes()) {
        die!("Could not send control message. Error: {}", e);
    }
    let dummy_name: String = Default::default();
    let mut printer = Printer::new(opts);
    let finder = BufParser::new(opts);
    loop {
        match stream.read(&mut buf) {
            Ok(n) if n == 0 => return,
            Ok(mut n) => {
                while n < buf.len()-1 && buf[n-1] != b'\n' {
                    n += stream.read(&mut buf[n..]).unwrap();
                }
                finder.getiter(&buf[..n]).for_each(|chunk| printer.print(chunk, &dummy_name, None));
                io::stdout().flush().unwrap();
            },
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
    fn new(name: &String, origpath: &String, f: File, idx: usize, opts: &Opts) -> Self {
        static COLORS: [Colour;8] = [Cyan, Red, Purple, Yellow, Green, Red, Purple, Green];
        let mut fi = FileInfo {
            file: f,
            rawname: origpath.clone(),
            name: if io::stdout().is_terminal() {
                if !opts.nameline && opts.theme != None {
                    Red.bold().paint(name).to_string()
                } else {
                    COLORS[idx % COLORS.len()].bold().on(Colour::RGB(20,15,10)).paint(name).to_string()
                }
            } else { name.to_string() },
            lastsize: 0,
            idx,
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
    let not_files: Vec<&String> = opts.tailfiles.iter().filter(|path| match fs::metadata(path) {
                                           Err(_) => true, _ => false, }).collect();
    if !not_files.is_empty() {
        die!("Args are not files:{}", not_files.iter().fold(String::new(),|acc,x|acc+" "+x));
    }
    let prefixlen = if !opts.nameline { 0 } else {
        opts.tailfiles[0].chars().enumerate().take_while(|c| {
            opts.tailfiles.iter().all(|s| match s.chars().nth(c.0) { Some(k)=>k==c.1, None=>false})}).count()};
    let mut formatted_names: Vec<String> = opts.tailfiles.iter().map(|s| s.chars().skip(prefixlen).collect()).collect();
    if opts.nameline {
        let maxlen = formatted_names.iter().map(|s|s.len()).fold(0,|max,len|max.max(len));
        if opts.termfit && opts.width > maxlen+1 {
            opts.width -= maxlen+1
        };
        formatted_names = formatted_names.iter().map(|s|format!("{:<width$}:", s, width=maxlen)).collect();
    } else {
        formatted_names = formatted_names.iter().map(|s|format!("===> {} <===", s)).collect();
    }
    let mut files = HashMap::<WatchDescriptor,FileInfo>::new();
    let mut ino = Inotify::init().expect("Error initializing inotify instance");
    for (i,path) in opts.tailfiles.iter().enumerate() {
        match ino.watches().add(path, WatchMask::MODIFY) {
            Ok(wd) => {
                let mut file = match File::open(path) {
                    Ok(f) => f,
                    Err(e) => die!("Error opening file:{}", e),
                };
                file.seek(SeekFrom::End(0)).unwrap();
                let _ = files.insert(wd, FileInfo::new(&formatted_names[i], path, file, i, &opts));
            },
            Err(e) => die!("Error adding watch:{}", e),
        }
    }
    let mut evbuf = [0; 1024];
    let mut buf = [0; BUFSZ];
    let mut printer = Printer::new(opts);
    let finder = BufParser::new(opts);
    if opts.tailfiles.is_empty() {
        let dummy_name = "".to_string();
        loop {
            let mut n = io::stdin().read(&mut buf).unwrap();
            if n == 0 { continue; }
            while n < buf.len()-1 && buf[n-1] != b'\n' {
                n += io::stdin().read(&mut buf[n..]).unwrap();
            }
            finder.getiter(&buf[..n]).for_each(|chunk| printer.print(chunk, &dummy_name, None));
        }
    }
    loop {
        let events = ino.read_events_blocking(&mut evbuf).expect("Error reading events");
        for e in events {
            if let Some(fi) = files.get_mut(&e.wd) {
                fi.updatesize();
                let mut n = fi.file.read(&mut buf).unwrap();
                if n == 0 { continue; }
                while n < buf.len()-1 && buf[n-1] != b'\n' {
                    n += fi.file.read(&mut buf[n..]).unwrap();
                }
                finder.getiter(&buf[..n]).for_each(|chunk| printer.print(chunk, &fi.name, Some(fi.idx)));
                io::stdout().flush().unwrap();
            }
        }
    }
}

enum LineSource<'a,'b> {
    Regex(&'a Regex),
    Lines(RefCell<std::str::Lines<'b>>),
    Blob,
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
            strategy,
            buf,
            posend: 0,
            posstart: 0,
            parent: p,
            group: Default::default(),
        }
    }

    fn vgrep_match(&self, line: &str) -> bool {
        match &self.parent.vgrep {
            Some(re) => re.is_match(line),
            None => false,
        }
    }

    fn grep_line(&mut self) -> Option<&'b str> {
        if let LineSource::Regex(r) = self.strategy {
            loop {
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
                        let line = &self.buf[linestart..lineend];
                        if self.vgrep_match(line) {
                            continue;
                        }
                        return Some(line)
                    },
                    None => return None,
                }
            }
        } else {None}
    }

    fn grep_ctx_blob(&mut self) -> Option<&'b str> {
        self.grep_line()?;
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
            let found = self.grep_line()?;
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

    fn remove_fields(&self, line: &'b str) -> Option<Cow<'b,str>> {
        let fields = match &self.parent.rem_fields {
            None => return Some(Cow::Borrowed(line)),
            Some(re) => re.find_iter(line),
        };
        let mut result = String::new();
        let mut start: usize = 0;
        fields.for_each(|pos| {
            if start < pos.start() {
                result += &line[start..pos.start()];
                start = pos.end();
            }
        });
        if start == 0 {
            return Some(Cow::Borrowed(line))
        }
        result += &line[start..];
        Some(Cow::Owned(result))
    }

    fn getline(&mut self) -> Option<&'b str> {
        if let LineSource::Lines(l) = &self.strategy {
            loop {
                let maybe_line = l.borrow_mut().next();
                match maybe_line {
                    None => return None,
                    Some(line) => {
                        if self.vgrep_match(line) {
                            continue;
                        }
                        return maybe_line
                    },
                }
            }
        }
        None
    }

    fn getblob(&mut self) -> Option<&'b str> {
        if self.posstart < self.buf.len() {
            let blob = if self.buf.ends_with('\n') && self.buf.len() > 1 {
                &self.buf[..self.buf.len()-1]
            } else {
                self.buf
            };
            match &self.parent.vgrep {
                None => {
                    self.posstart = self.buf.len();
                    return Some(blob);
                }
                Some(re) => loop {
                    let chunk = &blob[self.posstart..];
                    match re.find(chunk) {
                        None => {
                            self.posstart = self.buf.len();
                            return Some(chunk)
                        },
                        Some(m) => {
                            let cend = chunk.len();
                            let start = match chunk[..m.start()].rfind('\n') { Some(i) => i, None => 0 };
                            let end = match chunk[m.end()..].find('\n') { Some(i) => m.end()+i+1, None => cend };
                            if start == 0 {
                                if end == cend {
                                    return None
                                }
                                self.posstart += end;
                                continue;
                            }
                            self.posstart += end;
                            return Some(&chunk[..start])
                        }
                    }
                },
            };
        } else { None }
    }
}

impl<'a,'b> Iterator for LineSearcher<'a,'b> {
    type Item = Cow<'b,str>;
    fn next(&mut self) -> Option<Self::Item> {
        let res = match &mut self.strategy {
            LineSource::Regex(_) => {
                match (self.parent.ctx, self.parent.need_lines) {
                    ((0,0),_) => self.grep_line(),
                    ((_,_),true) => self.grep_ctx_lines(),
                    ((_,_),false) => self.grep_ctx_blob(),
                }
            },
            LineSource::Lines(_) => self.getline(),
            LineSource::Blob => self.getblob(),
        };
        self.remove_fields(res?)
    }
}

struct BufParser {
    grep: Option<Regex>,
    vgrep: Option<Regex>,
    need_lines: bool,
    ctx: (usize,usize),
    rem_fields: Option<Regex>,
}

impl BufParser {

    fn getiter<'a,'b>(self: &'a Self, buf: &'b [u8]) -> LineSearcher<'a,'b> {
        let buf = unsafe { std::str::from_utf8_unchecked(buf) };
        match (&self.grep, self.need_lines) {
            (Some(r), _) => LineSearcher::new(LineSource::Regex(&r),buf, &self),
            (None, true) => LineSearcher::new(LineSource::Lines(RefCell::new(buf.lines())), buf, &self),
            (None, false) => LineSearcher::new(LineSource::Blob, buf, &self),
        }
    }

    fn new<'a>(opts: &mut Opts) -> Self {
        let grep = if let Some(ref mut reg) = opts.grep {
            let search = if opts.word {
                format!("\\b{}\\b", reg)
                } else { mem::take(reg) };
            match RegexBuilder::new(search.as_str()).case_insensitive(opts.icase).build() {
                Ok(r) => Some(r),
                Err(e) => die!("Regex error for {}:{}", search, e),
            }
        } else { None };
        let vgrep = if let Some(ref reg) = opts.vgrep {
            match RegexBuilder::new(reg.as_str()).case_insensitive(opts.icase).build() {
                Ok(r) => Some(r),
                Err(e) => die!("Regex error for {}:{}", reg, e),
            }
        } else { None };
        let rem_fields = if !opts.fields.is_empty() {
            Some(Regex::new(format!(r#"\b({})=("[^"]*"|[^\s]*) ?"#, opts.fields.replace(",","|")).as_str())
                               .expect("Could not parse field name into regex"))
        } else { None };
        let need_lines = !opts.fields.is_empty() || opts.width > 0 || (opts.tailfiles.len() > 1 && opts.nameline);
        BufParser {
            grep,
            vgrep,
            need_lines,
            ctx: (opts.bctx,opts.actx),
            rem_fields,
        }
    }
}

fn read_presets(swizzle: String, opts: &mut Opts) {
    let rogrc = std::env::var("HOME").unwrap() + "/.config/rogrc";
    let rc_contents = match File::open(rogrc.clone()) {
        Ok(mut f) => {
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            s
        },
        Err(_) => {
            let mut rcfile = File::create(rogrc).unwrap();
            write!(rcfile, concat!("# Add command line args for default behavior and other presets.\n",
                "# Format is 'key = whatever flags and args you want'\n",
                "# 'key' can be 'default', a log file name, or a letter that you can then select with the -p flag.\n")).unwrap();
            "".to_string()
        },
    };
    if !rc_contents.is_empty() {
        let preset_map = rc_contents.lines().fold(HashMap::<&str,&str>::new(), |mut acc,line| {
            if line.starts_with('#') { return acc; }
            let mut vals = line.splitn(2, "=");
            acc.insert(
                str::trim(vals.next().unwrap()),
                vals.next().unwrap());
            acc
        });
        let argpat = Regex::new(r#"("[^"]+"|[^\s]+)"#).unwrap();
        // apply presets for defaults and params
        let mut presets = vec!["default".to_string()];
        swizzle.chars().for_each(|c| presets.push(c.to_string()));
        presets.iter().for_each(|key| {
            if let Some(argstr) = preset_map.get(key.as_str()) {
                let args = argpat.find_iter(argstr)
                    .map(|s|s.as_str().trim_matches(|c|c=='"'||c=='\'').to_string())
                    .collect::<Vec<String>>();
                if !args.is_empty() {
                    opts.merge(&mut Opts::newargs(&args, false));
                }
            }
        });
        // apply presets for files that could have been added by above params
        let tailfiles = mem::take(&mut opts.tailfiles);
        tailfiles.iter().map(|f|f.rsplitn(2,'/').next().unwrap()).for_each(|key| {
            if let Some(argstr) = preset_map.get(key) {
                let args = argpat.find_iter(argstr)
                    .map(|s|s.as_str().trim_matches(|c|c=='"'||c=='\'').to_string())
                    .collect::<Vec<String>>();
                if !args.is_empty() {
                    opts.merge(&mut Opts::newargs(&args, false));
                }
            }
        });
        opts.tailfiles = tailfiles;
    }
    if !opts.exclude.is_empty() {
        let re = Regex::new(format!("({})",opts.exclude.replace(",","|")).as_str()).unwrap();
        opts.tailfiles = opts.tailfiles.iter().filter(|x|!re.is_match(x)).map(|x|x.to_string()).collect();
    }
    //println!("FIN:{:#?}", opts);
}

#[derive(Debug)]
struct Opts {
    host: String,
    fifo: bool,
    srvpath: Option<String>,
    grep: Option<String>,
    vgrep: Option<String>,
    word: bool,
    icase: bool,
    limit: usize,
    width: usize,
    termfit: bool,
    theme: Option<String>,
    onetheme: bool,
    fields: String,
    exclude: String,
    actx: usize,
    bctx: usize,
    server: bool,
    nameline: bool,
    tailfiles: Vec<String>,
}
impl Opts {

    fn merge(self: &mut Self, other: &mut Opts) {
        if self.host.is_empty() { self.host = mem::take(&mut other.host); };
        if self.grep == None { self.grep = other.grep.take(); };
        if self.vgrep == None { self.vgrep = other.vgrep.take(); };
        if self.srvpath == None { self.srvpath = other.srvpath.take(); };
        if self.theme == None { self.theme = other.theme.take(); };
        self.fields = format!("{}{}{}",self.fields,
                              if !self.fields.is_empty() && !other.fields.is_empty() {","} else {""},
                              other.fields);
        self.fifo |= other.fifo;
        self.word |= other.word;
        self.icase |= other.icase;
        self.termfit |= other.termfit;
        self.onetheme |= other.onetheme;
        self.server |= other.server;
        self.nameline |= other.nameline;
        self.limit = self.limit.max(other.limit);
        self.width = self.width.max(other.width);
        self.actx = self.actx.max(other.actx);
        self.bctx = self.bctx.max(other.bctx);
        self.exclude = format!("{}{}{}",self.exclude,
                              if !self.exclude.is_empty() && !other.exclude.is_empty() {","} else {""},
                              other.exclude);
        self.tailfiles = self.tailfiles.iter().chain(other.tailfiles.iter())
            .collect::<HashSet<&String>>().iter().map(|s|s.to_string()).collect();
    }

    fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::newargs(&args, true)
    }
    fn newargs(args: &Vec<String>, recurse: bool) -> Self {
        let mut opts = Options::new();
        opts.optopt("I", "ip", "ip of server", "HOST");
        opts.optflag("k","client", "read from localhost in client mode");
        opts.optopt("f", "fifo", "path of fifo to create and read from", "PATH");
        opts.optopt("t", "file", "path of file to tail in server mode", "PATH");
        opts.optopt("l", "bytes", "number of bytes to read before exiting", "NUM");
        opts.optopt("o", "width", "limit number of bytes per row", "NUM");
        opts.optopt("r", "remove", "remove fields of the format field=value or field=\"value with spaces\"", "F1,F2,F3...");
        opts.optopt("g", "grep", "only show lines that match a pattern", "REGEX");
        opts.optopt("w", "wgrep", "only show lines that match a pattern, with word boundary", "REGEX");
        opts.optopt("v", "vrep", "only show lines that dont' match a pattern", "REGEX");
        opts.optopt("x", "exclude", "ignore file params that match a pattern", "REGEX");
        opts.optopt("m", "theme", "colorscheme", "THEME");
        opts.optopt("C", "context", "Lines of context aroung grep results", "NUM");
        opts.optopt("A", "after", "Lines of context after grep results", "NUM");
        opts.optopt("B", "before", "Lines of context before grep results", "NUM");
        opts.optopt("p", "presets", "Use preset args", "CHARS");
        opts.optflag("P", "nopreset", "Do not load presets from rc file");
        opts.optflag("i", "icase", "case insensitive grep");
        opts.optflag("u", "truncate", "truncate bytes that don't fit the terminal");
        opts.optflag("c", "nocolor", "No syntax highlighting");
        opts.optflag("n", "onecolor", "Uniform highlighting");
        opts.optflag("s", "server", "server mode, defaults to reading stdin");
        opts.optflag("b", "nameline", "put file name at the start of each line");
        opts.optflag("h", "help", "print this help menu");
        let mut matches = match opts.parse(args) {
            Ok(m) => { m },
            Err(e) => die!("bad args:{}", e),
        };
        if matches.opt_present("h") {
            die!("{}\n{}",opts.usage(&args[0]), "Use files as positional paramters to function the same as tail -f.\nUse -[f,s,t] to serve.\nOtherwise read from rog server and print.");
        }
        let theme = match (matches.opt_present("c"), matches.opt_present("m")) {
            (true,true) => die!("Cannot use 'c' with 'm'"),
            (true,false) => None,
            (false,true) => matches.opt_str("m"),
            (false,false) =>  Some("Visual Studio Dark+".to_string()),
        };
        let c = matches.opt_str("C").unwrap_or("0".to_string());
        let width = if matches.opt_present("u") {
            let termsize::Size { cols, .. } = termsize::get().unwrap();
            cols as usize
        } else {
            matches.opt_str("o").unwrap_or("0".to_string()).parse().unwrap()
        };
        let mut opts = Opts {
            host: matches.opt_str("I").unwrap_or(if matches.opt_present("k") {"127.0.0.1".to_string()} else {Default::default()}),
            fifo: matches.opt_present("f"),
            srvpath: matches.opt_str("f").map_or(matches.opt_str("t"),|t|Some(t)),
            grep: matches.opt_str("w").map_or(matches.opt_str("g"),|g|Some(g)),
            vgrep: matches.opt_str("v"),
            word: matches.opt_present("w"),
            icase: matches.opt_present("i"),
            limit: matches.opt_str("l").unwrap_or("0".to_string()).parse().unwrap(),
            termfit: matches.opt_present("u"),
            width,
            theme,
            fields: matches.opt_str("r").unwrap_or(Default::default()),
            actx: matches.opt_str("A").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            bctx: matches.opt_str("B").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            server: matches.opt_present("s") || matches.opt_present("f") || matches.opt_present("t"),
            nameline: matches.opt_present("b"),
            tailfiles: mem::take(&mut matches.free),
            exclude: matches.opt_str("x").unwrap_or(Default::default()),
            onetheme: matches.opt_present("n"),
        };
        if recurse && !matches.opt_present("P") {
            read_presets(matches.opt_str("p").unwrap_or("".to_string()), &mut opts);
        }
        opts
    }
}

fn main() {
    let mut opts = Opts::new();
    if opts.server {
        read_and_serve(&opts);
    } else if !opts.host.is_empty() {
        client(&mut opts);
    } else {
        tail(&mut opts);
    }
}

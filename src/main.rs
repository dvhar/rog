use {
std::collections::{HashMap,BTreeSet},
std::cell::RefCell,
std::io::{self, Read, Write, SeekFrom, Seek, IsTerminal},
std::mem,
std::sync::mpsc::Receiver,
std::{fs,fs::File},
std::path::PathBuf,
regex::{Regex,RegexBuilder},
termsize,
ansi_term::{Colour, Colour::*},
getopts::Options,
bat::{assets::HighlightingAssets, config::Config, controller::Controller, Input},
notify::{Event, Error, RecommendedWatcher, RecursiveMode, Watcher, event::{ModifyKind, DataChange}, EventKind::Modify},
ringbuffer,
ringbuffer::RingBuffer,
};

macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

#[macro_export]
macro_rules! dbug {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug_log")]
        {
            eprintln!($($arg)*);
        }
    }
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
#[allow(dead_code)]
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
                str::trim(match vals.next() { Some(v) => v, None => return acc, }),
                match vals.next() { Some(v) => v, None => return acc, });
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
            .collect::<BTreeSet<&String>>().iter().map(|s|s.to_string()).collect();
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

#[derive(Debug)]
#[allow(dead_code)]
enum Op {
    Jmp(usize),
    CtxJmp(usize),
    ReadNextFile(Receiver<Result<Event, Error>>),
    SliceWhole,
    SliceLine(usize),
    ReadStdin,
    PrintNameHeader,
    PrintPlain,
    PrintColor,
    Vgrep(Regex, usize),
    Grep(Regex, usize),
    RingbufAdd,
    BctxPrep,
    BctxNext,
}

fn runvm(ops: Vec<Op>, tailfiles: HashMap::<PathBuf,FileInfo>, opts: &mut Opts) {
    dbug!("OPS: {:?}", ops);
    let mut i = 0;
    let mut ax = 0;
    let mut bx = 0;
    let mut ax_store = 0;
    let mut bx_store = 0;
    let mut readbytes = 0;
    let mut buf = [0; BUFSZ];
    let mut paths: Vec<_> = Vec::new();
    let mut current_filename = String::new();
    let mut files = tailfiles;
    let mut prevfidx = 9999;
    let mut fidx = 0;
    let mut rb = ringbuffer::AllocRingBuffer::new(opts.bctx+1);
    let mut last_grepped: i64 = 0;
    let mut last_grepped_prev: i64 = 0;
    let mut line_num: i64 = 0;
    let mut ctx_count: u64 = 0;
    let color = match &mut opts.theme {
        Some(theme) => Some(RefCell::new(Colorizer::new(mem::take(theme)))),
        None => None,
    };
    loop {
        dbug!("OP:{:?}", ops[i]);
        match ops[i] {
            Op::Jmp(ip) => {
				i = ip;
				continue;
            },
            Op::ReadStdin => {
                bx = 0;
                match io::stdin().read(&mut buf) {
                    Ok(n) if n > 0 => readbytes = n,
                    Ok(_) => return,
                    Err(e) => eprintln!("read err:{}",e),
                }
            },
            Op::ReadNextFile(ref rx) => {
                bx = 0;
                if paths.len() == 0 {
                    let rev = rx.recv().unwrap();
                    match rev {
                        Ok(ev) if ev.kind == Modify(ModifyKind::Data(DataChange::Any)) => {
							paths = ev.paths;
						},
                        Ok(_) => { continue; },
                        Err(er) => eprintln!("EV Error:{}", er),
                    }
                }
				if let Some(path) = paths.pop() {
					if let Some(fi) = files.get_mut(&path) {
                        current_filename = fi.name.clone();
                        prevfidx = fidx;
                        fidx = fi.idx;
						fi.updatesize();
						let mut n = fi.file.read(&mut buf).unwrap();
						if n == 0 { continue; }
						while n < buf.len()-1 && buf[n-1] != b'\n' {
							n += fi.file.read(&mut buf[n..]).unwrap();
						}
                        readbytes = n;
					}
				}
            },
            Op::SliceWhole => {
                ax = 0;
                bx = readbytes;
            },
            Op::SliceLine(ip) => {
                if bx >= readbytes { i = ip; continue; }
                ax = bx;
                let nx = buf[bx..readbytes].iter().enumerate().find(|(_,b)|**b == b'\n');
                bx = if let Some(n) = nx { (bx+n.0+1).min(readbytes) } else { readbytes };
                line_num += 1;
            },
            Op::Vgrep(ref re, ip) => {
                if re.is_match(unsafe{str::from_utf8_unchecked(&buf[ax..bx])}) {
                    i = ip;
                    continue;
                }
            },
            Op::Grep(ref re, ip) => {
                if !re.is_match(unsafe{str::from_utf8_unchecked(&buf[ax..bx])}) {
                    i = ip;
                    continue;
                }
                last_grepped_prev = last_grepped;
                last_grepped = line_num;
            },
            Op::RingbufAdd => {
                dbug!("RingbufAdd: {}..{} '{}'", ax, bx, str::from_utf8(&buf[ax..bbx]).unwrap());
                rb.enqueue((ax,bx));
            },
            Op::BctxPrep => {
                ax_store = ax;
                bx_store = bx;
                ctx_count = (opts.bctx as u64).min((line_num-1) as u64);
                while rb.len() as i64 > (last_grepped - last_grepped_prev) {
                    eprintln!("B rb len: {}  {}  {}", rb.len(), last_grepped, last_grepped_prev);
                    rb.dequeue();
                }
            },
            Op::BctxNext => {
                dbug!("BN: {}", rb.len());
                (ax, bx) = rb.dequeue().unwrap_or((ax_store, bx_store));
                dbug!("BctxNext: {}..{} '{}'", ax, bx, str::from_utf8(&buf[ax..bx]).unwrap());
            },
            Op::CtxJmp(ip) => {
                if ctx_count > 0 {
                    ctx_count -= 1;
                    i = ip;
                    continue;
                }
            },
            Op::PrintNameHeader => {
                if fidx != prevfidx {
                    println!("\n  {}", current_filename); 
                }
            },
            Op::PrintPlain => {
                io::stdout().write_all(&buf[ax..bx]).unwrap();
            },
            Op::PrintColor => {
                color.as_ref().unwrap().borrow_mut().print(&buf[ax..bx], Some(fidx));
            },
        }
        i += 1;
    }
}

fn vm_tail(opts: &mut Opts) {
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
    let mut files = HashMap::<PathBuf,FileInfo>::new();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).unwrap();
    for (i,path) in opts.tailfiles.iter().enumerate() {
        match watcher.watch(path.as_ref(), RecursiveMode::NonRecursive) {
            Ok(()) => {
                let mut file = match File::open(path) {
                    Ok(f) => f,
                    Err(e) => die!("Error opening file:{}", e),
                };
                file.seek(SeekFrom::End(0)).unwrap();
                let pb = PathBuf::from(path);
				let pb = fs::canonicalize(pb).unwrap();
				println!("Watching path: {:?}", pb);
                let _ = files.insert(pb, FileInfo::new(&formatted_names[i], path, file, i, &opts));
            },
            Err(e) => die!("Error adding watch:{}", e),
        }
    }

	let mut ops = vec![
        if opts.tailfiles.is_empty() {
            Op::ReadStdin
        } else {
            Op::ReadNextFile(rx)
        },
        Op::SliceLine(0),
    ];
    if let Some(ref re) = opts.vgrep {
        match RegexBuilder::new(re.as_str()).case_insensitive(opts.icase).build() {
            Ok(r) => ops.push(Op::Vgrep(r, 1)),
            Err(e) => die!("Regex error for {}:{}", re, e),
        }
    }
    if let Some(ref re) = opts.grep {
        match RegexBuilder::new(re.as_str()).case_insensitive(opts.icase).build() {
            Ok(r) => {
                if opts.bctx > 0 {
                    ops.push(Op::RingbufAdd);
                }
                ops.push(Op::Grep(r, 1));
                if opts.bctx > 0 {
                    ops.push(Op::BctxPrep);
                    let ctx_start = ops.len();
                    ops.push(Op::BctxNext);
                    ops.push(Op::PrintColor);
                    ops.push(Op::CtxJmp(ctx_start));
                } else {
                    ops.push(Op::PrintColor);
                }
                ops.push(Op::Jmp(1));
            },
            Err(e) => die!("Regex error for {}:{}", re, e),
        }
    } else {
        ops.push(Op::PrintColor);
        ops.push(Op::Jmp(1));
    }

	runvm(ops, files, opts);
}

fn main() {
    let mut opts = Opts::new();
    vm_tail(&mut opts);
}

use {
    std::collections::HashMap,
    std::cell::RefCell,
    std::io::{self, Read, Write, SeekFrom, Seek, IsTerminal},
    std::mem,
    std::sync::mpsc::Receiver,
    std::{fs, fs::File},
    std::path::PathBuf,
    regex::{Regex, RegexBuilder},
    ansi_term::{Colour, Colour::*},
    notify::{Event, Error, RecommendedWatcher, RecursiveMode, Watcher, event::{ModifyKind, DataChange}, EventKind::Modify},
    ringbuffer,
    ringbuffer::RingBuffer,
};
pub mod cli;
use cli::{Opts,Colorizer};

#[macro_export]
macro_rules! dbug {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug_log")]
        {
            eprintln!($($arg)*);
        }
    }
}

#[macro_export]
macro_rules! dbug_ops {
    ($ops:expr) => {
        #[cfg(feature = "debug_log")]
        {
            eprintln!("OPS:");
            for (i, op) in $ops.iter().enumerate() {
                eprintln!("  {:2}: {:#?}", i, op);
            }
        }
    }
}

pub const BUFSZ: usize = 1024*1024;

pub struct JumpPositions {
    jumps: HashMap<i64,i64>,
    unique_key: i64,
}
impl JumpPositions {
    pub fn new() -> Self {
        Self {
            unique_key: -1,
            jumps: Default::default(),
        }
    }
    pub fn new_placeholder(&mut self) -> i64 {
        self.unique_key -= 1;
        self.unique_key
    }
    pub fn set_place(&mut self, placeholder: i64, val: usize) {
        self.jumps.insert(placeholder, val as i64);
    }
    pub fn update_ops(&self, ops: &mut Vec<Op>) {
        for op in ops.iter_mut(){
            match op {
                Op::Vgrep(_, ref mut idx) => {
                    if *idx < 0 {
                        *idx = *self.jumps.get(idx).expect("error in placeholder");
                    }
                },
                _ => {},
            }
        };
    }
}

pub struct FileInfo {
    pub file: File,
    pub name: String,
    pub rawname: String,
    pub lastsize: u64,
    pub idx: usize,
}
impl FileInfo {
    pub fn new(name: &String, origpath: &String, f: File, idx: usize, opts: &Opts) -> Self {
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
    pub fn updatesize(&mut self) {
        let newsize = self.file.metadata().unwrap().len();
        if newsize < self.lastsize {
            eprintln!("file {} truncated", self.rawname);
            self.file.seek(SeekFrom::Start(0)).unwrap();
        }
        self.lastsize = newsize;
    }
}


#[derive(Debug)]
#[allow(dead_code)]
pub enum Op {
    Jmp(usize),
    CtxJmp(usize),
    ReadNextFile(Receiver<Result<Event, Error>>),
    SliceWhole,
    SliceLine(usize),
    ReadStdin,
    PrintNameHeader,
    PrintPlain,
    PrintColor,
    Vgrep(Regex, i64),
    Grep(Regex, usize),
    RingbufAdd,
    BctxPrep,
    BctxNext,
}

pub fn runvm(ops: Vec<Op>, tailfiles: HashMap::<PathBuf,FileInfo>, opts: &mut Opts) {
    dbug_ops!(ops);
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
        //dbug!("     OP:{:?}", ops[i]);
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
                rb.clear();
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
                        rb.clear();
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
                if re.is_match(unsafe{std::str::from_utf8_unchecked(&buf[ax..bx])}) {
                    line_num -= 1;
                    i = ip as usize;
                    continue;
                }
            },
            Op::Grep(ref re, ip) => {
                if !re.is_match(unsafe{std::str::from_utf8_unchecked(&buf[ax..bx])}) {
                    i = ip;
                    continue;
                }
                last_grepped_prev = last_grepped;
                last_grepped = line_num;
            },
            Op::RingbufAdd => {
                rb.enqueue((ax,bx));
            },
            Op::BctxPrep => {
                ax_store = ax;
                bx_store = bx;
                ctx_count = (opts.bctx as u64).min((line_num-1) as u64);
                while rb.len() as i64 > (last_grepped - last_grepped_prev) {
                    rb.dequeue();
                }
            },
            Op::BctxNext => {
                (ax, bx) = match rb.dequeue() {
                    Some((a,b)) => (a,b),
                    None => {
                        ctx_count = 0;
                        (ax_store, bx_store)
                    },
                };
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

pub fn vm_tail(opts: &mut Opts) {
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
    let mut jumps = JumpPositions::new();

    let print_op = if opts.theme == None { Op::PrintPlain } else { Op::PrintColor };
    let mut ops = vec![
        if opts.tailfiles.is_empty() {
            Op::ReadStdin
        } else {
            Op::ReadNextFile(rx)
        },
        Op::SliceLine(0),
    ];
    let mut maybe_vre = if let Some(ref re) = opts.vgrep {
        match RegexBuilder::new(re.as_str()).case_insensitive(opts.icase).build() {
            Ok(r) => Some(r),
            Err(e) => die!("Regex error for {}:{}", re, e),
        }
    } else {
        None
    };
    if opts.bctx == 0 && !matches!(maybe_vre, None) {
        ops.push(Op::Vgrep(maybe_vre.take().unwrap(), 1));
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
                    let mut vjump: i64 = 0;
                    if let Some(vre) = maybe_vre {
                        vjump = jumps.new_placeholder();
                        ops.push(Op::Vgrep(vre, vjump));
                    }
                    ops.push(print_op);
                    if vjump != 0 {
                        jumps.set_place(vjump, ops.len());
                    }
                    ops.push(Op::CtxJmp(ctx_start));
                } else {
                    ops.push(print_op);
                }
            },
            Err(e) => die!("Regex error for {}:{}", re, e),
        }
    } else {
        ops.push(print_op);
    }
    ops.push(Op::Jmp(1));

    jumps.update_ops(&mut ops);
    runvm(ops, files, opts);
}

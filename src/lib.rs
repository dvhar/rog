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
                Op::MatchJmp(ref mut idx) => {
                    if *idx < 0 {
                        *idx = *self.jumps.get(idx).expect("error in placeholder");
                    }
                },
                Op::BctxNext(ref mut idx) => {
                    if *idx < 0 {
                        *idx = *self.jumps.get(idx).expect("error in placeholder");
                    }
                },
                Op::ActxJmp(ref mut idx) => {
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
    ReadFifo(String),
    SliceWhole,
    SliceLine(usize),
    ReadStdin,
    PrintNameHeader,
    PrintPlain,
    PrintColor,
    Match(Regex),
    Invert,
    MatchJmp(i64),
    RingbufAdd,
    BctxPrep,
    BctxNext(i64),
    SetGrepPos,
    ActxJmp(i64),
    ActxReset(Regex),
    Truncate,
}
impl Clone for Op {
    fn clone(&self) -> Self {
        match self {
            Op::ReadNextFile(_) => unreachable!("ReadNextFile should never be cloned"),
            Op::ReadFifo(p) => Op::ReadFifo(p.clone()),
            Op::Jmp(v) => Op::Jmp(*v),
            Op::CtxJmp(v) => Op::CtxJmp(*v),
            Op::SliceLine(v) => Op::SliceLine(*v),
            Op::MatchJmp(v) => Op::MatchJmp(*v),
            Op::BctxNext(v) => Op::BctxNext(*v),
            Op::Match(re) => Op::Match(re.clone()),
            Op::ReadStdin => Op::ReadStdin,
            Op::SliceWhole => Op::SliceWhole,
            Op::PrintNameHeader => Op::PrintNameHeader,
            Op::PrintPlain => Op::PrintPlain,
            Op::PrintColor => Op::PrintColor,
            Op::Invert => Op::Invert,
            Op::RingbufAdd => Op::RingbufAdd,
            Op::BctxPrep => Op::BctxPrep,
            Op::SetGrepPos => Op::SetGrepPos,
            Op::ActxJmp(v) => Op::ActxJmp(*v),
            Op::ActxReset(re) => Op::ActxReset(re.clone()),
            Op::Truncate => Op::Truncate,
        }
    }
}

pub fn runvm(ops: Vec<Op>, tailfiles: HashMap::<PathBuf,FileInfo>, opts: &mut Opts) {
    dbug_ops!(ops);
    let mut i = 0;
    let mut ax = 0;
    let mut bx = 0;
    let mut pbx = 0; // printable_bx: may be truncated for width
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
    let mut actx: usize = 0;
    let mut matched = false;
    // fifo file handle: kept open across loop iterations; dropped on EOF to allow reconnect
    let mut fifo_file: Option<File> = None;
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
            Op::ReadFifo(ref path) => {
                bx = 0;
                // If we don't have an open handle (first open or previous EOF), open the fifo
                if fifo_file.is_none() {
                    match File::open(path) {
                        Ok(f) => fifo_file = Some(f),
                        Err(e) => {
                            eprintln!("Error opening fifo {}:{}. Will retry...", path, e);
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            continue;
                        }
                    }
                }
                // Read from the fifo
                match fifo_file.as_mut().unwrap().read(&mut buf) {
                    Ok(n) if n > 0 => {
                        readbytes = n;
                        rb.clear();
                    },
                    Ok(_) => {
                        // EOF: all writers closed. Drop the handle so we re-open next iteration.
                        fifo_file = None;
                        continue;
                    },
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Non-blocking: no data available yet. Sleep briefly and retry.
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    },
                    Err(e) => {
                        eprintln!("fifo read error:{}; reopening...", e);
                        fifo_file = None;
                        continue;
                    }
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
                pbx = bx;
                line_num += 1;
            },
            Op::Match(ref re) => {
                matched = re.is_match(unsafe{std::str::from_utf8_unchecked(&buf[ax..bx])});
            },
            Op::Invert => {
                matched = !matched;
            },
            Op::MatchJmp(ip) => {
                if matched {
                    i = ip as usize;
                    continue;
                }
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
                    ctx_count -= 1;
                }
            },
            Op::BctxNext(ip) => {
                (ax, bx, pbx) = match rb.dequeue() {
                    Some((a,b)) => {
                        let pbx_val = b;
                        (a, b, pbx_val)
                    },
                    None => {
                        ctx_count = 0;
                        i = ip as usize;
                        (ax_store, bx_store, bx_store)
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
            Op::SetGrepPos => {
                if matched {
                    last_grepped_prev = last_grepped;
                    last_grepped = line_num;
                    actx = opts.actx;
                }
            },
            Op::PrintNameHeader => {
                if fidx != prevfidx {
                    println!("\n  {}", current_filename); 
                }
            },
            Op::PrintPlain => {
                io::stdout().write_all(&buf[ax..pbx]).unwrap();
            },
            Op::PrintColor => {
                color.as_ref().unwrap().borrow_mut().print(&buf[ax..pbx], Some(fidx));
            },
            Op::ActxJmp(ip) => {
                if actx > 0 {
                    actx -= 1;
                    i = ip as usize;
                    continue;
                }
            },
            Op::ActxReset(ref re) => {
                if re.is_match(unsafe{std::str::from_utf8_unchecked(&buf[ax..bx])}) {
                    actx = opts.actx;
                    last_grepped_prev = last_grepped;
                    last_grepped = line_num;
                    rb.enqueue((ax, bx));
                }
            },
            Op::Truncate => {
                if opts.width > 0 {
                    let has_nl = bx > ax && buf[bx-1] == b'\n';
                    let content_end = if has_nl { bx - 1 } else { bx };
                    let content_len = content_end - ax;
                    let mut trunc = opts.width.min(content_len) + ax;
                    while trunc < content_end && (buf[trunc] & 0b11000000) == 0b10000000 {
                        trunc += 1;
                    }
                    if trunc < content_end {
                        buf[trunc] = b'\n';
                        pbx = trunc + 1;
                    }
                }
            },
        }
        i += 1;
    }
}

/// Build the VM opcodes for grep/print logic, shared between vm_tail and vm_fifo.
/// Takes the read-source op as a parameter so each caller can provide its own.
fn build_ops(read_op: Op, opts: &mut Opts) -> Vec<Op> {
    let mut jumps = JumpPositions::new();

    let print_op = if opts.theme == None { Op::PrintPlain } else { Op::PrintColor };
    let mut ops = vec![
        read_op,
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
    let mut vgreps_jump: Option<i64> = None;
    if opts.bctx == 0 && opts.actx == 0 && !matches!(maybe_vre, None) {
        let vgreps = jumps.new_placeholder();
        ops.push(Op::Match(maybe_vre.take().unwrap()));
        ops.push(Op::MatchJmp(vgreps));
        vgreps_jump = Some(vgreps);
    }
    let mut skip_print: Option<i64> = None;
    let mut actx_print: Option<i64> = None;
    if let Some(ref re) = opts.grep {
        let search = if opts.word {
            format!("\\b{}\\b", re)
        } else {
            re.to_string()
        };
        match RegexBuilder::new(search.as_str()).case_insensitive(opts.icase).build() {
            Ok(r) => {
                if opts.actx > 0 {
                    actx_print = Some(jumps.new_placeholder());
                    ops.push(Op::ActxJmp(actx_print.unwrap()));
                }
                ops.push(Op::Match(r.clone()));
                ops.push(Op::SetGrepPos);
                if opts.bctx > 0 {
                    ops.push(Op::RingbufAdd);
                }
                ops.push(Op::Invert);
                skip_print = Some(jumps.new_placeholder());
                ops.push(Op::MatchJmp(skip_print.unwrap()));
                if opts.bctx > 0 {
                    ops.push(Op::BctxPrep);
                    let ctx_start = ops.len();
                    let bjump = jumps.new_placeholder();
                    ops.push(Op::BctxNext(bjump));
                    let mut vjump: i64 = 0;
                    if let Some(ref vre) = maybe_vre {
                        vjump = jumps.new_placeholder();
                        ops.push(Op::Match(vre.clone()));
                        ops.push(Op::MatchJmp(vjump));
                    }
                    if opts.width > 0 { ops.push(Op::Truncate); }
                    ops.push(print_op.clone());
                    if vjump != 0 {
                        jumps.set_place(vjump, ops.len());
                    }
                    jumps.set_place(bjump, ops.len());
                    ops.push(Op::CtxJmp(ctx_start));
                } else if opts.width > 0 {
                    ops.push(Op::Truncate);
                    ops.push(print_op.clone());
                } else {
                    ops.push(print_op.clone());
                }
                ops.push(Op::Jmp(1));
                if opts.actx > 0 {
                    let actx_target = ops.len();
                    let mut vjump: i64 = 0;
                    if let Some(ref vre) = maybe_vre {
                        vjump = jumps.new_placeholder();
                        ops.push(Op::Match(vre.clone()));
                        ops.push(Op::MatchJmp(vjump));
                    }
                    ops.push(Op::ActxReset(r.clone()));
                    if opts.width > 0 { ops.push(Op::Truncate); }
                    if opts.theme == None {
                        ops.push(Op::PrintPlain);
                    } else {
                        ops.push(Op::PrintColor);
                    }
                    if vjump != 0 {
                        jumps.set_place(vjump, ops.len());
                    }
                    ops.push(Op::Jmp(1));
                    jumps.set_place(actx_print.unwrap(), actx_target);
                }
            },
            Err(e) => die!("Regex error for {}:{}", search, e),
        }
    } else if opts.width > 0 {
        ops.push(Op::Truncate);
    }
    ops.push(print_op.clone());
    if let Some(sp) = skip_print {
        jumps.set_place(sp, ops.len());
    }
    if let Some(vj) = vgreps_jump {
        jumps.set_place(vj, ops.len());
    }
    ops.push(Op::Jmp(1));

    jumps.update_ops(&mut ops);
    ops
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

    let read_op = if opts.tailfiles.is_empty() {
        Op::ReadStdin
    } else {
        Op::ReadNextFile(rx)
    };
    let ops = build_ops(read_op, opts);
    runvm(ops, files, opts);
}

/// Set up a named pipe (FIFO) at the given path and read from it using the VM.
/// - Removes any existing file/fifo at the path before creating the new fifo.
/// - Creates the fifo with mode 0o666.
/// - Registers a ctrlc handler to remove the fifo on exit.
/// - Reads from the fifo, re-opening on EOF (when writers disconnect).
pub fn vm_fifo(path: String, opts: &mut Opts) {
    // Remove existing file/fifo at the path if it exists
    if fs::metadata(&path).is_ok() {
        if let Err(e) = fs::remove_file(&path) {
            die!("Error removing existing file at {}:{}", path, e);
        }
    }
    // Create the fifo
    if let Err(e) = unix_named_pipe::create(&path, Some(0o666)) {
        die!("Error creating fifo at {}:{}", path, e);
    }
    println!("Created fifo: {}", path);

    // Register ctrlc handler to clean up the fifo on exit
    let cleanup_path = path.clone();
    ctrlc::set_handler(move || {
        if let Err(e) = fs::remove_file(cleanup_path.as_str()) {
            eprintln!("failed to remove fifo:{}", e);
        }
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    let ops = build_ops(Op::ReadFifo(path), opts);
    runvm(ops, HashMap::new(), opts);
}

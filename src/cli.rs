use {
    std::collections::{HashMap, BTreeSet},
    std::net::TcpStream,
    std::sync::{Arc, Mutex},
    termsize,
    getopts::Options,
    bat::{assets::HighlightingAssets, config::Config, controller::Controller, Input},
    std::mem,
    std::{fs::File},
    regex::Regex,
    std::io::{Read, Write},
};

#[macro_export]
macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}


pub const THEMES: [&'static str; 13] = [
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

pub struct Colorizer<'a> {
    conf: bat::config::Config<'a>,
    assets: bat::assets::HighlightingAssets,
    themes: [String; 13],
    idx: usize,
}
#[allow(dead_code)]
impl<'b> Colorizer<'b> {
    pub fn new<'a>(theme: String) -> Colorizer<'a> {
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
    pub fn string(&mut self, buf: &[u8], theme: Option<usize>) -> String {
        self.prep(theme);
        let controller = Controller::new(&self.conf, &self.assets);
        let mut out = String::new();
        let input = Input::from_bytes(buf).name("a.log");
        if let Err(e) = controller.run(vec![input.into()], Some(&mut out)) {
            eprintln!("Error highlighting synatx:{}", e);
        }
        out
    }
    pub fn print(&mut self, buf: &[u8], theme: Option<usize>) {
        self.prep(theme);
        let controller = Controller::new(&self.conf, &self.assets);
        let input = Input::from_bytes(buf).name("a.log");
        if let Err(e) = controller.run(vec![input.into()], None) {
            eprintln!("Error highlighting synatx:{}", e);
        }
    }
}

pub fn read_presets(swizzle: String, opts: &mut Opts) {
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
pub struct Opts {
    pub host: String,
    pub port: u16,
    pub fifo: bool,
    pub srvpath: Option<String>,
    pub grep: Option<String>,
    pub vgrep: Option<String>,
    pub word: bool,
    pub icase: bool,

    pub width: usize,
    pub termfit: bool,
    pub theme: Option<String>,
    pub onetheme: bool,
    pub fields: String,
    pub exclude: String,
    pub actx: usize,
    pub bctx: usize,
    pub client_mode: bool,
    pub server_mode: bool,
    pub tailfiles: Vec<String>,
    pub tcp_clients: Option<Arc<Mutex<HashMap<i64, TcpStream>>>>,
    pub socket: Option<TcpStream>,
}
impl Opts {
    pub fn merge(self: &mut Self, other: &mut Opts) {
        if self.host.is_empty() { self.host = mem::take(&mut other.host); };
        if self.port == 0 { self.port = other.port; };
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
        self.server_mode |= other.server_mode;
        self.client_mode |= other.client_mode;

        self.width = self.width.max(other.width);
        self.actx = self.actx.max(other.actx);
        self.bctx = self.bctx.max(other.bctx);
        self.exclude = format!("{}{}{}",self.exclude,
                              if !self.exclude.is_empty() && !other.exclude.is_empty() {","} else {""},
                              other.exclude);
        self.tailfiles = self.tailfiles.iter().chain(other.tailfiles.iter())
            .collect::<BTreeSet<&String>>().iter().map(|s|s.to_string()).collect();
    }

    pub fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::newargs(&args, true)
    }
    pub fn newargs(args: &Vec<String>, recurse: bool) -> Self {
        let mut opts = Options::new();
        opts.optopt("I", "ip", "ip of server", "HOST");
        opts.optopt("d", "port", "tcp port (default: 19888)", "PORT");
        opts.optflag("k","client", "read from localhost in client mode");
        opts.optopt("f", "fifo", "path of fifo to create and read from", "PATH");
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
        opts.optflag("h", "help", "print this help menu");
        let mut matches = match opts.parse(args) {
            Ok(m) => { m },
            Err(e) => die!("bad args:{}", e),
        };
        if matches.opt_present("h") {
            die!("{}\n{}",opts.usage(&args[0]), "Use files as positional parameters to function the same as tail -f.\nUse -f for fifo mode, -s for server mode, -k for client mode.\nOtherwise read from rog server and print.");
        }
        let theme = match (matches.opt_present("c"), matches.opt_present("m")) {
            (true,true) => die!("Cannot use 'c' with 'm'"),
            (true,false) => None,
            (false,true) => matches.opt_str("m"),
            (false,false) =>  Some("Visual Studio Dark+".to_string()),
        };
        let c = matches.opt_str("C").unwrap_or("0".to_string());
        let width = if matches.opt_present("u") {
            termsize::get().map(|t| t.cols as usize).unwrap_or(80)
        } else {
            matches.opt_str("o").unwrap_or("0".to_string()).parse().unwrap()
        };
        let mut opts = Opts {
            host: matches.opt_str("I").unwrap_or(if matches.opt_present("k") {"127.0.0.1".to_string()} else {Default::default()}),
            port: matches.opt_str("d").unwrap_or("19888".to_string()).parse().expect("d must be a port number"),
            fifo: matches.opt_present("f"),
            srvpath: matches.opt_str("f"),
            grep: matches.opt_str("w").map_or(matches.opt_str("g"),|g|Some(g)),
            vgrep: matches.opt_str("v"),
            word: matches.opt_present("w"),
            icase: matches.opt_present("i"),
            termfit: matches.opt_present("u"),
            width,
            theme,
            fields: matches.opt_str("r").unwrap_or(Default::default()),
            actx: matches.opt_str("A").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            bctx: matches.opt_str("B").unwrap_or(c.to_string()).parse().expect("A,B,C matches must be positive integers"),
            client_mode: matches.opt_present("k"),
            server_mode: matches.opt_present("s"),
            tailfiles: mem::take(&mut matches.free),
            exclude: matches.opt_str("x").unwrap_or(Default::default()),
            onetheme: matches.opt_present("n"),
            tcp_clients: None,
            socket: None,
        };
        if recurse && !matches.opt_present("P") {
            read_presets(matches.opt_str("p").unwrap_or("".to_string()), &mut opts);
        }
        opts
    }
}

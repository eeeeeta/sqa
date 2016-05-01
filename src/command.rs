use parser::{Tokens, EtokenFSM, ParserErr, SpaceRet};
use state::Context;
use rsndfile::SndFile;
use streamv2::{FileStream, LiveParameters, db_lin};
use mixer::FRAMES_PER_CALLBACK;
use std::string::ToString;
pub trait Command {
    fn add(&mut self, tok: Tokens, ctx: &Context) -> Result<(), ParserErr>;
    fn back(&mut self) -> Option<Tokens>;
    fn is_complete(&self, ctx: &Context) -> Result<(), String>;
    fn line(&self) -> (Tokens, &Vec<Tokens>);
    fn execute(&mut self, ctx: &mut Context);
}
pub struct VolCommand {
    cli: Vec<Tokens>,
    ident: Option<String>,
    chan: isize,
    target: Option<i16>,
    fade: Option<f32>
}
impl VolCommand {
    fn new() -> Self {
        VolCommand {
            cli: Vec::new(),
            ident: None,
            chan: -1,
            target: None,
            fade: None
        }
    }
}
impl Command for VolCommand {
    fn execute(&mut self, ctx: &mut Context) {
        let (ident, chan, target) = (self.ident.take().unwrap(), self.chan, db_lin(self.target.take().unwrap() as f32));
        let fsx = ctx.idents.get_mut(&ident).unwrap();
        for (i, ch) in fsx.iter_mut().enumerate() {
            if chan == i as isize || chan == -1 {
                if self.fade.is_some() {
                    let lp = ch.lp();
                    let end = lp.pos + (self.fade.unwrap() * 44_100 as f32) as usize;
                    ch.set_fader(Box::new(move |pos, vol, _| {
                        let fade_left = *vol - target;
                        if fade_left == 0.0 { return false };
                        let units_left = (end - pos) / FRAMES_PER_CALLBACK;
                        if units_left == 0 {
                            *vol = target;
                            true
                        }
                        else {
                            *vol = *vol - (fade_left / units_left as f32);
                            true
                        }
                    }));
                }
                else {
                    ch.set_vol(target);
                }
            }
        }
    }
    fn is_complete(&self, ctx: &Context) -> Result<(), String> {
        if self.ident.is_none() { Err(format!("No identifier to fade.")) }
        else if self.target.is_none() { Err(format!("No volume to fade to.")) }
        else {
            if ctx.idents.get(self.ident.as_ref().unwrap()).is_none() {
                Err(format!("The identifier ${} does not exist.", self.ident.as_ref().unwrap()))
            }
            else {
                if self.chan != -1 && ctx.idents.get(self.ident.as_ref().unwrap()).unwrap().len() <= self.chan as usize {
                    Err(format!("The identifier ${} does not have a channel numbered {}.", self.ident.as_ref().unwrap(), self.chan))
                }
                else {
                    Ok(())
                }
            }
        }
    }
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Vol, &self.cli)
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Vol; // to appease borrowck
            let mut biter = self.cli.iter();
            let last = biter.next_back().unwrap_or(&ld);
            match last {
                &Tokens::Vol => {},
                &Tokens::Fade => {},
                &Tokens::At => {},
                &Tokens::Chan => {},
                &Tokens::Identifier(_) => { self.ident = None },
                &Tokens::Num(_) | &Tokens::NegNum(_) => {
                    match biter.next_back().unwrap() {
                        &Tokens::At => { self.target = None },
                        &Tokens::Chan => { self.chan = -1 },
                        &Tokens::Fade => { self.fade = None },
                        _ => unreachable!()
                    }
                },
                &Tokens::Float(_) => { self.fade = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()

    }
    fn add(&mut self, tok: Tokens, ctx: &Context) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Vol).clone();
        match last {
            Tokens::Vol => {
                if let Tokens::Identifier(id) = tok {
                    if ctx.idents.get(&id).is_none() {
                        Err(ParserErr::ArgumentError(format!("The identifier ${} does not exist.", id)))
                    }
                    else {
                        self.ident = Some(id.clone());
                        self.cli.push(Tokens::Identifier(id));
                        Ok(())
                    }
                }
                else {
                    Err(ParserErr::Expected("[identifier]"))
                }
            },
            Tokens::Identifier(_) => {
                match tok {
                    Tokens::At => {
                        self.cli.push(Tokens::At);
                        Ok(())
                    },
                    Tokens::Chan => {
                        self.cli.push(Tokens::Chan);
                        Ok(())
                    },
                    _ => Err(ParserErr::Expected("@ or Chan"))
                }
            },
            Tokens::Chan => {
                if let Tokens::Num(n) = tok {
                    self.chan = n as isize;
                    self.cli.push(Tokens::Num(n));
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("Num"))
                }
            },
            Tokens::At => {
                match tok {
                    Tokens::Num(n) => {
                        self.target = Some(n as i16);
                        self.cli.push(Tokens::Num(n));
                        Ok(())
                    },
                    Tokens::NegNum(n) => {
                        self.target = Some(n);
                        self.cli.push(Tokens::NegNum(n));
                        Ok(())
                    },
                    _ => Err(ParserErr::Expected("Num or NegNum"))
                }
            },
            Tokens::Num(_) | Tokens::NegNum(_) => {
                match {
                    let mut i = self.cli.iter();
                    i.next_back();
                    i.next_back().unwrap().clone()
                } {
                    Tokens::At => {
                        if let Tokens::Fade = tok {
                            self.cli.push(Tokens::Fade);
                            Ok(())
                        }
                        else {
                            Err(ParserErr::Expected("Fade or [finish]"))
                        }
                    },
                    Tokens::Chan => {
                        if let Tokens::At = tok {
                            self.cli.push(Tokens::At);
                            Ok(())
                        }
                        else {
                            Err(ParserErr::Expected("@"))
                        }
                    },
                    Tokens::Fade => Err(ParserErr::Expected("[finish]")),
                    _ => unreachable!()
                }
            },
            Tokens::Fade => {
                match tok {
                    Tokens::Num(n) => {
                        self.fade = Some(n as f32);
                        self.cli.push(Tokens::Num(n));
                        Ok(())
                    },
                    Tokens::Float(f) => {
                        self.fade = Some(f);
                        self.cli.push(Tokens::Float(f));
                        Ok(())
                    }
                    _ => Err(ParserErr::Expected("Num or Float"))
                }
            },
            Tokens::Float(_) => Err(ParserErr::Expected("[finish]")),
            _ => unreachable!()
        }
    }
}
pub struct LoadCommand {
    cli: Vec<Tokens>,
    file: Option<SndFile>,
    ident: Option<String>
}
impl LoadCommand {
    fn new() -> Self {
        LoadCommand {
            cli: Vec::new(),
            file: None,
            ident: None
        }
    }
}
impl Command for LoadCommand {
    fn execute(&mut self, ctx: &mut Context) {
        let file = self.file.take();
        let mut ident = self.ident.take();
        let mut cvec = vec![];
        for stream in FileStream::new(file.unwrap()) {
            cvec.push(stream.get_x());
            ctx.mstr.add_source(Box::new(stream));
        }
        for (i, ch) in cvec.iter().enumerate() {
            if i > 15 { continue; }
            ctx.mstr.wire(ch.uuid(), ctx.qchans[i]).unwrap();
        }
        if ident.is_none() {
            let mut path = None;
            for tok in self.cli.iter() {
                if let &Tokens::Path(ref p) = tok {
                    path = Some(p.clone());
                    break;
                }
            }
            let filen = path.expect("invariant violated: file, but no Path in cli");
            ident = Some(::std::path::Path::new(&filen).file_stem().map(|x| x.to_str()).unwrap().unwrap().to_owned());
        }
        ctx.idents.insert(ident.unwrap(), cvec);
    }
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Load, &self.cli)
    }
    fn is_complete(&self, ctx: &Context) -> Result<(), String> {
        if self.file.is_none() { Err(format!("No file.")) }
        else {
            if self.ident.is_none() {
                let mut path = None;
                for tok in self.cli.iter() {
                    if let &Tokens::Path(ref p) = tok {
                        path = Some(p.clone());
                        break;
                    }
                }
                let filen = path.expect("invariant violated: file, but no Path in cli");
                let ident = ::std::path::Path::new(&filen).file_stem().map(|x| x.to_str());
                if let Some(Some(id)) = ident {
                    if ctx.idents.get(id).is_some() {
                        Err(format!("The identifier ${} is already in use. Please manually specify an identifier name for this file.", id))
                    }
                    else {
                        Ok(())
                    }

                }
                else {
                    Err(format!("Please manually specify an identifier name for this file."))
                }
            }
            else {
                let self_ident: &str = self.ident.as_ref().unwrap();
                if ctx.idents.get(self_ident).is_some() {
                    Err(format!("The identifier ${} is already in use. Please specify another.", self_ident))
                }
                else {
                    Ok(())
                }
            }
        }
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Load; // to appease borrowck
            let last = self.cli.iter().next_back().unwrap_or(&ld);
            match last {
                &Tokens::Load => {},
                &Tokens::Path(_) => { self.file = None },
                &Tokens::As => {},
                &Tokens::Identifier(_) => { self.ident = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()
    }
    fn add(&mut self, tok: Tokens, ctx: &Context) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Load).clone();
        match last {
            Tokens::Load => {
                if let Tokens::Path(path) = tok {
                    let file = SndFile::open(&path);
                    if let Err(e) = file {
                        Err(ParserErr::ArgumentError(format!("Failed to open file: {}", e.expl)))
                    }
                    else if file.as_ref().unwrap().info.samplerate != 44_100 {
                        Err(ParserErr::ArgumentError(format!("Sample rate mismatch.")))
                    }
                    else {
                        self.file = Some(file.unwrap());
                        self.cli.push(Tokens::Path(path));
                        Ok(())
                    }
                }
                else {
                    Err(ParserErr::Expected("Path"))
                }
            },
            Tokens::Path(_) => {
                if let Tokens::As = tok {
                    self.cli.push(Tokens::As);
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("As or [finish]"))
                }
            },
            Tokens::As => {
                if let Tokens::Identifier(ident) = tok {
                    if ctx.idents.get(&ident).is_some() {
                        Err(ParserErr::ArgumentError(format!("The identifier ${} is already in use", ident)))
                    }
                    else {
                        self.ident = Some(ident.clone());
                        self.cli.push(Tokens::Identifier(ident));
                        Ok(())
                    }
                }
                else {
                    Err(ParserErr::Expected("[identifier]"))
                }
            },
            Tokens::Identifier(_) => {
                Err(ParserErr::Expected("[finish]"))
            },
            _ => unreachable!()
        }
    }
}

pub enum CmdParserFSM {
    Idle,
    Parsing(Box<Command>),
    ParsingEtokFor(Box<Command>, EtokenFSM)
}
impl CmdParserFSM {
    pub fn new() -> Self {
        CmdParserFSM::Idle
    }
    fn char_tok(c: char) -> Option<Tokens> {
        match c {
            'l' => Some(Tokens::Load),
            'v' => Some(Tokens::Vol),
            'p' => Some(Tokens::Pos),
            's' => Some(Tokens::Start),
            'o' => Some(Tokens::Stop),
            'a' => Some(Tokens::As),
            '#' => Some(Tokens::All),
            'c' => Some(Tokens::Chan),
            '@' => Some(Tokens::At),
            'f' => Some(Tokens::Fade),
            _ => None
        }
    }
    fn cmdline_from_cmd(cmd: &Box<Command>) -> String {
        let (orig, line) = cmd.line();
        let mut ret = orig.to_string();
        for tok in line.iter() {
            ret.push_str(" ");
            ret = ret + &tok.to_string();
        }
        ret
    }
    pub fn cmdline(&self) -> String {
        match self {
            &CmdParserFSM::Idle => format!(""),
            &CmdParserFSM::Parsing(ref cmd) => CmdParserFSM::cmdline_from_cmd(cmd),
            &CmdParserFSM::ParsingEtokFor(ref cmd, ref etok) => CmdParserFSM::cmdline_from_cmd(cmd) + " " + &etok.to_string()
        }
    }
    pub fn debug_remove_me(&self, ctx: &Context) -> String {
        match self {
            &CmdParserFSM::Idle => format!("idle"),
            &CmdParserFSM::Parsing(ref cmd) => format!("parsing command (complete: {:?})", cmd.is_complete(ctx)),
            &CmdParserFSM::ParsingEtokFor(ref cmd, ref etok) => format!("parsing etok {:?} for command (complete: {:?})", etok, cmd.is_complete(ctx))
        }
    }
    pub fn back(self) -> Self {
        match self {
            CmdParserFSM::Idle => CmdParserFSM::Idle,
            CmdParserFSM::Parsing(mut cmd) => {
                let popped = cmd.back();
                if let Some(Some(pos_etok)) = popped.map(|x| EtokenFSM::from_token(x)) {
                    if let Some(new_etok) = pos_etok.back() {
                        CmdParserFSM::ParsingEtokFor(cmd, new_etok)
                    }
                    else {
                        CmdParserFSM::Parsing(cmd)
                    }
                }
                else if cmd.line().1.len() == 0 {
                    CmdParserFSM::Idle
                }
                else {
                    CmdParserFSM::Parsing(cmd)
                }
            },
            CmdParserFSM::ParsingEtokFor(cmd, etok) => {
                if let Some(new_etok) = etok.back() {
                    CmdParserFSM::ParsingEtokFor(cmd, new_etok)
                }
                else {
                    CmdParserFSM::Parsing(cmd)
                }
            }
        }
    }
    pub fn enter(self, ctx: &mut Context) -> Result<Self, (Self, String)> {
        match self {
            CmdParserFSM::Idle => Ok(CmdParserFSM::Idle),
            CmdParserFSM::Parsing(mut cmd) => {
                if let Err(e) = cmd.is_complete(ctx) {
                    Err((CmdParserFSM::Parsing(cmd), e))
                }
                else {
                    cmd.execute(ctx);
                    Ok(CmdParserFSM::Idle)
                }
            },
            CmdParserFSM::ParsingEtokFor(mut cmd, etok) => {
                // TODO: needs more DRY
                let oe = etok.clone();
                match etok.finish(false) {
                    SpaceRet::Parsed(tok) => {
                        match cmd.add(tok, ctx) {
                            Ok(_) => {
                                if let Err(e) = cmd.is_complete(ctx) {
                                    Err((CmdParserFSM::Parsing(cmd), e))
                                }
                                else {
                                    cmd.execute(ctx);
                                    Ok(CmdParserFSM::Idle)
                                }
                            }
                            Err(e) => Err((CmdParserFSM::ParsingEtokFor(cmd, oe), Into::into(e)))
                        }
                    },
                    SpaceRet::Continue(fsm) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::IncompleteEtoken)))
                    },
                    SpaceRet::Incomplete(fsm) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::IncompleteEtoken)))
                    },
                    SpaceRet::IntErr(fsm, _) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::NumError)))
                    },
                    SpaceRet::FloatErr(fsm, _) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::NumError)))
                    }
                }
            }
        }
    }
    pub fn addc(self, c: char, ctx: &Context) -> Result<Self, (Self, ParserErr)> {
        match self {
            CmdParserFSM::Idle => {
                match CmdParserFSM::char_tok(c) {
                    Some(Tokens::Load) => {
                        Ok(CmdParserFSM::Parsing(Box::new(LoadCommand::new())))
                    },
                    Some(Tokens::Vol) => {
                        Ok(CmdParserFSM::Parsing(Box::new(VolCommand::new())))
                    }
                    _ => Err((CmdParserFSM::Idle, ParserErr::Expected("Load or Vol")))
                }
            },
            CmdParserFSM::Parsing(mut cmd) => {
                if let Some(tok) = CmdParserFSM::char_tok(c) {
                    match cmd.add(tok, ctx) {
                        Ok(_) => Ok(CmdParserFSM::Parsing(cmd)),
                        Err(e) => Err((CmdParserFSM::Parsing(cmd), e))
                    }
                }
                else if let Ok(etok) = EtokenFSM::new().addc(c) {
                    Ok(CmdParserFSM::ParsingEtokFor(cmd, etok))
                }
                else {
                    Err((CmdParserFSM::Parsing(cmd), ParserErr::InvalidToken))
                }
            },
            CmdParserFSM::ParsingEtokFor(mut cmd, etok) => {
                if c == ' ' {
                    let oe = etok.clone();
                    match etok.finish(true) {
                        SpaceRet::Parsed(tok) => {
                            match cmd.add(tok, ctx) {
                                Ok(_) => Ok(CmdParserFSM::Parsing(cmd)),
                                Err(e) => Err((CmdParserFSM::ParsingEtokFor(cmd, oe), e))
                            }
                        },
                        SpaceRet::Continue(fsm) => {
                            Ok(CmdParserFSM::ParsingEtokFor(cmd, fsm))
                        },
                        SpaceRet::Incomplete(fsm) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::IncompleteEtoken))
                        },
                        SpaceRet::IntErr(fsm, _) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::NumError))
                        },
                        SpaceRet::FloatErr(fsm, _) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::NumError))
                        }
                    }
                }
                else {
                    match etok.addc(c) {
                        Ok(fsm) => Ok(CmdParserFSM::ParsingEtokFor(cmd, fsm)),
                        Err((opt, err)) => Err((CmdParserFSM::ParsingEtokFor(cmd, opt.unwrap()), err))
                    }
                }
            }
        }
    }
}

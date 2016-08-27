//! Program state management.

use mixer::Magister;
use command::{Command, CommandUpdate, HunkState, HunkTypes};
use std::collections::{HashMap, BTreeMap};
use uuid::Uuid;
use std::ops::Deref;
use gtk::Adjustment;
use chrono::{UTC, DateTime};
use std::time::Duration;
use ui::UIMode;
use mio::EventLoop;
pub use cues::{ChainType, Chain}; // FIXME: hacky

#[derive(Clone)]
/// An object for cross-thread notification.
pub struct ThreadNotifier {
    adj: Adjustment
}
impl ThreadNotifier {
    pub fn new() -> Self {
        ThreadNotifier {
            adj: Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        }
    }
    pub fn notify(&self) {
        let selfish = self.clone();
        ::glib::timeout_add(0, move || {
            selfish.adj.changed();
            ::glib::Continue(false)
        });
    }
    pub fn register_handler<F: Fn() + 'static>(&self, func: F) {
        self.adj.connect_changed(move |_| {
            func()
        });
    }
}
/// I'm pretty sure this is safe. Maybe.
///
/// Seriously: glib::timeout_add() runs its handler _in the main thread_,
/// so we should be fine.
unsafe impl Send for ThreadNotifier {}

pub enum Message {
    /// C -> S: Create a new command with given UUID from spawner.
    NewCmd(Uuid, ::commands::CommandSpawner),
    /// C -> S: Set hunk index of command with given UUID to value.
    SetHunk(Uuid, usize, HunkTypes),
    /// C -> S: Execute command.
    Execute(Uuid),
    /// C -> S: Delete command.
    Delete(Uuid),
    /// C -> S: Attach command to chain.
    Attach(Uuid, ChainType),
    /// C -> S: Start running chain.
    Go(ChainType),
    /// C -> S: Standby on given chain - this removes all other standby states
    Standby(Option<ChainType>),
    /// C -> S: Set fallthrough of command to given state.
    SetFallthru(Uuid, bool),
    /// S -> C: Update your descriptor of command with given UUID.
    CmdDesc(Uuid, CommandDescriptor),
    /// S -> C: Update given chain.
    ChainDesc(ChainType, Chain),
    /// S -> C: Update fallthrough field for given chain.
    ChainFallthru(ChainType, HashMap<Uuid, bool>),
    /// S -> C: Delete command.
    Deleted(Uuid),
    /// S -> C: Delete given chain.
    ChainDeleted(ChainType),
    /// S -> C: Update list of named identifiers.
    Identifiers(HashMap<String, Uuid>),
    /// Other Backend Threads -> S: Apply closure to command with given UUID & propagate changes.
    Update(Uuid, CommandUpdate),
    /// UI objects -> UI: Change UI mode to the following
    UIChangeMode(UIMode),
    /// UI objects -> UI: Start editing command on the command line.
    UIBeginEditing(Uuid)
}
#[derive(Clone, Debug)]
pub enum CommandState {
    /// The command contains errors, and can not run.
    ///
    /// A command may not be in this state if it is currently running - if errors are introduced
    /// while the command is running, the command should transition to this state after completion.
    Incomplete,
    /// The command is ready to execute.
    Ready,
    /// The command is ready to execute (and has loaded some parts of itself into memory, for speedier
    /// execution)
    Loaded,
    /// The command is running.
    Running(Duration),
    /// The command has encountered a fatal error, from which it cannot recover.
    Errored(String),
}
#[derive(Clone, Debug)]
pub struct CommandDescriptor {
    pub desc: String,
    pub name: &'static str,
    pub hunks: Vec<HunkState>,
    pub state: CommandState,
    pub ctime: DateTime<UTC>,
    pub uuid: Uuid
}
impl CommandDescriptor {
    pub fn new(desc: String, name: &'static str, state: CommandState, hunks: Vec<HunkState>, uu: Uuid) -> Self {
        CommandDescriptor {
            desc: desc,
            name: name,
            state: state,
            hunks: hunks,
            ctime: UTC::now(),
            uuid: uu
        }
    }
}

/// Global context
pub struct Context<'a> {
    pub pa: &'a mut ::portaudio::PortAudio,
    pub commands: BTreeMap<Uuid, Box<Command>>,
    pub identifiers: HashMap<String, Uuid>,
    pub chains: BTreeMap<ChainType, Chain>,
    pub mstr: Magister,
    pub tx: ::std::sync::mpsc::Sender<Message>,
    pub tn: ThreadNotifier,
}
impl<'a> Context<'a> {
    pub fn new(pa: &'a mut ::portaudio::PortAudio, tx: ::std::sync::mpsc::Sender<Message>, tn: ThreadNotifier) -> Self {
        let mut ctx = Context {
            pa: pa,
            commands: BTreeMap::new(),
            identifiers: HashMap::new(),
            chains: BTreeMap::new(),
            mstr: Magister::new(),
            tx: tx,
            tn: tn,
        };
        ctx.chains.insert(ChainType::Unattached, Chain::new());
        ctx
    }
    fn loadunload_cmd(&mut self, uu: Uuid, evl: &mut EventLoop<Context>, load: bool) {
        let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
        if load {
            cmd.load(self, evl, uu);
        }
        else {
            cmd.unload(self, evl, uu);
        }
        self.commands.insert(uu, cmd);
    }
    pub fn load_cmd(&mut self, uu: Uuid, evl: &mut EventLoop<Context>) {
        self.loadunload_cmd(uu, evl, true);
        self.update_cmd(uu);
    }
    pub fn unload_cmd(&mut self, uu: Uuid, evl: &mut EventLoop<Context>) {
        self.loadunload_cmd(uu, evl, false);
        self.update_cmd(uu);
    }
    pub fn exec_cmd(&mut self, uu: Uuid, evl: &mut EventLoop<Context>) -> bool {
        let mut cmd = self.commands.get_mut(&uu).unwrap().box_clone();
        let finished = cmd.execute(self, evl, uu).unwrap();
        self.commands.insert(uu, cmd);
        self.update_cmd(uu);
        finished
    }
    pub fn execution_completed(&mut self, uu: Uuid, evl: &mut EventLoop<Context>) {
        let mut blocked = None;
        for (k, chn) in self.chains.iter_mut() {
            if chn.is_blocked_on(uu) {
                blocked = Some((k.clone(), chn.clone()));
                break;
            }
        }
        if let Some((k, mut blk)) = blocked {
            blk.on_exec_completed(uu, self, evl);
            self.chains.insert(k, blk);
        }
    }
    pub fn prettify_uuid(&self, uu: &Uuid) -> String {
        for (ct, chn) in self.chains.iter() {
            for (i, v) in chn.commands.iter().enumerate() {
                if v == uu { return format!("{}{}", ct, i) }
            }
        }
        format!("{}", uu)
    }
    pub fn label(&mut self, lbl: Option<String>, uu: Uuid) {
        if let Some(lbl) = lbl {
            self.identifiers.insert(lbl, uu);
        }
        else {
            let mut todel = vec![];
            for (k, v) in self.identifiers.iter() {
                if *v == uu { todel.push(k.clone()); }
            }
            for k in todel {
                self.identifiers.remove(&k);
            }
        }
        let idents = { self.identifiers.clone() };
        self.send(Message::Identifiers(idents));
    }
    pub fn update_chn(&mut self, ct: ChainType) {
        let chn = self.chains.get(&ct).unwrap().clone();
        self.send(Message::ChainDesc(ct, chn));
    }
    pub fn attach_chn(&mut self, ct: Option<ChainType>, cu: Uuid) {
        let mut modified = None;
        for (ref mut ct, ref mut chn) in &mut self.chains {
            if chn.remove(cu) {
                modified = Some(ct.clone());
            }
        }
        if let Some(ct) = modified {
            self.update_chn(ct);
        }
        if let Some(ct) = ct {
            self.chains.entry(ct.clone()).or_insert(Chain::new()).push(cu);
            self.update_chn(ct);
        }
    }
    pub fn desc_cmd(&self, cu: Uuid) -> CommandDescriptor {
        let cmd = self.commands.get(&cu).unwrap();
        let errs: u32 = cmd.get_hunks().into_iter().map(|c| {
            if let Some(..) = c.get_val(cmd.deref(), &self).err { 1 } else { 0 }
        }).sum();
        let state = if let Some(st) = cmd.run_state() {
            st
        }
        else if errs > 0 {
            CommandState::Incomplete
        }
        else {
            CommandState::Ready
        };
        CommandDescriptor::new(
            cmd.desc(self),
            cmd.name(),
            state,
            cmd.get_hunks().into_iter().map(|c| c.get_val(cmd.deref(), &self)).collect(),
            cu)
    }
    pub fn update_cmd(&mut self, cu: Uuid) {
        let cd = self.desc_cmd(cu);
        self.send(Message::CmdDesc(cu, cd));
    }
    pub fn send(&mut self, msg: Message) {
        self.tx.send(msg).unwrap();
        self.tn.notify();
    }
}

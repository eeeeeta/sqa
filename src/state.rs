//! Program state management.

use streamv2::{FileStream, FileStreamX};
use mixer::{QChannel, Magister, Sink, Source, DeviceSink};
use command::{Command, CommandUpdate, HunkState, HunkTypes};
use std::collections::BTreeMap;
use uuid::Uuid;
use std::any::Any;
use std::fmt;
use std::ops::Deref;
use gtk::Adjustment;
use chrono::{UTC, Duration, DateTime};
use std::rc::Rc;
use std::cell::RefCell;
use threadpool::ThreadPool;
use cues::QRunnerX;
use ui::UIMode;

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

#[derive(Clone)]
/// The type of an object stored in the database.
pub enum ObjectType {
    /// A channel of a FileStream created from a given file path.
    FileStream(String, usize),
    /// A numbered QChannel.
    QChannel(usize),
    /// A numbered device output channel.
    DeviceOut(usize)
}
impl ObjectType {
    fn is_same_type(&self, rhs: &Self) -> bool {
        match rhs {
            &ObjectType::FileStream(_, _) => {
                if let &ObjectType::FileStream(_, _) = self {
                    true
                }
                else {
                    false
                }
            },
            &ObjectType::QChannel(_) => {
                if let &ObjectType::QChannel(_) = self {
                    true
                }
                else {
                    false
                }
            },
            &ObjectType::DeviceOut(_) => {
                if let &ObjectType::DeviceOut(_) = self {
                    true
                }
                else {
                    false
                }
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum ChainType {
    Unattached,
    Q(String)
}
impl fmt::Display for ChainType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &ChainType::Unattached => write!(f, "X"),
            &ChainType::Q(ref st) => write!(f, "Q{}.", st),
        }
    }
}
#[derive(Clone)]
pub struct Chain {
    pub commands: Vec<Uuid>
}
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
    /// S -> C: Update your descriptor of command with given UUID.
    CmdDesc(Uuid, CommandDescriptor),
    /// S -> C: Update given chain.
    ChainDesc(ChainType, Chain),
    /// S -> C: Delete command.
    Deleted(Uuid),
    /// S -> C: Delete given chain.
    ChainDeleted(ChainType),
    /// S -> C: Update list of named identifiers.
    Identifiers(BTreeMap<String, Uuid>),
    /// Other Backend Threads -> S: Apply closure to command with given UUID & propagate changes.
    Update(Uuid, CommandUpdate),
    /// Other Backend Threads -> S: Execution of given command completed - notify relevant QRunner
    ExecutionCompleted(Uuid),
    /// QRunner -> S: I (tuple.0) am blocked on command (tuple.1).
    QRunnerBlocked(Uuid, Uuid),
    /// QRunner -> S: My thread is just about to exit, please remove my counterpart
    QRunnerCompleted(Uuid),
    /// UI objects -> UI: Change UI mode to the following
    UIChangeMode(UIMode),
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
    pub identifiers: BTreeMap<String, Uuid>,
    pub chains: BTreeMap<ChainType, Chain>,
    pub runners: Vec<QRunnerX>,
    pub mstr: Magister,
    pub tx: ::std::sync::mpsc::Sender<Message>,
    pub tn: ThreadNotifier,
    pub pool: ThreadPool
}
impl<'a> Context<'a> {
    pub fn new(pa: &'a mut ::portaudio::PortAudio, tx: ::std::sync::mpsc::Sender<Message>, tn: ThreadNotifier) -> Self {
        let mut ctx = Context {
            pa: pa,
            commands: BTreeMap::new(),
            identifiers: BTreeMap::new(),
            chains: BTreeMap::new(),
            mstr: Magister::new(),
            runners: Vec::new(),
            tx: tx,
            tn: tn,
            pool: ThreadPool::new(4)
        };
        ctx.chains.insert(ChainType::Unattached, Chain { commands: vec![] });
        ctx
    }
    pub fn execution_completed(&mut self, uu: Uuid) {
        println!("execution completed for {}", uu);
        for qrx in self.runners.iter_mut() {
            qrx.trigger(uu);
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
        let mut modified = vec![];
        for (ref mut ct, ref mut chn) in &mut self.chains {
            chn.commands.retain(|uu| {
                if *uu == cu {
                    modified.push(ct.clone());
                    false
                }
                else { true }
            });
        }
        for ct in modified {
            self.update_chn(ct);
        }
        if let Some(ct) = ct {
            self.chains.entry(ct.clone()).or_insert(Chain { commands: vec![] }).commands.push(cu);
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

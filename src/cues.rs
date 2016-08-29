//! Cues (lists of commands).

use state::{Context, Message};
use backend::BackendSender;
use std::collections::HashMap;
use mio::EventLoop;
use uuid::Uuid;
use std::fmt;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum ChainType {
    Unattached,
    Q(usize)
}
impl fmt::Display for ChainType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &ChainType::Unattached => write!(f, "X"),
            &ChainType::Q(num) => write!(f, "Q{}", num),
        }
    }
}
#[derive(Clone)]
pub struct Chain {
    fsm: QFSM,
    pub commands: Vec<Uuid>,
    pub fallthru: HashMap<Uuid, bool>
}

/// Fixed state machine for a cue runner.
#[derive(Clone)]
pub enum QFSM {
    /// Nothing happening
    Idle,
    /// Next in line for execution (all cues loaded)
    Standby,
    /// Executing, blocked on tuple.0
    Blocked(Uuid, usize)
}

impl Chain {
    pub fn new() -> Self {
        Chain {
            fsm: QFSM::Idle,
            commands: Vec::new(),
            fallthru: HashMap::new()
        }
    }
    pub fn push(&mut self, cmd: Uuid) {
        self.commands.push(cmd);
        self.fallthru.insert(cmd, false);
    }
    pub fn insert(&mut self, cmd: Uuid, mut idx: usize) {
        if idx > self.commands.len() {
            idx = self.commands.len();
        }
        self.commands.insert(idx, cmd);
        self.fallthru.insert(cmd, false);
    }
    pub fn remove(&mut self, cmd: Uuid) -> bool {
        let mut idx = None;
        for (i, uu) in self.commands.iter().enumerate() {
            if cmd == *uu {
                idx = Some(i);
                break;
            }
        }
        if let Some(i) = idx {
            self.commands.remove(i);
            self.fallthru.remove(&cmd);
            true
        }
        else { false }
    }
    pub fn set_fallthru(&mut self, cmd: Uuid, ft: bool) -> bool {
        if self.fallthru.get(&cmd).is_some() {
            self.fallthru.insert(cmd, ft);
            true
        }
        else {
            false
        }
    }
    pub fn is_blocked_on(&mut self, uu: Uuid) -> bool {
        if let QFSM::Blocked(u2, _) = self.fsm {
            u2 == uu
        }
        else {
            false
        }
    }
    pub fn on_exec_completed(&mut self, completed: Uuid, ctx: &mut Context, evl: &mut EventLoop<Context>) -> bool {
        if let QFSM::Blocked(uu, mut idx) = self.fsm.clone() {
            if uu == completed {
                idx += 1;
                self.exec(idx, ctx, evl);
                return true;
            }
        }
        false
    }
    fn exec(&mut self, idx: usize, ctx: &mut Context, evl: &mut EventLoop<Context>) {
        if let Some(now) = self.commands.get(idx).map(|x| *x) {
            self.fsm = QFSM::Blocked(now, idx);
            if ctx.exec_cmd(now, evl) || self.fallthru.get(&now).map(|x| *x).unwrap_or(false) {
                self.on_exec_completed(now, ctx, evl);
            }
        }
        else {
            self.fsm = QFSM::Idle;
        }
    }
    pub fn go(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>) {
        self.exec(0, ctx, evl);
    }
    pub fn standby(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>) {
        if let QFSM::Idle = self.fsm {
            for cmd in self.commands.iter() {
                ctx.load_cmd(*cmd, evl);
            }
            self.fsm = QFSM::Standby;
        }
    }
    pub fn unstandby(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>) {
        if let QFSM::Standby = self.fsm {
            for cmd in self.commands.iter() {
                ctx.unload_cmd(*cmd, evl);
            }
            self.fsm = QFSM::Idle;
        }
    }
}

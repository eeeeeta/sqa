macro_rules! clone {
    ($($n:ident),+; || $body:block) => (
        {
            $( let $n = $n.clone(); )+
                move || { $body }
        }
    );
    ($($n:ident),+; |$($p:ident),+| $body:block) => (
        {
            $( let $n = $n.clone(); )+
                move |$($p),+| { $body }
        }
    );
}

mod line;
mod chooser;
mod hunks;

pub use self::chooser::CommandChooserController;
pub use self::line::CommandLine;

pub static INTERFACE_SRC: &'static str = include_str!("interface.glade");

use std::collections::BTreeMap;
use state::{CommandDescriptor, CommandState, Message, ThreadNotifier, ChainType, Chain};
use uuid::Uuid;
use std::rc::Rc;
use std::fmt;
use std::cell::RefCell;
use std::sync::mpsc::{Sender, Receiver};
use backend::BackendSender;
use gtk::{Builder, Label, TreeStore, ListStore, Window, Image};
use gtk::prelude::*;
use gdk::enums::key as gkey;
use std::ops::Deref;

#[derive(Clone)]
pub struct UISender {
    tx: Sender<Message>,
    tn: ThreadNotifier
}
impl UISender {
    pub fn new(tx: Sender<Message>, tn: ThreadNotifier) -> Self {
        UISender {
            tx: tx,
            tn: tn
        }
    }
    /// WARNING: This function will cause a borrow of all UI elements.
    /// It thus stands that you should NOT call this in a UI element if you're borrowing it,
    /// as this will cause a panic.
    pub fn send(&mut self, m: Message) {
        self.tx.send(m).unwrap();
        self.tn.notify();
    }
}
#[derive(Clone)]
pub enum UIMode {
    Live(ChainType),
    Blind(ChainType)
}
impl UIMode {
    fn get_ct(&self) -> ChainType {
        match self {
            &UIMode::Live(ref ct) => ct.clone(),
            &UIMode::Blind(ref ct) => ct.clone(),
        }
    }
}
impl fmt::Display for UIMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &UIMode::Live(ref ct) => write!(f, "LIVE: {}", ct),
            &UIMode::Blind(ref ct) => write!(f, "BLIND: {}", ct),
        }
    }
}
pub struct UIContext {
    pub commands: BTreeMap<Uuid, CommandDescriptor>,
    pub identifiers: BTreeMap<String, Uuid>,
    pub chains: BTreeMap<ChainType, Chain>,
    pub chooser: Rc<RefCell<CommandChooserController>>,
    pub line: Rc<RefCell<CommandLine>>,
    pub store: TreeStore,
    pub completions: ListStore,
    pub rx: Receiver<Message>,
    pub uitx: Sender<Message>,
    pub mode: UIMode
}
impl UIContext {
    pub fn init(sender: BackendSender, uisender: Sender<Message>, recvr: Receiver<Message>, tn: ThreadNotifier, win: Window, builder: &Builder)  -> Rc<RefCell<Self>> {
        let compl: ListStore = builder.get_object("command-identifiers-list").unwrap();
        let line = CommandLine::new(sender, compl.clone(), &builder);
        let ccc = CommandChooserController::new(line.clone(), UIMode::Live(ChainType::Unattached), UISender::new(uisender.clone(), tn.clone()), &builder);
        let uic = Rc::new(RefCell::new(UIContext {
            commands: BTreeMap::new(),
            identifiers: BTreeMap::new(),
            chains: BTreeMap::new(),
            chooser: ccc,
            line: line,
            rx: recvr,
            completions: compl,
            store: builder.get_object("command-tree").unwrap(),
            uitx: uisender,
            mode: UIMode::Live(ChainType::Unattached)
        }));
        tn.register_handler(clone!(uic; || {
            UIContext::handler(uic.clone());
        }));
        win.connect_key_press_event(clone!(uic; |_s, ek| {
            if ek.get_state().contains(::gdk::CONTROL_MASK) {
                match ek.get_keyval() {
                    gkey::Return => {
                        CommandChooserController::toggle(uic.borrow().chooser.clone());
                        Inhibit(true)
                    },
                    _ => Inhibit(false)
                }
            }
            else {
                Inhibit(false)
            }
        }));
        win.connect_delete_event(|_, _| {
            ::gtk::main_quit();
            Inhibit(false)
        });
        win.show_all();

        uic
    }
    pub fn cstate(&self, uu: Uuid) -> u16 {
        if let Some(ref cd) = self.line.borrow().cd {
            if cd.uuid == uu {
                1
            } else { 0 }
        }
        else if let Some(uu2) = self.line.borrow().uuid {
            if uu == uu2 {
                2
            } else { 0 }
        } else { 0 }
    }
    pub fn update(&mut self) {
        self.store.clear();
        self.completions.clear();
        for (ref ct, ref chn) in &self.chains {
            for (i, ref uu) in chn.commands.iter().enumerate() {
                if let Some(ref v) = self.commands.get(uu) {
                    let (mut icon, ident, desc, mut dur, mut bgc) =
                        (format!("dialog-question"),
                         format!("{}<span fgcolor=\"#666666\">{}</span>", ct, i),
                         v.desc.clone(),
                         format!(""),
                         format!("white"));
                    match v.state {
                        CommandState::Incomplete => {
                            icon = format!("dialog-error");
                            bgc = format!("lightpink");
                        },
                        CommandState::Ready => {
                            icon = format!("");
                        },
                        CommandState::Loaded => {
                            icon = format!("go-home");
                            bgc = format!("lemonchiffon");
                        },
                        CommandState::Running(cd) => {
                            icon = format!("media-seek-forward");
                            bgc = format!("powderblue");
                            dur = format!("{:02}:{:02}:{:02}",
                                          cd.num_hours(),
                                          cd.num_minutes() - (60 * cd.num_hours()),
                                          cd.num_seconds() - (60 * cd.num_minutes()));
                        },
                        _ => {}
                    }
                    self.store.set(&self.store.append(None), &vec![
                        0, // icon
                        1, // identifier (looking glass column)
                        2, // description
                        3, // duration
                        4, // background colour
                    ], &vec![
                        &icon as &ToValue,
                        &ident as &ToValue,
                        &desc as &ToValue,
                        &dur as &ToValue,
                        &bgc as &ToValue,
                    ].deref());
                    self.completions.set(&self.completions.append(), &vec![
                        0, // identifier
                        1, // uuid
                        2, // description
                        3, // icon
                    ], &vec![
                        &format!("{}{}", ct, i) as &ToValue,
                        &format!("{}", uu) as &ToValue,
                        &desc as &ToValue,
                        &icon as &ToValue,
                    ].deref());
                    for (k, v) in self.identifiers.iter() {
                        if *uu == v {
                            self.completions.set(&self.completions.append(), &vec![
                                0, // identifier
                                1, // uuid
                                2, // description
                                3, // icon
                            ], &vec![
                                &format!("${}", k) as &ToValue,
                                &format!("{}", uu) as &ToValue,
                                &desc as &ToValue,
                                &icon as &ToValue,
                            ].deref());
                        }
                    }
                }
            }
        }
    }
    pub fn handler(selfish: Rc<RefCell<Self>>) {
        let mut selfish = selfish.borrow_mut();
        let msg = selfish.rx.recv().unwrap();
        match msg {
            Message::CmdDesc(uu, desc) => {
                selfish.commands.insert(uu, desc.clone());
                match selfish.cstate(uu) {
                    1 => {
                        CommandLine::update(selfish.line.clone(), Some(desc));
                        CommandChooserController::update(selfish.chooser.clone());
                    },
                    2 => {
                        CommandLine::build(selfish.line.clone(), desc);
                        CommandChooserController::update(selfish.chooser.clone());
                    },
                    _ => {}
                }
                selfish.update();
            },
            Message::Deleted(uu) => {
                selfish.commands.remove(&uu);
                match selfish.cstate(uu) {
                    1 => {
                        selfish.line.borrow_mut().cd = None;
                        CommandLine::update(selfish.line.clone(), None);
                        CommandChooserController::update(selfish.chooser.clone());
                    },
                    2 => {
                        selfish.line.borrow_mut().uuid = None;
                        CommandLine::update(selfish.line.clone(), None);
                        CommandChooserController::update(selfish.chooser.clone());
                    },
                    _ => {}
                }
                selfish.update();
            },
            Message::ChainDesc(ct, chn) => {
                selfish.chains.insert(ct, chn);
                selfish.update();
            },
            Message::ChainDeleted(ct) => {
                selfish.chains.remove(&ct);
                selfish.update();
            },
            Message::Identifiers(id) => {
                selfish.identifiers = id;
                selfish.update();
            },
            Message::UIChangeMode(ct) => {
                selfish.mode = ct.clone();
                CommandChooserController::set_mode(selfish.chooser.clone(), ct);
            },
            _ => unimplemented!()
        }
    }
}

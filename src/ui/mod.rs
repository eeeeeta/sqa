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
mod list;

use self::chooser::CommandChooserController;
use self::line::{CommandLine, CommandLineFSM};
use self::list::ListController;

pub static INTERFACE_SRC: &'static str = include_str!("interface.glade");

use state::{CommandDescriptor, CommandState, Message, ThreadNotifier, ChainType, Chain};
use uuid::Uuid;
use std::rc::Rc;
use std::fmt;
use std::cell::RefCell;
use std::default::Default;
use std::sync::mpsc::{Sender, Receiver};
use backend::BackendSender;
use gtk::{Builder, ListStore, Window};
use gtk::prelude::*;
use gdk::enums::key as gkey;

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
pub struct UIState {
    live: bool,
    sel: Option<(ChainType, usize)>
}
impl Default for UIState {
    fn default() -> Self {
        UIState {
            live: true,
            sel: None
        }
    }
}
pub struct UIContext {
    pub chooser: Rc<RefCell<CommandChooserController>>,
    pub line: Rc<RefCell<CommandLine>>,
    pub completions: ListStore,
    pub list: Rc<RefCell<ListController>>,
    pub rx: Receiver<Message>,
    pub uitx: Sender<Message>,
    pub tx: BackendSender,
    pub state: UIState
}
impl UIContext {
    pub fn init(sender: BackendSender, uisender: Sender<Message>, recvr: Receiver<Message>, tn: ThreadNotifier, win: Window, builder: &Builder)  -> Rc<RefCell<Self>> {
        let compl: ListStore = builder.get_object("command-identifiers-list").unwrap();
        let line = CommandLine::new(sender.clone(), compl.clone(), Default::default(), &builder);
        let ccc = CommandChooserController::new(line.clone(), Default::default(), UISender::new(uisender.clone(), tn.clone()), &builder);
        let uic = Rc::new(RefCell::new(UIContext {
            chooser: ccc,
            line: line,
            rx: recvr,
            list: ListController::new(UISender::new(uisender.clone(), tn.clone()), sender.clone(), compl.clone(), &builder),
            completions: compl,
            tx: sender,
            uitx: uisender,
            state: Default::default()
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
    fn update_cline(&self, uu: Uuid) -> bool {
        match self.line.borrow().state {
            CommandLineFSM::AwaitingCreation(u2) => { u2 == uu },
            CommandLineFSM::Editing(ref u2, _) => { u2.uuid == uu },
            _ => false
        }
    }
    fn handler(selfish: Rc<RefCell<Self>>) {
        let mut selfish = selfish.borrow_mut();
        let msg = selfish.rx.recv().unwrap();
        match msg {
            Message::CmdDesc(uu, desc) => {
                if selfish.update_cline(uu) {
                    CommandLine::update(selfish.line.clone(), Some(desc.clone()));
                    CommandChooserController::update(selfish.chooser.clone());
                }
                ListController::update_desc(selfish.list.clone(), uu, desc);
            },
            Message::Deleted(uu) => {
                if selfish.update_cline(uu) {
                    selfish.line.borrow_mut().state = CommandLineFSM::Idle;
                    CommandLine::update(selfish.line.clone(), None);
                    CommandChooserController::update(selfish.chooser.clone());
                }
                ListController::delete(selfish.list.clone(), uu);
            },
            Message::ChainDesc(ct, chn) => {
                ListController::update_chain(selfish.list.clone(), ct, Some(chn));
            },
            Message::ChainDeleted(ct) => {
                ListController::update_chain(selfish.list.clone(), ct, None);
            },
            Message::ChainFallthru(ct, state) => {
                ListController::update_chain_fallthru(selfish.list.clone(), ct, state);
            },
            Message::Identifiers(id) => {
                ListController::update_identifiers(selfish.list.clone(), id);
            },
            Message::UIChangeLive(live) => {
                selfish.state.live = live;
                CommandChooserController::set_ui_state(selfish.chooser.clone(), selfish.state.clone());
            },
            Message::UIChangeSel(sel) => {
                selfish.state.sel = sel;
                CommandChooserController::set_ui_state(selfish.chooser.clone(), selfish.state.clone());
            },
            Message::UIBeginEditing(uu) => {
                if let Some(desc) = selfish.list.borrow().commands.get(&uu) {
                    CommandLine::edit_command(selfish.line.clone(), desc.clone());
                }
            },
            Message::UIToggleFallthru(uu) => {
                selfish.tx.send(Message::SetFallthru(uu, !ListController::get_fallthru_state(selfish.list.clone(), uu))).unwrap();
            },
            _ => unimplemented!()
        }
    }
}

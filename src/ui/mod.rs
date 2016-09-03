//! User interaction subsystem
//!
//! SQA uses GTK+ as its UI toolkit. (This is because, at the time of writing,
//! the GTK+ bindings are the best choice for cross-platform UIs in Rust.)
//!
//! The general layout of the UI code is as follows:
//!
//! - There is one central `UIContext`, which is responsible for handling `state::Message`s.
//! - This `UIContext` owns all of the individual UI components, and lets them know of any messages
//!   that require them to do something.
//! - When these components wish to make a change, they either notify the backend directly or
//!   use a `UISender` to notify the `UIContext`. From here, changes will be made, and communicated
//!   back to the `UIContext`, which will then let them know of the outcome.
//!
//! Also, note the fact that most UI components, instead of taking `&self`, will take
//! `Rc<RefCell<Self>>` and borrow it themselves.

/// Macro for cloning objects to be used in closures.
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
mod header;

use self::chooser::CommandChooserController;
use self::line::{CommandLine, CommandLineFSM};
use self::list::ListController;
use self::header::HeaderController;

/// Glade source for the UI itself. Found in `sqa/src/ui/interface.glade`.
pub static INTERFACE_SRC: &'static str = include_str!("interface.glade");

use state::{Message, ThreadNotifier, ChainType};
use uuid::Uuid;
use std::rc::Rc;
use std::cell::RefCell;
use std::default::Default;
use std::sync::mpsc::{Sender, Receiver};
use backend::BackendSender;
use gtk::{Builder, ListStore, Window};
use gtk::prelude::*;
use gdk::enums::key as gkey;

/// Communication channel from UI objects to the `UIContext`.
///
/// Essentially, this is just a clone of the mechanisms the backend uses
/// to transmit messages to the `UIContext`.
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
    /// Sends a message.
    pub fn send(&mut self, m: Message) {
        self.tx.send(m).unwrap();
        self.tn.notify();
    }
}
/// Overall state of the user interface.
#[derive(Clone)]
pub struct UIState {
    /// Whether we are in Live mode (if false, then Blind)
    live: bool,
    /// What command is selected in the main TreeView (if any)
    sel: Option<(ChainType, Uuid)>
}
impl Default for UIState {
    fn default() -> Self {
        UIState {
            live: true,
            sel: None
        }
    }
}
/// Central UI context, responsible for receiving communications and passing them on
/// to other UI components.
///
/// See the module documentation for more information on this system.
pub struct UIContext {
    pub chooser: Rc<RefCell<CommandChooserController>>,
    pub line: Rc<RefCell<CommandLine>>,
    pub list: Rc<RefCell<ListController>>,
    pub header: Rc<RefCell<HeaderController>>,
    pub completions: ListStore,
    pub rx: Receiver<Message>,
    pub uitx: UISender,
    pub tx: BackendSender,
    pub state: UIState
}
impl UIContext {
    /// Makes a new UIContext.
    pub fn init(sender: BackendSender, uisender: Sender<Message>, recvr: Receiver<Message>, tn: ThreadNotifier, win: Window, builder: &Builder)  -> Rc<RefCell<Self>> {
        let compl: ListStore = builder.get_object("command-identifiers-list").unwrap();
        let line = CommandLine::new(sender.clone(), compl.clone(), Default::default(), UISender::new(uisender.clone(), tn.clone()), builder);
        let ccc = CommandChooserController::new(line.clone(), Default::default(), UISender::new(uisender.clone(), tn.clone()), builder);
        let uic = Rc::new(RefCell::new(UIContext {
            chooser: ccc,
            line: line,
            rx: recvr,
            list: ListController::new(UISender::new(uisender.clone(), tn.clone()), sender.clone(), compl.clone(), builder),
            header: HeaderController::new(UISender::new(uisender.clone(), tn.clone()), builder),
            completions: compl,
            tx: sender,
            uitx: UISender::new(uisender.clone(), tn.clone()),
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
                    gkey::space => {
                        let btn = {
                            let uic = uic.borrow();
                            let header = uic.header.borrow();
                            if header.cur.is_some() {
                                Some(header.go_btn.clone())
                            }
                            else {
                                None
                            }
                        };
                        if let Some(btn) = btn {
                            btn.clicked();
                        }
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
    /// Asks "does the command line need to know about changes to this UUID?"
    fn update_cline(&self, uu: Uuid) -> bool {
        match self.line.borrow().state {
            CommandLineFSM::AwaitingCreation(u2) => { u2 == uu },
            CommandLineFSM::Editing(ref u2, _) => { u2.uuid == uu },
            _ => false
        }
    }
    /// Main handler function for incoming messages. Called by the `ThreadNotifier`.
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
                if let Some((_, u2)) = selfish.state.sel {
                    if u2 == uu {
                        selfish.uitx.send(Message::UIChangeSel(None));
                    }
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
                if let Some(sel) = sel {
                    let mut chain = ChainType::Unattached;
                    for (ct, chn) in &selfish.list.borrow().chains {
                        if chn.commands.contains(&sel) {
                            chain = *ct;
                            break;
                        }
                    }
                    selfish.state.sel = Some((chain, sel));
                    ListController::update_sel(selfish.list.clone(), Some(sel));
                    CommandChooserController::set_ui_state(selfish.chooser.clone(), selfish.state.clone());
                }
                else {
                    selfish.state.sel = None;
                    ListController::update_sel(selfish.list.clone(), None);
                    CommandChooserController::set_ui_state(selfish.chooser.clone(), selfish.state.clone());
                }
                HeaderController::update_sel(selfish.header.clone(), selfish.state.sel);
            },
            Message::UIBeginEditing(uu) => {
                if let Some(desc) = selfish.list.borrow().commands.get(&uu) {
                    CommandLine::edit_command(selfish.line.clone(), desc.clone());
                }
            },
            Message::UIToggleFallthru(uu) => {
                selfish.tx.send(Message::SetFallthru(uu, !ListController::get_fallthru_state(selfish.list.clone(), uu))).unwrap();
            },
            Message::UIGo(ct) => {
                selfish.tx.send(Message::Go(ct)).unwrap();
                let mut new_sel = None;
                if let ChainType::Q(mut num) = ct {
                    let list = selfish.list.borrow();
                    num += 1;
                    if list.chains.get(&ChainType::Q(num)).is_some() {
                        if let Some(uu) = list.chains.get(&ChainType::Q(num)).as_ref().unwrap().commands.get(0) {
                            new_sel = Some(*uu);
                        }
                    }
                }
                selfish.uitx.send(Message::UIChangeSel(new_sel));
            },
            _ => unimplemented!()
        }
    }
}

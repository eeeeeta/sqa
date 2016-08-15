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

use std::collections::BTreeMap;
use state::{CommandDescriptor, CommandState, Message, ThreadNotifier};
use uuid::Uuid;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::mpsc::Receiver;
use backend::BackendSender;
use gtk::{Builder, Label, TreeStore, Window, Image};
use gtk::prelude::*;
use gdk::enums::key as gkey;
use std::ops::Deref;

pub struct UIContext {
    pub commands: BTreeMap<Uuid, CommandDescriptor>,
    pub chooser: Rc<RefCell<CommandChooserController>>,
    pub line: Rc<RefCell<CommandLine>>,
    pub store: TreeStore,
    pub rx: Receiver<Message>
}
impl UIContext {
    pub fn init(sender: BackendSender, recvr: Receiver<Message>, tn: ThreadNotifier, win: Window, builder: &Builder)  -> Rc<RefCell<Self>> {
        let line = CommandLine::new(sender, &builder);
        let ccc = CommandChooserController::new(line.clone(), &builder);
        let uic = Rc::new(RefCell::new(UIContext {
            commands: BTreeMap::new(),
            chooser: ccc,
            line: line,
            rx: recvr,
            store: builder.get_object("command-tree").unwrap()
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
        for (ref k, ref v) in &self.commands {
            let ref ti = self.store.insert(None, -1);
            self.store.set(ti, &vec![0], vec![&"dialog-question".to_string() as &ToValue].deref());
            self.store.set(ti, &vec![2], vec![&format!("") as &ToValue].deref());
            self.store.set(ti, &vec![3], vec![&v.desc as &ToValue].deref());
            self.store.set(ti, &vec![4], vec![&format!("") as &ToValue].deref());
            self.store.set(ti, &vec![5], vec![&format!("white") as &ToValue].deref());
            match v.state {
                CommandState::Incomplete => {
                    self.store.set(ti, &vec![0], vec![&"dialog-error".to_string() as &ToValue].deref());
                    self.store.set(ti, &vec![5], vec![&format!("lightpink") as &ToValue].deref());
                },
                CommandState::Ready => {
                    self.store.set(ti, &vec![0], vec![&"".to_string() as &ToValue].deref());
                },
                CommandState::Loaded => {
                    self.store.set(ti, &vec![0], vec![&"go-home".to_string() as &ToValue].deref());
                    self.store.set(ti, &vec![5], vec![&format!("lemonchiffon") as &ToValue].deref());
                },
                CommandState::Running(dur) => {
                    self.store.set(ti, &vec![0], vec![&"media-seek-forward".to_string() as &ToValue].deref());
                    self.store.set(ti, &vec![5], vec![&format!("powderblue") as &ToValue].deref());
                    self.store.set(ti, &vec![4], vec![
                        &format!("{:02}:{:02}:{:02}",
                                 dur.num_hours(),
                                 dur.num_minutes() - (60 * dur.num_hours()),
                                 dur.num_seconds() - (60 * dur.num_minutes())) as &ToValue].deref());
                },
                _ => {}
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
            _ => unimplemented!()
        }
    }
}

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
use state::{CommandDescriptor, Message, ThreadNotifier};
use uuid::Uuid;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::mpsc::Receiver;
use backend::BackendSender;
use gtk::{Builder, Label, ListBox, Window};
use gtk::prelude::*;
use gdk::enums::key as gkey;

pub struct UIContext {
    pub commands: BTreeMap<Uuid, CommandDescriptor>,
    pub chooser: Rc<RefCell<CommandChooserController>>,
    pub line: Rc<RefCell<CommandLine>>,
    pub clist: ListBox,
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
            clist: builder.get_object("active-command-list").unwrap()
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
        for chld in self.clist.get_children() {
            chld.destroy();
        }
        for (ref k, ref v) in &self.commands {
            let label = Label::new(None);
            label.set_markup(&format!("{:?}: {} ({})", v.state, v.desc, k));
            self.clist.add(&label);
            self.clist.show_all();
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

use commands::{get_chooser_grid, GridNode};
use std::cell::RefCell;
use std::rc::Rc;
use gdk::enums::key as gkey;
use gtk::prelude::*;
use gtk::{Label, Grid, Button, Builder, Entry, Popover};
use std::ops::Rem;
use state::Message;
use uuid::Uuid;
use state::ChainType;
use super::line::{CommandLine, CommandLineFSM};
use super::{UISender, UIMode};

pub struct CommandChooserController {
    grid: Grid,
    mode: UIMode,
    sender: UISender,
    back_btn: Button,
    status_lbl: Label,
    pop: Popover,
    prompt_pop: Popover,
    prompt_lbl: Label,
    prompt_ent: Entry,
    prompt_handler: Box<Fn(String)>,
    cl: Rc<RefCell<CommandLine>>,
    pos: Vec<usize>,
    top: Vec<(&'static str, gkey::Key, GridNode)>
}

impl CommandChooserController {
    pub fn new(cl: Rc<RefCell<CommandLine>>, mode: UIMode, sender: UISender, b: &Builder) -> Rc<RefCell<Self>> {
        let ret = Rc::new(RefCell::new(CommandChooserController {
            grid: b.get_object("cc-grid").unwrap(),
            back_btn: b.get_object("cc-end-button").unwrap(),
            status_lbl: b.get_object("cc-status-label").unwrap(),
            pop: b.get_object("command-chooser-popover").unwrap(),
            prompt_pop: b.get_object("prompt-popover").unwrap(),
            prompt_lbl: b.get_object("prompt-popover-label").unwrap(),
            prompt_ent: b.get_object("prompt-popover-entry").unwrap(),
            prompt_handler: Box::new(|_| {}),
            pos: vec![],
            cl: cl,
            top: get_chooser_grid(),
            mode: mode,
            sender: sender
        }));
        ret.borrow().prompt_ent.connect_activate(clone!(ret; |ent| {
            let mut ret = ret.borrow_mut();
            if let Some(txt) = ent.get_text() {
                (ret.prompt_handler)(txt);
                ret.prompt_handler = Box::new(|_| {});
            }
            ret.prompt_pop.hide();
        }));
        ret.borrow().prompt_ent.connect_icon_press(|ent, _, _| {
            ent.activate();
        });
        ret.borrow().back_btn.connect_clicked(clone!(ret; |_s| {
            {
                let mut pos = &mut ret.borrow_mut().pos;
                let len = pos.len().saturating_sub(1);
                pos.truncate(len);
            }
            Self::update(ret.clone());
        }));
        ret.borrow().pop.connect_key_press_event(clone!(ret; |_s, ek| {
            if ek.get_keyval() == gkey::BackSpace {
                ret.borrow().back_btn.clone().activate();
                return Inhibit(true)
            }
            let mut wdgt: Option<::gtk::Widget> = None;
            {
                let selfish = ret.borrow();
                for (i, &(_, key, _)) in selfish.get_ptr().0.iter().rev().enumerate() {
                    if ek.get_keyval() == key {
                        wdgt = Some(selfish.grid.get_children()[i].clone());
                        break;
                    }
                }
            }
            if let Some(w) = wdgt {
                let w = w.downcast::<Button>().unwrap();
                if w.is_sensitive() {
                    w.clicked();
                }
                else {
                    let sctx = _s.get_style_context().unwrap();
                    sctx.add_class("err-pulse");
                    ::gdk::beep();
                    timeout_add(450, move || {
                        sctx.remove_class("err-pulse");
                        Continue(false)
                    });
                }
                Inhibit(true)
            }
            else {
                Inhibit(false)
            }
        }));
        ret
    }
    pub fn toggle(selfish_: Rc<RefCell<Self>>) {
        {
            let mut selfish = selfish_.borrow_mut();
            selfish.pos = vec![];
            selfish.pop.show_all();
        }
        Self::update(selfish_);
    }
    pub fn execute(selfish_: Rc<RefCell<Self>>, cl: Rc<RefCell<CommandLine>>) {
        {
            let mut cl = cl.borrow_mut();
            let selfish = selfish_.borrow();
            if !cl.ready { return; }
            if let CommandLineFSM::Editing(cd, creation) = ::std::mem::replace(&mut cl.state, CommandLineFSM::Idle) {
                if creation {
                    match selfish.mode {
                        UIMode::Live(_) => cl.tx.send(Message::Execute(cd.uuid)).unwrap(),
                        UIMode::Blind(ref ct) => cl.tx.send(Message::Attach(cd.uuid, ct.clone())).unwrap()
                    }
                }
            }
        }
        CommandLine::update(cl, None);
    }
    fn get_ptr(&self) -> (&Vec<(&'static str, gkey::Key, GridNode)>, String) {
        let mut ptr = &self.top;
        let mut st = "🏠".to_string(); // U+1F3E0 HOUSE BUILDING
        if self.pos.len() > 0 {
            for i in &self.pos {
                if let Some(&(ref disp, _, GridNode::Grid(ref vec))) = ptr.get(*i) {
                    ptr = vec;
                    st.push_str(" → ");
                    st.push_str(disp);
                }
                else {
                    panic!("Grid traversal failed");
                }
            }
            self.back_btn.set_sensitive(true);
        }
        else {
            self.back_btn.set_sensitive(false);
        }
        (ptr, st)
    }
    pub fn set_mode(selfish: Rc<RefCell<Self>>, mode: UIMode) {
        {
            selfish.borrow_mut().mode = mode;
        }
        CommandChooserController::update(selfish);
    }
    pub fn update(selfish_: Rc<RefCell<Self>>) {
        let selfish = selfish_.borrow();
        let (ptr, st) = selfish.get_ptr();
        selfish.status_lbl.set_markup(&st);
        for chld in selfish.grid.get_children() {
            chld.destroy();
        }
        for (i, &(st, _, ref opt)) in ptr.iter().enumerate() {
            let lbl = Label::new(None);
            let btn = Button::new();
            lbl.set_markup(st);
            match opt {
                &GridNode::Choice(spawner) => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        selfish_.borrow().pop.hide();
                        CommandLine::new_command(cl.clone(), spawner);
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-choice");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                },
                &GridNode::Grid(_) => {
                    btn.connect_clicked(clone!(selfish_; |_s| {
                        {
                            selfish_.borrow_mut().pos.push(i);
                        }
                        Self::update(selfish_.clone());
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-grid");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                },
                &GridNode::Mode => {
                    btn.connect_clicked(clone!(selfish_; |_s| {
                        let (mut sender, mode) = {
                            let selfish = selfish_.borrow_mut();
                            selfish.pop.hide();
                            (selfish.sender.clone(), match selfish.mode {
                                UIMode::Live(ref ct) => UIMode::Blind(ct.clone()),
                                UIMode::Blind(ref ct) => UIMode::Live(ct.clone())
                            })
                        };
                        sender.send(Message::UIChangeMode(mode));
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-mode");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                },
                &GridNode::Clear => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        CommandLine::reset(cl.clone());
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if let CommandLineFSM::Editing(..) = cl.borrow().state {
                        lbl.get_style_context().unwrap().add_class("gridnode-clear");
                        btn.set_sensitive(true);
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                },
                &GridNode::Go => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        {
                            let cl = cl.borrow();
                            let selfish = selfish_.borrow();
                            let ct = selfish.mode.get_ct();
                            cl.tx.send(Message::Go(ct)).unwrap();
                        }
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    lbl.get_style_context().unwrap().add_class("gridnode-execute");
                    lbl.set_markup(&format!("Go {} <b>G</b>", selfish.mode.get_ct()));
                },
                &GridNode::GotoQ => {
                    let ref cl = selfish.cl;
                    selfish.prompt_pop.set_relative_to(Some(&cl.borrow().line));
                    btn.connect_clicked(clone!(selfish_; |_s| {
                        {
                            let mut selfish = selfish_.borrow_mut();
                            selfish.pop.hide();
                            selfish.prompt_lbl.set_markup(&format!("<b>Go to cue:</b>"));
                            let sender = selfish.sender.clone();
                            let mode = selfish.mode.clone();
                            selfish.prompt_handler = Box::new(move |txt| {
                                let mode = match mode {
                                    UIMode::Live(_) => UIMode::Live(ChainType::Q(txt)),
                                    UIMode::Blind(_) => UIMode::Blind(ChainType::Q(txt))
                                };
                                // FIXME: why the double clone?
                                sender.clone().send(Message::UIChangeMode(mode));
                            });
                            selfish.prompt_pop.show_all();
                        }
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    lbl.get_style_context().unwrap().add_class("gridnode-clear");
                },
                &GridNode::Execute => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        Self::execute(selfish_.clone(), cl.clone());
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    match selfish.mode {
                        UIMode::Live(_) => lbl.set_markup(&format!("Execute <b>↵</b>")),
                        UIMode::Blind(ref ct) => lbl.set_markup(&format!("Attach to {} <b>↵</b>", ct))
                    }
                    if cl.borrow().ready {
                        btn.set_sensitive(true);
                        lbl.get_style_context().unwrap().add_class("gridnode-execute");
                        if let CommandLineFSM::Editing(_, creation) = cl.borrow().state {
                            if !creation {
                                lbl.set_markup(&format!("Done <b>↵</b>"));
                            }
                        }
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                }
            }
            btn.add(&lbl);
            selfish.grid.attach(&btn, i.rem(3) as i32, (i/3) as i32, 1, 1);
        }
        selfish.grid.show_all();
    }
}

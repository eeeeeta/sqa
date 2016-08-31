//! Control of the Control-Enter menu popup (the command chooser).

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
use std::ascii::AsciiExt;
use super::line::{CommandLine, CommandLineFSM};
use super::{UISender, UIState};

/// The command chooser itself.
pub struct CommandChooserController {
    grid: Grid,
    state: UIState,
    sender: UISender,
    back_btn: Button,
    status_lbl: Label,
    pop: Popover,
    prompt_pop: Popover,
    prompt_lbl: Label,
    prompt_ent: Entry,
    prompt_handler: Box<Fn(&mut CommandChooserController, String) -> bool>,
    cl: Rc<RefCell<CommandLine>>,
    pos: Vec<usize>,
    top: Vec<(&'static str, &'static str, gkey::Key, GridNode)>
}

impl CommandChooserController {
    pub fn new(cl: Rc<RefCell<CommandLine>>, state: UIState, sender: UISender, b: &Builder) -> Rc<RefCell<Self>> {
        let ret = Rc::new(RefCell::new(CommandChooserController {
            grid: b.get_object("cc-grid").unwrap(),
            back_btn: b.get_object("cc-end-button").unwrap(),
            status_lbl: b.get_object("cc-status-label").unwrap(),
            pop: b.get_object("command-chooser-popover").unwrap(),
            prompt_pop: b.get_object("prompt-popover").unwrap(),
            prompt_lbl: b.get_object("prompt-popover-label").unwrap(),
            prompt_ent: b.get_object("prompt-popover-entry").unwrap(),
            prompt_handler: Box::new(|_, _| {true}),
            pos: vec![],
            cl: cl,
            top: get_chooser_grid(),
            state: state,
            sender: sender
        }));
        ret.borrow().prompt_ent.connect_activate(clone!(ret; |ent| {
            let mut ret = ret.borrow_mut();
            if let Some(txt) = ent.get_text() {
                let ph = ::std::mem::replace(&mut ret.prompt_handler, Box::new(|_, _| {true}));
                if ph(&mut ret, txt) {
                    ret.prompt_pop.hide();
                }
                else {
                    let sctx = ret.prompt_pop.get_style_context().unwrap();
                    sctx.add_class("err-pulse");
                    ::gdk::beep();
                    timeout_add(450, move || {
                        sctx.remove_class("err-pulse");
                        Continue(false)
                    });
                    ret.prompt_handler = ph;
                }
            }
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
                for (i, &(_, _, key, _)) in selfish.get_ptr().0.iter().rev().enumerate() {
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
                if creation && selfish.state.live {
                    cl.tx.send(Message::Execute(cd.uuid)).unwrap();
                }
            }
        }
        CommandLine::update(cl, None);
    }
    fn get_ptr(&self) -> (&Vec<(&'static str, &'static str, gkey::Key, GridNode)>, String) {
        let mut ptr = &self.top;
        let mut st = "🏠".to_string(); // U+1F3E0 HOUSE BUILDING
        if self.pos.len() > 0 {
            for i in &self.pos {
                if let Some(&(ref disp, _, _, GridNode::Grid(ref vec))) = ptr.get(*i) {
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
    pub fn set_ui_state(selfish: Rc<RefCell<Self>>, state: UIState) {
        {
            let mut selfish = selfish.borrow_mut();
            selfish.state = state.clone();
            CommandLine::set_ui_state(selfish.cl.clone(), state);
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
        for (i, &(st, tooltip, _, ref opt)) in ptr.iter().enumerate() {
            let lbl = Label::new(None);
            let btn = Button::new();
            lbl.set_markup(st);
            lbl.set_tooltip_markup(Some(tooltip));
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
                        let (mut sender, live) = {
                            let selfish = selfish_.borrow_mut();
                            (selfish.sender.clone(), !selfish.state.live)
                        };
                        sender.send(Message::UIChangeLive(live));
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-mode");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if selfish.state.live {
                        lbl.set_markup(&format!("»Blind <b>O</b>"));
                        lbl.set_tooltip_markup(Some(&format!("Switch to BLIND mode")));
                    }
                    else {
                        lbl.set_markup(&format!("»Live <b>O</b>"));
                        lbl.set_tooltip_markup(Some(&format!("Switch to LIVE mode")));
                    }
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
                &GridNode::Fallthru => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        if let CommandLineFSM::Editing(ref cd, _) = cl.borrow().state {
                            selfish_.borrow_mut().sender.send(Message::UIToggleFallthru(cd.uuid));
                        }
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if let CommandLineFSM::Editing(..) = cl.borrow().state {
                        lbl.get_style_context().unwrap().add_class("gridnode-choice");
                        btn.set_sensitive(true);
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                },
                &GridNode::Reorder => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_; |_s| {
                        {
                            let mut selfish = selfish_.borrow_mut();
                            selfish.pop.hide();
                            selfish.prompt_lbl.set_markup(&format!(
                                "<b>New position:</b>
<small>Enter a chain name (eg: <i>X</i>, <i>Q1</i>) to attach to,
followed by an optional position (eg: <i>X-1</i>, <i>Q1-4</i>)</small>"));
                            selfish.prompt_handler = Box::new(move |s, txt| {
                                let mut txt = txt.split("-");
                                let chn = txt.next();
                                let pos = txt.next();

                                let mut ct = None;
                                let mut idx = None;
                                if let Some(chn) = chn {
                                    let mut chn = chn.to_string();
                                    if chn.len() >= 1 {
                                        match chn.remove(0).to_ascii_lowercase() {
                                            'x' => ct = Some(ChainType::Unattached),
                                            'q' => {
                                                if let Ok(num) = chn.parse::<usize>() {
                                                    ct = Some(ChainType::Q(num));
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                }
                                if let Some(pos) = pos {
                                    if let Ok(pos) = pos.parse::<usize>() {
                                        idx = Some(pos);
                                    }
                                }
                                let mut cl = s.cl.borrow_mut();
                                if let CommandLineFSM::Editing(cd, _) = cl.state.clone() {
                                    if let Some(ct) = ct {
                                        if let Some(idx) = idx {
                                            cl.tx.send(Message::Insert(cd.uuid, ct, idx)).unwrap();
                                            return true;
                                        }
                                        else {
                                            cl.tx.send(Message::Attach(cd.uuid, ct)).unwrap();
                                            return true;
                                        }
                                    }
                                }
                                false
                            });
                            selfish.prompt_pop.show_all();
                        }
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if let CommandLineFSM::Editing(..) = cl.borrow().state {
                        lbl.get_style_context().unwrap().add_class("gridnode-execute");
                        btn.set_sensitive(true);
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                },
                &GridNode::Execute => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        Self::execute(selfish_.clone(), cl.clone());
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if selfish.state.live {
                        lbl.set_markup(&format!("Execute <b>↵</b>"));
                        lbl.set_tooltip_markup(Some(&format!("Execute this command now")));
                    }
                    else {
                        lbl.set_markup(&format!("Create <b>↵</b>"));
                        lbl.set_tooltip_markup(Some(&format!("Create this command")));
                    }
                    if cl.borrow().ready {
                        btn.set_sensitive(true);
                        lbl.get_style_context().unwrap().add_class("gridnode-execute");
                        if let CommandLineFSM::Editing(_, creation) = cl.borrow().state {
                            if !creation {
                                lbl.set_markup(&format!("Finish <b>↵</b>"));
                                lbl.set_tooltip_markup(Some(&format!("Finish editing this command")));
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

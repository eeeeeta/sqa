use command::Command;
use commands::{get_chooser_grid, GridNode};
use std::cell::RefCell;
use std::rc::Rc;
use gdk::enums::key as gkey;
use gtk::prelude::*;
use gtk::{Label, Grid, Button, Builder, Popover};
use std::ops::Rem;
use state::Message;
use uuid::Uuid;
use super::line::CommandLine;

pub struct CommandChooserController {
    grid: Grid,
    back_btn: Button,
    pop: Popover,
    cl: Rc<RefCell<CommandLine>>,
    pos: Vec<usize>,
    top: Vec<(&'static str, gkey::Key, GridNode)>
}

impl CommandChooserController {
    pub fn new(cl: Rc<RefCell<CommandLine>>, b: &Builder) -> Rc<RefCell<Self>> {
        let ret = Rc::new(RefCell::new(CommandChooserController {
            grid: b.get_object("cc-grid").unwrap(),
            back_btn: b.get_object("cc-end-button").unwrap(),
            pop: b.get_object("command-chooser-popover").unwrap(),
            pos: vec![],
            cl: cl,
            top: get_chooser_grid()
        }));
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
                for (i, &(_, key, _)) in selfish.get_ptr().iter().rev().enumerate() {
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
    pub fn execute(cl: Rc<RefCell<CommandLine>>, clone: bool) {
        {
            let cl = cl.borrow();
            if !cl.ready { return; }
            cl.tx.send(Message::Execute(cl.cd.as_ref().unwrap().uuid));
        }
    }
    fn get_ptr(&self) -> &Vec<(&'static str, gkey::Key, GridNode)> {
        let mut ptr = &self.top;
        if self.pos.len() > 0 {
            for i in &self.pos {
                if let Some(&(_, _, GridNode::Grid(ref vec))) = ptr.get(*i) {
                    ptr = vec;
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
        ptr
    }
    pub fn update(selfish_: Rc<RefCell<Self>>) {
        let selfish = selfish_.borrow();
        let ptr = selfish.get_ptr();
        for chld in selfish.grid.get_children() {
            chld.destroy();
        }
        for (i, &(st, _, ref opt)) in ptr.iter().enumerate() {
            let lbl = Label::new(None);
            let btn = Button::new();
            lbl.set_markup(st);
            btn.add(&lbl);
            match opt {
                &GridNode::Choice(spawner) => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        selfish_.borrow().pop.hide();
                        let uu = Uuid::new_v4();
                        cl.borrow().tx.send(Message::NewCmd(uu, spawner));
                        cl.borrow_mut().uuid = Some(uu);
                        CommandLine::update(cl.clone(), None);
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
                &GridNode::Clear => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        {
                            let mut cl = cl.borrow_mut();
                            if let Some(ref cd) = cl.cd {
                                cl.tx.send(Message::Delete(cd.uuid));
                            }
                            cl.cd = None;
                        }
                        CommandLine::update(cl.clone(), None);
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if cl.borrow().cd.is_some() {
                        lbl.get_style_context().unwrap().add_class("gridnode-clear");
                        btn.set_sensitive(true);
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                },
                &GridNode::Execute(clone) => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        Self::execute(cl.clone(), clone);
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if cl.borrow().ready {
                        btn.set_sensitive(true);
                        lbl.get_style_context().unwrap().add_class("gridnode-execute");
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                }
            }
            selfish.grid.attach(&btn, i.rem(3) as i32, (i/3) as i32, 1, 1);
        }
        selfish.grid.show_all();
    }
}

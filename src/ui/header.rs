//! The header object with the GO button & current position

use state::{ChainType, Message};
use gtk::prelude::*;
use gtk::{Builder, Button, Label};
use std::rc::Rc;
use std::cell::RefCell;
use uuid::Uuid;
use super::UISender;

pub struct HeaderController {
    pub go_btn: Button,
    go_lbl: Label,
    status_lbl: Label,
    pub cur: Option<ChainType>,
    sender: UISender
}
impl HeaderController {
    pub fn new(tx: UISender, b: &Builder) -> Rc<RefCell<Self>> {
        let go_btn: Button = b.get_object("go-button").unwrap();
        let lbl = Label::new(None);
        lbl.set_markup("<b>NO</b> <i>Ctrl-Space</i>");
        lbl.set_sensitive(false);
        lbl.get_style_context().unwrap().add_class("gridnode");
        go_btn.add(&lbl);
        go_btn.show_all();
        let ret = Rc::new(RefCell::new(HeaderController {
            go_btn: go_btn,
            go_lbl: lbl,
            status_lbl: b.get_object("current-selection-label").unwrap(),
            sender: tx,
            cur: None
        }));
        ret.borrow().go_btn.connect_clicked(clone!(ret; |_s| {
            Self::go(ret.clone());
        }));
        Self::update_sel(ret.clone(), None);
        ret
    }
    pub fn go(selfish: Rc<RefCell<Self>>) {
        let mut selfish = selfish.borrow_mut();
        if let Some(ct) = selfish.cur.clone() {
            selfish.sender.send(Message::UIGo(ct));
        }
    }
    pub fn update_sel(selfish: Rc<RefCell<Self>>, sel: Option<(ChainType, Uuid)>) {
        let mut selfish = selfish.borrow_mut();
        if let Some((ct, _)) = sel {
            if let ChainType::Unattached = ct {
                selfish.cur = None;
                selfish.status_lbl.set_markup("Unattached command selected <span fgcolor=\"#888888\">- select a command attached to a cue</span>");
                selfish.go_btn.set_sensitive(false);
                selfish.go_lbl.get_style_context().unwrap().remove_class("gridnode-execute");
                selfish.go_lbl.set_markup("<b>NO Q</b> <i>Ctrl-Space</i>");
            }
            else {
                selfish.status_lbl.set_markup(&format!("Current cue: <b>{}</b> <span fgcolor=\"#888888\">- press Ctrl+Space or the GO button to run</span>", ct));
                selfish.go_lbl.set_markup(&format!("<b>GO {}</b> <i>Ctrl-Space</i>", ct));
                selfish.cur = Some(ct);
                selfish.go_btn.set_sensitive(true);
                selfish.go_lbl.get_style_context().unwrap().add_class("gridnode-execute");
            }
        }
        else {
            selfish.cur = None;
            selfish.status_lbl.set_markup("No cue selected <span fgcolor=\"#888888\">- select a command below to select its cue</span>");
            selfish.go_btn.set_sensitive(false);
            selfish.go_lbl.get_style_context().unwrap().remove_class("gridnode-execute");
            selfish.go_lbl.set_markup("<b>NO Q</b> <i>Ctrl-Space</i>");
        }
    }
}

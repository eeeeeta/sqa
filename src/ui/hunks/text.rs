use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Label;
use gtk::Box as GtkBox;
use super::CommandLine;
use super::HunkUIController;

pub struct TextUIController {
    lbl: Label
}

impl TextUIController {
    pub fn new() -> Self {
        TextUIController {
            lbl: Label::new(None)
        }
    }
}

impl HunkUIController for TextUIController {
    fn bind(&mut self, _: Rc<RefCell<CommandLine>>, _: usize) {}
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.lbl, false, false, 3);
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>) {
        match val {
            Some(txt) => {
                self.lbl.set_markup(&format!("<span fgcolor=\"#888888\">{}</span>",txt.downcast_ref::<String>().unwrap()));
            },
            None => {
                self.lbl.set_markup("");
            }
        }
    }
    fn set_error(&mut self, _: Option<String>) {}
}

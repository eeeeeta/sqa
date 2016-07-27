use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Label;
use gtk::Box as GtkBox;
use command::HunkTypes;
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
    fn bind(&mut self, _: Rc<RefCell<CommandLine>>, _: usize, ht: HunkTypes) {}
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.lbl, false, false, 3);
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        self.lbl.set_markup(&format!("<span fgcolor=\"#888888\">{}</span>", val.downcast_ref::<String>().unwrap()));
    }
    fn set_error(&mut self, _: Option<String>) {}
}

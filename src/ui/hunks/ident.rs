use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Box as GtkBox;
use gtk::{EntryCompletion, TreeStore};
use command::HunkTypes;
use super::CommandLine;
use super::HunkUIController;
use super::EntryUIController;
use ui::INTERFACE_SRC;
pub struct IdentUIController {
    entuic: EntryUIController
}
impl IdentUIController {
    pub fn new() -> Self {
        IdentUIController {
            entuic: EntryUIController::new("edit-find")
        }
    }
}
impl HunkUIController for IdentUIController {
    fn focus(&self) {
        self.entuic.focus();
    }
    fn pack(&self, onto: &GtkBox) {
        self.entuic.pack(onto);
    }
    fn set_help(&mut self, help: &'static str) {
        self.entuic.set_help(help);
    }
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize, ht: HunkTypes) {
        self.entuic.bind(line.clone(), idx, ht.clone());
    }
    fn bind_completions(&mut self, compl: TreeStore) {
        let b = ::gtk::Builder::new_from_string(INTERFACE_SRC);
        let ic: EntryCompletion = b.get_object("identifier-completion").unwrap();
        ic.set_model(Some(&compl));
        let entc = self.entuic.ent.clone();
        ic.connect_match_selected(move |_, tm, ti| {
            entc.set_text(&tm.get_value(ti, 6).get::<String>().unwrap());
            entc.activate();
            Inhibit(true)
        });
        self.entuic.ent.set_completion(Some(&ic));
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        self.entuic.set_val(val);
    }
    fn set_error(&mut self, err: Option<String>) {
        self.entuic.set_error(err);
    }
}

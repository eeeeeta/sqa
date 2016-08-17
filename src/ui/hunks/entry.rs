use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Entry;
use gtk::Box as GtkBox;
use command::HunkTypes;
use super::CommandLine;
use super::PopoverUIController;
use super::HunkUIController;

pub struct EntryUIController {
    pub pop: Rc<RefCell<PopoverUIController>>,
    pub ent: Entry,
    pub activate_handler: Option<u64>
}

impl EntryUIController {
    pub fn new(icon: &'static str) -> Self {
        let uic = EntryUIController {
            pop: Rc::new(RefCell::new(PopoverUIController::new())),
            ent: Entry::new(),
            activate_handler: None
        };
        uic.pop.borrow().popover.set_relative_to(Some(&uic.ent));
        uic.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Primary, Some(icon));
        uic
    }
}

impl HunkUIController for EntryUIController {
    fn focus(&self) {
        self.ent.grab_focus();
    }
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.ent, false, false, 3);
    }
    fn set_help(&mut self, help: &'static str) {
        self.pop.borrow().set_help(help);
    }
    /* FIXME: more clone()s for seemingly ø reason */
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize, ht: HunkTypes) {
        let ref pop = self.pop;
        let entc = self.ent.clone();

        pop.borrow().bind_defaults(line.clone(), idx, ht.clone());
        self.ent.connect_focus_in_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.ent.connect_focus_out_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(false);
            entc.activate();
            Inhibit(false)
        }));
        self.activate_handler = Some(self.ent.connect_activate(move |selfish| {
            let txt = selfish.get_text().unwrap();
            let val = if txt == "" { None } else { Some(txt) };
            CommandLine::set_val(line.clone(), idx, ht.string_of(val));
        }));
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        let val = val.downcast_ref::<Option<String>>().unwrap();
        self.pop.borrow().val_exists(val.is_some());
        match val {
            &Some(ref txt) => {
                self.ent.set_text(txt);
            },
            &None => {
                self.ent.set_text("");
            }
        }
    }
    fn set_error(&mut self, err: Option<String>) {
        if err.is_some() {
            self.ent.get_style_context().unwrap().add_class("entry-error");
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-error"));
        }
        else {
            self.ent.get_style_context().unwrap().remove_class("entry-error");
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, None);
        }
        self.pop.borrow_mut().set_err(err);
    }
}

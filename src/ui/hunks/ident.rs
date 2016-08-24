use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Box as GtkBox;
use gtk::{EntryCompletion, ListStore};
use command::HunkTypes;
use super::CommandLine;
use super::HunkUIController;
use super::EntryUIController;
use ui::INTERFACE_SRC;
use uuid::Uuid;

pub struct IdentUIController {
    entuic: EntryUIController,
    err: Rc<RefCell<Option<String>>>
}
impl IdentUIController {
    pub fn new() -> Self {
        IdentUIController {
            entuic: EntryUIController::new("edit-find"),
            err: Rc::new(RefCell::new(None))
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
        let ref uierr = self.err;
        self.entuic.ent.connect_activate(clone!(line; |ent| {
            if let Some(strn) = ent.get_text() {
                if Uuid::parse_str(&strn).is_err() {
                    // FIXME: replace with try_borrow()
                    if let ::std::cell::BorrowState::Unused = line.borrow_state() {
                        let line = line.borrow();
                        let mut ti = match line.completion.iter_children(None) {
                            Some(v) => v,
                            None => return
                        };
                        loop {
                            if let Some(v) = line.completion.get_value(&ti, 0).get::<String>() {
                                if v == strn {
                                    ent.set_text(&line.completion.get_value(&ti, 1).get::<String>().unwrap());
                                    break;
                                }
                            }
                            if !line.completion.iter_next(&mut ti) {
                                break;
                            }
                        }
                    }
                }
            }
        }));
        self.entuic.ent.connect_activate(clone!(line, uierr; |ent| {
            *uierr.borrow_mut() = None;
            if let Some(strn) = ent.get_text() {
                if let Ok(val) = Uuid::parse_str(&strn) {
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Identifier(Some(val)));
                }
                else if strn == "" {
                    *uierr.borrow_mut() = None;
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Identifier(None));
                }
                else {
                    *uierr.borrow_mut() = Some(strn.to_owned());
                    CommandLine::update(line.clone(), None);
                }
            }
            else {
                CommandLine::set_val(line.clone(), idx, HunkTypes::Identifier(None));
            }
        }));
        self.entuic.bind(line.clone(), idx, ht.clone());
        ::glib::signal_handler_block(&self.entuic.ent, self.entuic.activate_handler.unwrap());
    }
    fn bind_completions(&mut self, compl: ListStore) {
        let b = ::gtk::Builder::new_from_string(INTERFACE_SRC);
        let ic: EntryCompletion = b.get_object("identifier-completion").unwrap();
        ic.set_model(Some(&compl));
        let entc = self.entuic.ent.clone();
        ic.connect_match_selected(move |_, tm, ti| {
            entc.set_text(&tm.get_value(ti, 1).get::<String>().unwrap());
            entc.activate();
            Inhibit(true)
        });
        self.entuic.ent.set_completion(Some(&ic));
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        if self.err.borrow().is_some() {
            return;
        }
        let val = val.downcast_ref::<Option<Uuid>>().unwrap();
        self.entuic.pop.borrow().val_exists(val.is_some());
        match val {
            &Some(ref txt) => {
                self.entuic.ent.set_text(&format!("{}", txt));
            },
            &None => {
                self.entuic.ent.set_text("");
            }
        }
    }
    fn set_error(&mut self, err: Option<String>) {
        self.entuic.set_error(err);
    }
    fn get_error(&self) -> Option<String> {
        if self.err.borrow().is_some() {
            Some(format!("Please enter a valid identifier."))
        }
        else {
            None
        }
    }
}

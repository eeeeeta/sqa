use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Box as GtkBox;
use command::HunkTypes;
use super::CommandLine;
use super::HunkUIController;
use super::EntryUIController;

pub struct TimeUIController {
    entuic: EntryUIController,
    err: Rc<RefCell<Option<String>>>
}
impl TimeUIController {
    pub fn new() -> Self {
        TimeUIController {
            entuic: EntryUIController::new("appointment-soon"),
            err: Rc::new(RefCell::new(None))
        }
    }
}
impl HunkUIController for TimeUIController {
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
        let ref pop = self.entuic.pop;
        let ref ent = self.entuic.ent;
        let ref uierr = self.err;

        pop.borrow().bind_defaults(line.clone(), idx, ht);
        self.entuic.ent.connect_focus_in_event(clone!(pop; |_s, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.entuic.ent.connect_focus_out_event(clone!(pop, uierr, line, ent; |_s, _y| {
            pop.borrow().visible(false);
            if let Some(strn) = ent.get_text() {
                if let Ok(_) = str::parse::<u64>(&strn) {
                    ent.activate();
                    return Inhibit(false);
                }
                else if strn == "" {
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Time(None));
                    *uierr.borrow_mut() = None;
                    return Inhibit(false);
                }
            }
            else {
                CommandLine::set_val(line.clone(), idx, HunkTypes::Time(None));
                *uierr.borrow_mut() = None;
                return Inhibit(false);
            }
            *uierr.borrow_mut() = Some(ent.get_text().unwrap().to_owned());
            CommandLine::update(line.clone(), None);
            Inhibit(false)
        }));
        self.entuic.ent.connect_activate(clone!(line, uierr; |ent| {
            *uierr.borrow_mut() = None;
            if let Some(strn) = ent.get_text() {
                if let Ok(ref val) = str::parse::<u64>(&strn) {
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Time(Some(*val)));
                }
                else if strn == "" {
                    *uierr.borrow_mut() = None;
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Time(None));
                }
                else {
                    *uierr.borrow_mut() = Some(strn.to_owned());
                    CommandLine::update(line.clone(), None);
                }
            }
            else {
                CommandLine::set_val(line.clone(), idx, HunkTypes::Time(None));
            }
        }));
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        if self.err.borrow().is_some() {
            return;
        }
        let val = val.downcast_ref::<Option<u64>>().unwrap();
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
            Some(format!("Please enter a valid whole number of milliseconds (or unset this value)."))
        }
        else {
            None
        }
    }
}

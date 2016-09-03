use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::CheckButton;
use gtk::Box as GtkBox;
use command::HunkTypes;
use super::CommandLine;
use super::PopoverUIController;
use super::HunkUIController;

pub struct CheckboxUIController {
    pop: Rc<RefCell<PopoverUIController>>,
    btn: CheckButton,
    handler: Option<u64>
}

impl CheckboxUIController {
    pub fn new() -> Self {
        let uic = CheckboxUIController {
            pop: Rc::new(RefCell::new(PopoverUIController::new())),
            btn: CheckButton::new(),
            handler: None
        };
        uic.pop.borrow().popover.set_relative_to(Some(&uic.btn));
        uic.pop.borrow().val_exists(false);
        uic
    }
}

impl HunkUIController for CheckboxUIController {
    fn focus(&self) {
        self.btn.grab_focus();
    }
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.btn, false, false, 3);
    }
    fn set_help(&mut self, help: &'static str) {
        self.pop.borrow().set_help(help);
    }
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize, _: HunkTypes) {
        let pop = &self.pop;

        self.btn.connect_focus_in_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.btn.connect_focus_out_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(false);
            Inhibit(false)
        }));
        self.handler = Some(self.btn.connect_toggled(move |selfish| {
            let state = selfish.get_active();
            CommandLine::set_val(line.clone(), idx, HunkTypes::Checkbox(state));
        }));
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        let val = val.downcast_ref::<bool>().unwrap();
        ::glib::signal_handler_block(&self.btn, self.handler.unwrap());
        self.btn.set_active(*val);
        ::glib::signal_handler_unblock(&self.btn, self.handler.unwrap());
    }
    fn set_error(&mut self, err: Option<String>) {
        self.pop.borrow_mut().set_err(err);
    }
}

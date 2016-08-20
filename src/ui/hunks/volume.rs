use streamv2::{lin_db, db_lin};
use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::Scale;
use gtk::Box as GtkBox;
use command::HunkTypes;
use super::CommandLine;
use super::HunkUIController;
use super::EntryUIController;

pub struct VolumeUIController {
    entuic: EntryUIController,
    sc: Scale,
    err: Rc<RefCell<bool>>,
    sc_handler: Option<u64>
}

impl VolumeUIController {
    pub fn new() -> Self {
        let ret = VolumeUIController {
            entuic: EntryUIController::new("volume-knob"),
            sc: Scale::new_with_range(::gtk::Orientation::Horizontal, 0.0, db_lin(3.0) as f64, 0.01),
            err: Rc::new(RefCell::new(false)),
            sc_handler: None
        };
        ret.sc.set_value(1.0);
        ret.sc.set_draw_value(false);
        ret.sc.set_can_focus(false);
        ret.sc.set_size_request(450, -1);
        ret.sc.set_digits(3);
        ret.sc.add_mark(0.0, ::gtk::PositionType::Bottom, Some("-∞"));

        ret.sc.add_mark(db_lin(-20.0) as f64, ::gtk::PositionType::Bottom, Some("-20"));
        ret.sc.add_mark(db_lin(-10.0) as f64, ::gtk::PositionType::Bottom, Some("-10"));
        ret.sc.add_mark(db_lin(-6.0) as f64, ::gtk::PositionType::Bottom, Some("-6"));
        ret.sc.add_mark(db_lin(-3.0) as f64, ::gtk::PositionType::Bottom, Some("-3"));
        ret.sc.add_mark(db_lin(-1.0) as f64, ::gtk::PositionType::Bottom, Some("-1"));
        ret.sc.add_mark(db_lin(0.0) as f64, ::gtk::PositionType::Bottom, Some("<b>0dB</b>"));
        ret.sc.add_mark(db_lin(1.0) as f64, ::gtk::PositionType::Bottom, Some("1"));
        ret.sc.add_mark(db_lin(2.0) as f64, ::gtk::PositionType::Bottom, Some("2"));
        ret.sc.add_mark(db_lin(3.0) as f64, ::gtk::PositionType::Bottom, Some("3"));
        ret.entuic.ent.set_width_chars(-1);
        ret
    }
}

impl HunkUIController for VolumeUIController {
    fn focus(&self) {
        self.entuic.focus();
    }
    fn pack(&self, onto: &GtkBox) {
        self.entuic.pack(onto);
        let ref sa = self.entuic.pop.borrow().state_actions;
        sa.pack_start(&self.sc, false, false, 3);
    }
    fn set_help(&mut self, help: &'static str) {
        self.entuic.set_help(help);
    }
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize, ht: HunkTypes) {
        let ref uierr = self.err;
        self.entuic.ent.connect_key_release_event(move |ent, _| {
            ent.activate();
            Inhibit(false)
        });
        self.entuic.ent.connect_activate(clone!(line, uierr; |ent| {
            if let Some(strn) = ent.get_text() {
                if let Ok(flt) = str::parse::<f32>(&strn) {
                    *uierr.borrow_mut() = false;
                    CommandLine::set_val(line.clone(), idx, HunkTypes::Volume(flt));
                }
                else {
                    *uierr.borrow_mut() = true;
                }
            }
        }));
        self.sc_handler = Some(self.sc.connect_value_changed(clone!(uierr, line; |sc| {
            let mut val = sc.get_value() as f32; /* stupid macro */
            if val < 0.0002 {
                val = ::std::f32::NEG_INFINITY;
            }
            else {
                val = lin_db(val);
            }
            *uierr.borrow_mut() = false;
            CommandLine::set_val(line.clone(), idx, HunkTypes::Volume(val));
        })));
        self.entuic.bind(line.clone(), idx, ht.clone());
        for ch in self.entuic.pop.borrow().state_actions.get_children().into_iter() {
            ch.destroy();
        }
        ::glib::signal_handler_block(&self.entuic.ent, self.entuic.activate_handler.unwrap());
    }
    fn set_val(&mut self, val: &::std::any::Any) {
        let val = *val.downcast_ref::<f32>().unwrap();
        ::glib::signal_handler_block(&self.sc, self.sc_handler.unwrap());
        self.sc.set_value(db_lin(val) as f64);
        ::glib::signal_handler_unblock(&self.sc, self.sc_handler.unwrap());
        if {
            if let Some(strn) = self.entuic.ent.get_text() {
                if let Ok(flt) = str::parse::<f32>(&strn) {
                    flt != val
                }
                else {
                    true
                }
            }
            else { true }
        } {
            self.entuic.ent.set_text(&format!("{:.02}", val));
        }

    }
    fn set_error(&mut self, err: Option<String>) {
        self.entuic.set_error(err);
    }
    fn get_error(&self) -> Option<String> {
        if *self.err.borrow() {
            Some(format!("Please enter or select a valid decibel value."))
        }
        else {
            None
        }
    }
}

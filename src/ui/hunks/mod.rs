mod entry;
mod text;
mod time;
mod volume;
mod ident;
mod checkbox;

pub use self::entry::EntryUIController;
pub use self::text::TextUIController;
pub use self::time::TimeUIController;
pub use self::volume::VolumeUIController;
pub use self::ident::IdentUIController;
pub use self::checkbox::CheckboxUIController;
pub use super::line::CommandLine;

use command::HunkTypes;
use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::{Label, Image, Button, Builder, Popover, ListStore};
use gtk::Box as GtkBox;

pub trait HunkUIController {
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize, ht: HunkTypes);
    fn bind_completions(&mut self, compl: ListStore) {}
    fn focus(&self) {}
    fn pack(&self, onto: &GtkBox);
    fn set_help(&mut self, _help: &'static str) {}
    fn set_val(&mut self, _val: &::std::any::Any) {}
    fn set_accel(&mut self, _accel: Option<::gdk::enums::key::Key>) {}
    fn set_error(&mut self, _err: Option<String>) {}
    fn get_error(&self) -> Option<String> { None }
}

pub struct PopoverUIController {
    pub popover: Popover,
    pub state_lbl: Label,
    pub state_actions: GtkBox,
    pub err_box: GtkBox,
    pub err_lbl: Label,
    pub err_vis: bool,
    pub unset_btn: Button
}

impl PopoverUIController {
    pub fn new() -> Self {
        let hunk_glade = include_str!("hunk.glade");
        let bldr = Builder::new_from_string(hunk_glade);
        let uic = PopoverUIController {
            popover: bldr.get_object("hunk-popover").unwrap(),
            state_actions: bldr.get_object("hunk-state-actions").unwrap(),
            state_lbl: bldr.get_object("hunk-state-label").unwrap(),
            err_box: bldr.get_object("hunk-error-box").unwrap(),
            err_lbl: bldr.get_object("hunk-error-label").unwrap(),
            unset_btn: Self::build_btn("Unset", "dialog-cancel"),
            err_vis: false
        };
        uic.err_box.hide();
        uic
    }
    pub fn visible(&self, vis: bool) {
        if vis {
            self.popover.show_all();
        }
        else {
            self.popover.hide();
        }
        self.err_box.set_visible(self.err_vis);
    }
    pub fn set_help(&self, hlp: &'static str) {
        self.state_lbl.set_text(hlp);
    }
    pub fn val_exists(&self, exists: bool) {
        self.unset_btn.set_sensitive(exists);
    }
    pub fn set_err(&mut self, err: Option<String>) {
        if let Some(e) = err {
            self.err_vis = true;
            self.err_box.show_all();
            self.err_lbl.set_text(&e);
        }
        else {
            self.err_vis = false;
            self.err_box.hide();
        }
    }
    pub fn build_btn(label: &'static str, icon: &'static str) -> Button {
        let btn = Button::new();
        btn.set_always_show_image(true);
        btn.set_can_focus(false);
        btn.set_sensitive(false);
        btn.set_image(&Image::new_from_icon_name(icon, 1));
        btn.set_label(label);
        btn
    }
    /* FIXME: why does the rust compiler make us clone() here? */
    pub fn bind_defaults(&self, line: Rc<RefCell<CommandLine>>, idx: usize, ht: HunkTypes) {
        self.unset_btn.connect_clicked(clone!(line; |_s| {
            CommandLine::set_val(line.clone(), idx, ht.none_of());
        }));
        self.state_actions.pack_start(&self.unset_btn, false, false, 0);
    }
}

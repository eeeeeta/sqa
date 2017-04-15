use gtk::prelude::*;
use gtk::{Entry, Popover, Image, Label, Builder, self};
use gdk::RGBA;
use std::ops::Deref;

pub struct FallibleEntry {
    popover: Popover,
    entry: Entry,
    image: Image,
    label: Label
}
impl FallibleEntry {
    pub fn new(b: &Builder) -> Self {
        let entry = Entry::new();
        let ret = build!(FallibleEntry using b
                         with entry
                         get popover, image, label);
        let pop = ret.popover.clone();
        ret.entry.connect_changed(move |slf| {
            slf.override_color(gtk::STATE_FLAG_NORMAL, None);
            pop.hide();
        });
        ret.popover.set_relative_to(Some(&ret.entry));
        ret
    }
    pub fn throw_error(&mut self, e: String) {
        self.entry.override_color(gtk::STATE_FLAG_NORMAL, Some(&RGBA::red()));
        self.label.set_text(&e);
        self.popover.show_all();
    }
    pub fn on_enter<F: Fn(&Entry) + 'static>(&self, func: F) {
        self.entry.connect_activate(func);
    }
    pub fn get_text(&self) -> String {
        self.entry.get_text().unwrap_or("".into())
    }
}
impl Deref for FallibleEntry {
    type Target = Entry;

    fn deref(&self) -> &Entry {
        &self.entry
    }
}

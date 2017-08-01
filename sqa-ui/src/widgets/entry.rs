use gtk::prelude::*;
use gtk::{Entry, Popover, Image, Label, Builder, self};
use gdk::RGBA;
use std::rc::Rc;
use std::ops::Deref;
use util;

#[derive(Clone)]
pub struct FallibleEntry {
    popover: Popover,
    entry: Entry,
    label: Label
}
impl FallibleEntry {
    pub fn new() -> Self {
        let b = Builder::new_from_string(util::INTERFACE_SRC);
        let entry = Entry::new();
        let ret = build!(FallibleEntry using b
                         with entry
                         get popover, label);
        let pop = ret.popover.clone();
        ret.entry.connect_changed(clone!(pop; |slf| {
            slf.override_color(gtk::STATE_FLAG_NORMAL, None);
            slf.get_style_context().unwrap().remove_class("fe-error");
            pop.hide();
        }));
        ret.entry.connect_activate(move |slf| {
            if slf.get_style_context().unwrap().list_classes().contains(&"fe-error".into()) {
                pop.show_all();
            }
        });
        ret.popover.set_relative_to(Some(&ret.entry));
        ret
    }
    pub fn reset_error(&self) {
        self.entry.override_color(gtk::STATE_FLAG_NORMAL, None);
        self.get_style_context().unwrap().remove_class("fe-error");
        self.popover.hide();
    }
    pub fn throw_error(&self, e: &str) {
        self.entry.override_color(gtk::STATE_FLAG_NORMAL, Some(&RGBA::red()));
        self.get_style_context().unwrap().add_class("fe-error");
        self.label.set_text(e);
        self.popover.show_all();
    }
    pub fn on_text_updated<F: Fn(&FallibleEntry, Option<String>, bool) + 'static>(&self, func: F) {
        let slf = self.clone();
        let func = Rc::new(func);
        self.entry.connect_activate(clone!(func, slf; |ent| {
            let mut txt = ent.get_text();
            if txt.is_some() {
                if txt.as_ref().unwrap() == "" {
                    txt = None;
                }
            }
            func(&slf, txt, true);
        }));
        self.entry.connect_focus_out_event(move |ent, _| {
            let mut txt = ent.get_text();
            if txt.is_some() {
                if txt.as_ref().unwrap() == "" {
                    txt = None;
                }
            }
            func(&slf, txt, false);
            Inhibit(false)
        });
    }
    pub fn on_enter<F: Fn(&Entry) + 'static>(&self, func: F) {
        self.entry.connect_activate(func);
    }
    pub fn get_text(&self) -> String {
        self.entry.get_text().unwrap_or("".into())
    }
    pub fn set_text(&self, text: &str) {
        self.entry.set_text(text);
    }
}
impl Deref for FallibleEntry {
    type Target = Entry;

    fn deref(&self) -> &Entry {
        &self.entry
    }
}

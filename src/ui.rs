//! Functions for managing the frontend UI
use command::{Command, Hunk, HunkTypes, CommandState};
use state::ReadableContext;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Label, Image, Entry, Button, Builder, Popover};
use gtk::Box as GtkBox;


#[derive(Clone)]
struct HintPopover {
    pop: Popover,
    si: Image,
    sl: Label,
    sa: GtkBox
}
macro_rules! clone {
    ($($n:ident),+; || $body:block) => (
        {
            $( let $n = $n.clone(); )+
            move || { $body }
        }
    );
    ($($n:ident),+; |$($p:ident),+| $body:block) => (
        {
            $( let $n = $n.clone(); )+
            move |$($p),+| { $body }
        }
    );
}
struct CommandContext {
    cmd: Option<Rc<RefCell<Box<Command>>>>,
    ctx: Rc<RefCell<ReadableContext>>,
    blocked: u16,
    h_image: Image,
    h_label: Label,
}
impl CommandContext {
    fn block(&mut self) {
        self.blocked += 1;
    }
    fn unblock(&mut self) {
        self.blocked -= 1;
    }
    fn update(&self) {
        if self.cmd.is_none() {
            self.h_label.set_text("Command line idle.");
            self.h_image.set_from_icon_name("dialog-question", 1);
        }
        else if self.blocked > 0 {
            self.h_label.set_text("Command contains errors - please amend or revert");
            self.h_image.set_from_icon_name("dialog-error", 1);
        }
        else {
            let state = self.cmd.as_ref().unwrap().borrow().get_state(&self.ctx.borrow());
            self.h_label.set_text(&state.message);
            if state.complete {
                self.h_image.set_from_icon_name("dialog-ok", 1);
            }
            else {
                self.h_image.set_from_icon_name("dialog-warning", 1);
            }
        }
    }
}
pub struct CommandLine {
    hunks: Vec<Rc<RefCell<Box<Hunk>>>>,
    ctx: Rc<RefCell<CommandContext>>,
    line: GtkBox
}
impl CommandLine {
    pub fn new(ctx: Rc<RefCell<ReadableContext>>, b: Builder) -> Self {
        let line = CommandLine {
            hunks: Vec::new(),
            ctx: Rc::new(RefCell::new(CommandContext {
                cmd: None,
                ctx: ctx,
                blocked: 0,
                h_image: b.get_object("line-hint-image").unwrap(),
                h_label: b.get_object("line-hint-label").unwrap(),
            })),
            line: b.get_object("command-line").unwrap()
        };
        line.clear(true);
        line
    }
    fn clear(&self, hlpm: bool) {
        for wdgt in self.line.get_children().into_iter() {
            wdgt.destroy();
        }
        if hlpm {
            let label = Label::new(None);
            label.set_markup("<span fgcolor=\"#888888\"><i>Select a command with Ctrl+Enter</i></span>");
            self.line.pack_start(&label, false, false, 0);
        }
        self.ctx.borrow().update();
    }
    pub fn set_cmd(&mut self, cmd: Box<Command>) {
        let mut ctx = self.ctx.borrow_mut();
        ctx.cmd = Some(Rc::new(RefCell::new(cmd)));
        self.hunks = Vec::new();
        let cmd = ctx.cmd.as_ref().unwrap().borrow();
        for mut hnk in cmd.get_hunks().into_iter() {
            hnk.assoc(ctx.cmd.as_ref().unwrap().clone());
            self.hunks.push(Rc::new(RefCell::new(hnk)));
        }
    }
    fn build_entry(hnk: &Box<Hunk>, icon: &'static str) -> Entry {
        let entry = get_str_and!(hnk, |st: Option<&str>| {
            if st.is_some() { Entry::new_with_buffer(&::gtk::EntryBuffer::new(st)) }
            else { Entry::new() }
        });
        entry.set_icon_from_icon_name(::gtk::EntryIconPosition::Primary, Some(icon));
        entry
    }
    fn build_pop_btn(label: &'static str, icon: &'static str) -> Button {
        let btn = Button::new();
        btn.set_always_show_image(true);
        btn.set_can_focus(false);
        btn.set_sensitive(false);
        btn.set_image(&Image::new_from_icon_name(icon, 1));
        btn.set_label(label);
        btn
    }
    /* this function is a monolith of terrible hackiness & haphazard state
     * please, someone build a state machine
     * also the borrows are *terrible* */
    fn connect_entry(hnk: Rc<RefCell<Box<Hunk>>>, pop: HintPopover, ctx: Rc<RefCell<CommandContext>>, entry: &Entry) {
        let blocked = Rc::new(RefCell::new(false));
        let unset = Self::build_pop_btn("Unset", "dialog-cancel");
        pop.sa.pack_start(&unset, false, false, 0);

        let revert = Self::build_pop_btn("Revert", "edit-undo");
        revert.connect_clicked(clone!(hnk, pop, ctx, entry, blocked, unset; |selfish| {
            get_str_and!(hnk.borrow(), |st: Option<&str>| {
                let mut ctx = ctx.borrow_mut();
                if st.is_some() { unset.set_sensitive(true); }
                else { unset.set_sensitive(false); }
                entry.set_text(st.unwrap_or(""));
                pop.sl.set_text(hnk.borrow().help());
                pop.si.set_from_icon_name("dialog-information", 1);
                selfish.set_sensitive(false);
                let mut blocked = blocked.borrow_mut();
                if *blocked {
                    *blocked = false;
                    ctx.unblock();
                }
                entry.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, None);
                ctx.update();
            });
        }));
        pop.sa.pack_start(&revert, false, false, 0);

        unset.connect_clicked(clone!(hnk, ctx, revert; |_x| {
            {
                let ctx = ctx.borrow();
                hnk.borrow_mut().set_val(::std::ops::Deref::deref(&ctx.ctx.borrow()), None).unwrap();
            }
            revert.clicked();
        }));
        entry.connect_key_press_event(clone!(revert; |_selfish, ev| {
            match ev.get_keyval() {
                ::gdk::enums::key::F1 => unset.clicked(),
                ::gdk::enums::key::F2 => revert.clicked(),
                _ => {}
            };
            Inhibit(false)
        }));
        entry.connect_focus_in_event(clone!(pop, revert, entry; |_x, _y| {
            if entry.get_text().as_ref().map(|x| x as &str).unwrap_or("") == "" {
                revert.clicked();
            }
            pop.pop.show_all();
            Inhibit(false)
        }));
        entry.connect_focus_out_event(clone!(pop; |selfish, _y| {
            selfish.activate();
            pop.pop.hide();
            Inhibit(false)
        }));
        entry.connect_activate(move |ent| {
            let txt = ent.get_text().unwrap();
            let val: Option<Box<::std::any::Any>> = if txt == "" { None } else { Some(Box::new(txt)) };
            let ret;
            {
                let ctx = ctx.borrow();
                ret = hnk.borrow_mut().set_val(::std::ops::Deref::deref(&ctx.ctx.borrow()), val);
            }
            match ret {
                Ok(()) => {
                    revert.clicked();
                    pop.sl.set_text("Value successfully modified.");
                    pop.si.set_from_icon_name("dialog-ok", 1);
                },
                Err(st) => {
                    pop.sl.set_text(&st);
                    pop.si.set_from_icon_name("dialog-error", 1);
                    revert.set_sensitive(true);
                    ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-error"));
                    let mut ctx = ctx.borrow_mut();
                    let mut blocked = blocked.borrow_mut();
                    if !*blocked {
                        *blocked = true;
                        ctx.block();
                    }
                    ctx.update();
                }
            }
        });
    }
    fn get_hint_popover(hnk: &Box<Hunk>) -> HintPopover {
        let hunk_glade = include_str!("hunk.glade");
        let bldr = Builder::new_from_string(hunk_glade);

        let pop = HintPopover {
            pop: bldr.get_object("hunk-popover").unwrap(),
            si: bldr.get_object("hunk-state-image").unwrap(),
            sa: bldr.get_object("hunk-state-actions").unwrap(),
            sl: bldr.get_object("hunk-state-label").unwrap()
        };
        pop.sl.set_text(hnk.help());
        pop
    }
    pub fn build(&self) {
        let ctx = self.ctx.borrow();
        if ctx.cmd.is_none() {
            self.clear(true);
            return;
        }
        self.clear(false);
        for hunk in self.hunks.iter() {
            let hnk = hunk.borrow();
            match hnk.disp() {
                HunkTypes::FilePath => {
                    let entry = Self::build_entry(&hnk, "document-open");
                    let hp = Self::get_hint_popover(&hnk);
                    hp.pop.set_relative_to(Some(&entry));
                    Self::connect_entry(hunk.clone(), hp.clone(), self.ctx.clone(), &entry);
                    self.line.pack_start(&entry, false, false, 3);
                },
                HunkTypes::Identifier => {
                    let entry = Self::build_entry(&hnk, "edit-find");
                    let hp = Self::get_hint_popover(&hnk);
                    hp.pop.set_relative_to(Some(&entry));
                    Self::connect_entry(hunk.clone(), hp.clone(), self.ctx.clone(), &entry);
                    self.line.pack_start(&entry, false, false, 3);
                },
                HunkTypes::String => {
                    let entry = Self::build_entry(&hnk, "text-x-generic");
                    let hp = Self::get_hint_popover(&hnk);
                    hp.pop.set_relative_to(Some(&entry));
                    Self::connect_entry(hunk.clone(), hp.clone(), self.ctx.clone(), &entry);
                    self.line.pack_start(&entry, false, false, 3);
                },
                HunkTypes::Label => {
                    let label = get_str_and!(hnk, |st: Option<&str>| {
                        if st.is_some() {
                            let label = Label::new(None);
                            label.set_markup(st.unwrap()); /* allow use of Pango */
                            label
                        }
                        else { panic!("HunkTypes::Label's getter failed") }
                    });
                    self.line.pack_start(&label, false, false, 3);
                },
                _ => {}
            }
        }
    }
}

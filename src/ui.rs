//! Functions for managing the frontend UI
use command::{Command, Hunk, HunkTypes};
use state::ReadableContext;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Label, Image, Entry, Button, Builder, Popover};
use gtk::Box as GtkBox;

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

pub enum HunkFSM {
    ErrStored,
    Err,
    Ok
}
struct PopoverUIController {
    popover: Popover,
    state_lbl: Label,
    state_actions: GtkBox,
    err_box: GtkBox,
    err_lbl: Label,
    unset_btn: Button,
    revert_btn: Button
}
struct EntryUIController {
    pop: Rc<RefCell<PopoverUIController>>,
    ent: Entry
}
struct TextUIController {
    lbl: Label
}
pub trait HunkUIController {
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize);
    fn pack(&self, onto: &GtkBox);
    fn set_help(&mut self, _help: &'static str) {}
    fn set_val(&mut self, _val: Option<&Box<::std::any::Any>>, stored: bool) {}
    fn error(&mut self, _err: Option<String>, _stored: bool) {}
}
pub struct HunkUI {
    hnk: Box<Hunk>,
    ctl: Box<HunkUIController>,
    state: HunkFSM
}
pub struct CommandLine {
    ctx: Rc<RefCell<ReadableContext>>,
    cmd: Option<Rc<RefCell<Box<Command>>>>,
    hunks: Vec<HunkUI>,
    ready: bool,
    line: GtkBox,
    h_image: Image,
    h_label: Label
}
impl PopoverUIController {
    fn new() -> Self {
        let hunk_glade = include_str!("hunk.glade");
        let bldr = Builder::new_from_string(hunk_glade);
        let uic = PopoverUIController {
            popover: bldr.get_object("hunk-popover").unwrap(),
            state_actions: bldr.get_object("hunk-state-actions").unwrap(),
            state_lbl: bldr.get_object("hunk-state-label").unwrap(),
            err_box: bldr.get_object("hunk-error-box").unwrap(),
            err_lbl: bldr.get_object("hunk-error-label").unwrap(),
            unset_btn: Self::build_btn("Unset", "dialog-cancel"),
            revert_btn: Self::build_btn("Revert", "edit-undo")
        };
        uic.err_box.hide();
        uic
    }
    fn visible(&self, vis: bool) {
        if vis {
            self.popover.show_all();
        }
        else {
            self.popover.hide();
        }
    }
    fn set_help(&self, hlp: &'static str) {
        self.state_lbl.set_text(hlp);
    }
    fn val_exists(&self, exists: bool) {
        self.unset_btn.set_sensitive(exists);
    }
    fn set_err(&self, err: Option<String>, stored: bool) {
        self.revert_btn.set_sensitive(stored);
        if let Some(e) = err {
            self.err_box.show_all();
            self.err_lbl.set_text(&e);
        }
        else {
            self.err_box.hide();
        }
    }
    fn build_btn(label: &'static str, icon: &'static str) -> Button {
        let btn = Button::new();
        btn.set_always_show_image(true);
        btn.set_can_focus(false);
        btn.set_sensitive(false);
        btn.set_image(&Image::new_from_icon_name(icon, 1));
        btn.set_label(label);
        btn
    }
    /* FIXME: why does the rust compiler make us clone() here? */
    fn bind_defaults(&self, line: Rc<RefCell<CommandLine>>, idx: usize) {
        self.unset_btn.connect_clicked(clone!(line; |_s| {
            CommandLine::set_val(line.clone(), idx, None);
        }));
        self.revert_btn.connect_clicked(move |_| {
            {
                let mut line = line.borrow_mut();
                line.hunks[idx].clear();
            }
            CommandLine::update(line.clone());
        });
        self.state_actions.pack_start(&self.unset_btn, false, false, 0);
        self.state_actions.pack_start(&self.revert_btn, false, false, 0);
    }
}
impl TextUIController {
    fn new() -> Self {
        TextUIController {
            lbl: Label::new(None)
        }
    }
}
impl HunkUIController for TextUIController {
    fn bind(&mut self, _: Rc<RefCell<CommandLine>>, _: usize) {}
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.lbl, false, false, 3);
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>, stored: bool) {
        match val {
            Some(txt) => {
                self.lbl.set_markup(&txt.downcast_ref::<String>().unwrap());
            },
            None => {
                self.lbl.set_markup("");
            }
        }
    }
    fn error(&mut self, _: Option<String>, _: bool) {}
}
impl EntryUIController {
    fn new(icon: &'static str) -> Self {
        let uic = EntryUIController {
            pop: Rc::new(RefCell::new(PopoverUIController::new())),
            ent: Entry::new()
        };
        uic.pop.borrow().popover.set_relative_to(Some(&uic.ent));
        uic.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Primary, Some(icon));
        uic
    }
}
impl HunkUIController for EntryUIController {
    fn pack(&self, onto: &GtkBox) {
        onto.pack_start(&self.ent, false, false, 3);
    }
    fn set_help(&mut self, help: &'static str) {
        self.pop.borrow().set_help(help);
    }
    /* FIXME: more clone()s for seemingly ø reason */
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize) {
        let ref pop = self.pop;
        let entc = self.ent.clone();

        pop.borrow().bind_defaults(line.clone(), idx);
        self.ent.connect_focus_in_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.ent.connect_focus_out_event(clone!(pop; |_x, _y| {
            pop.borrow().visible(false);
            entc.activate();
            Inhibit(false)
        }));
        self.ent.connect_activate(move |selfish| {
            let txt = selfish.get_text().unwrap();
            let val: Option<Box<::std::any::Any>> = if txt == "" { None } else { Some(Box::new(txt)) };
            CommandLine::set_val(line.clone(), idx, val);
        });
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>, stored: bool) {
        if !stored {
            self.pop.borrow().val_exists(val.is_some());
        }
        match val {
            Some(txt) => {
                self.ent.set_text(&txt.downcast_ref::<String>().unwrap());
            },
            None => {
                self.ent.set_text("");
            }
        }
    }
    fn error(&mut self, err: Option<String>, stored: bool) {
        if stored {
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-cancel"));
        }
        else if err.is_some() {
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-error"));
        }
        else {
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, None);
        }
        self.pop.borrow().set_err(err, stored);
    }
}
impl HunkUI {
    fn from_hunk(hnk: Box<Hunk>) -> Self {
        let ctl: Box<HunkUIController> = match hnk.disp() {
            HunkTypes::FilePath => Box::new(EntryUIController::new("document-open")),
            HunkTypes::Identifier => Box::new(EntryUIController::new("edit-find")),
            HunkTypes::String => Box::new(EntryUIController::new("text-x-generic")),
            HunkTypes::Label => Box::new(TextUIController::new()),
            _ => unimplemented!()
        };
        HunkUI {
            hnk: hnk,
            ctl: ctl,
            state: HunkFSM::Err
        }
    }
    fn update(&mut self, cmd: &Box<Command>, ctx: &ReadableContext) {
        let state = self.hnk.get_val(cmd, ctx);
        if state.stored.is_some() {
            self.state = HunkFSM::ErrStored;
            self.ctl.set_val(state.stored, true);
            self.ctl.error(state.err, true);
        }
        else if state.err.is_some() {
            self.state = HunkFSM::Err;
            self.ctl.error(state.err, false);
        }
        else if state.val.is_none() && state.required {
            self.state = HunkFSM::Err;
            self.ctl.error(Some(format!("This field is required, but contains nothing.")), false);
        }
        else {
            self.state = HunkFSM::Ok;
            self.ctl.error(None, false);
        }
        if state.stored.is_none() {
            self.ctl.set_val(state.val.as_ref(), false);
        }
        self.ctl.set_help(state.help);
    }
    fn set_val(&mut self, cmd: &mut Box<Command>, ctx: &ReadableContext, val: Option<Box<::std::any::Any>>) {
        /* gee, this function was really hard to code */
        self.hnk.set_val(cmd, ctx, val);
    }
    fn clear(&mut self) {
        /* this one took a day */
        self.hnk.clear()
    }
}
impl CommandLine {
    pub fn new(ctx: Rc<RefCell<ReadableContext>>, b: Builder) -> Rc<RefCell<Self>> {
        let line = CommandLine {
            ctx: ctx,
            cmd: None,
            hunks: Vec::new(),
            ready: false,
            line: b.get_object("command-line").unwrap(),
            h_image: b.get_object("line-hint-image").unwrap(),
            h_label: b.get_object("line-hint-label").unwrap()
        };
        Rc::new(RefCell::new(line))
    }
    pub fn build(selfish2: Rc<RefCell<Self>>, cmd: Box<Command>) {
        {
            let mut selfish = selfish2.borrow_mut();
            selfish.clear();
            selfish.cmd = Some(Rc::new(RefCell::new(cmd)));
            let mut hunks = Vec::new();
            for (i, hunk) in selfish.cmd.as_ref().unwrap().borrow().get_hunks().into_iter().enumerate() {
                let mut hui = HunkUI::from_hunk(hunk);
                hui.ctl.pack(&selfish.line);
                hui.ctl.bind(selfish2.clone(), i);
                hunks.push(hui);
            }
            selfish.hunks = hunks;
        }
        Self::update(selfish2);
    }
    fn set_val(selfish: Rc<RefCell<Self>>, idx: usize, val: Option<Box<::std::any::Any>>) {
        {
            let mut selfish = selfish.borrow_mut();
            let ctx = selfish.ctx.clone();
            let ctx = ctx.borrow();
            let cmd = selfish.cmd.as_ref().unwrap().clone();
            let mut cmd = cmd.borrow_mut();
            selfish.hunks[idx].set_val(&mut cmd, &ctx, val);
        }
        Self::update(selfish);
    }
    fn update(selfish: Rc<RefCell<Self>>) {
        let mut selfish = selfish.borrow_mut();
        if selfish.cmd.is_none() {
            selfish.h_image.set_from_icon_name("dialog-question", 1);
            selfish.h_label.set_text("Command line idle.");
            selfish.clear();
            let label = Label::new(None);
            label.set_markup("<span fgcolor=\"#888888\"><i>Select a command with Ctrl+Enter</i></span>");
            selfish.line.pack_start(&label, false, false, 0);
            return;
        }
        assert!(selfish.hunks.len() > 0);
        let (mut erred, mut stored, mut ok) = (0, 0, 0);
        let ctx = selfish.ctx.clone();
        let ctx = ctx.borrow();
        let cmd = selfish.cmd.as_ref().unwrap().clone();
        let cmd = cmd.borrow();
        for hunk in &mut selfish.hunks {
            hunk.update(&cmd, &ctx);
            match hunk.state {
                HunkFSM::ErrStored => {
                    erred += 1;
                    stored += 1;
                },
                HunkFSM::Err => erred += 1,
                HunkFSM::Ok => ok += 1
            }
        }
        if erred > 1 {
            selfish.ready = false;
            selfish.h_image.set_from_icon_name("dialog-error", 1);
            selfish.h_label.set_text(&format!("Command contains {} errors ({} stored) - please adjust marked elements", erred, stored));
        }
        else if ok < selfish.hunks.len() {
            selfish.ready = false;
            selfish.h_image.set_from_icon_name("dialog-warning", 1);
            selfish.h_label.set_text("Command incomplete - please adjust marked elements");
        }
        else {
            selfish.ready = true;
            selfish.h_image.set_from_icon_name("dialog-ok", 1);
            selfish.h_label.set_text("To execute the command, press Enter.");
        }
    }
    fn clear(&mut self) {
        for wdgt in self.line.get_children().into_iter() {
            wdgt.destroy();
        }
        self.hunks = Vec::new();
        self.ready = false;
    }
}

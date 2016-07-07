use command::{Command, Hunk, HunkTypes};
use state::ReadableContext;
use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::{Label, Image, Builder};
use gtk::Box as GtkBox;
use std::sync::{Arc, Mutex};
use backend::BackendSender;
use super::hunks::HunkUIController;
use super::hunks::EntryUIController;
use super::hunks::TextUIController;
use super::hunks::VolumeUIController;
use super::hunks::TimeUIController;

pub enum HunkFSM {
    Err,
    UIErr,
    Ok
}

pub struct HunkUI {
    hnk: Box<Hunk>,
    ctl: Box<HunkUIController>,
    state: HunkFSM
}

pub struct CommandLine {
    pub ctx: Arc<Mutex<ReadableContext>>,
    pub tx: BackendSender,
    pub cmd: Option<Rc<RefCell<Box<Command>>>>,
    pub hunks: Vec<HunkUI>,
    pub ready: bool,
    pub line: GtkBox,
    pub h_image: Image,
    pub h_label: Label,
}
impl HunkUI {
    pub fn from_hunk(hnk: Box<Hunk>) -> Self {
        let ctl: Box<HunkUIController> = match hnk.disp() {
            HunkTypes::FilePath => Box::new(EntryUIController::new("document-open")),
            HunkTypes::Identifier => Box::new(EntryUIController::new("edit-find")),
            HunkTypes::String => Box::new(EntryUIController::new("text-x-generic")),
            HunkTypes::Label => Box::new(TextUIController::new()),
            HunkTypes::Volume => Box::new(VolumeUIController::new()),
            HunkTypes::Time => Box::new(TimeUIController::new())
        };
        HunkUI {
            hnk: hnk,
            ctl: ctl,
            state: HunkFSM::Err
        }
    }
    pub fn update(&mut self, cmd: &Box<Command>, ctx: &ReadableContext) {
        let state = self.hnk.get_val(cmd, ctx);
        let uierr = self.ctl.get_error();
        if state.err.is_some() {
            self.state = HunkFSM::Err;
            self.ctl.set_error(state.err);
        }
        else if uierr.is_some() {
            self.state = HunkFSM::UIErr;
            self.ctl.set_error(uierr);
        }
        else if state.val.is_none() && state.required {
            self.state = HunkFSM::Err;
            self.ctl.set_error(Some(format!("This field is required, but contains nothing.")));
        }
        else {
            self.state = HunkFSM::Ok;
            self.ctl.set_error(None);
        }
        self.ctl.set_val(state.val.as_ref());
        self.ctl.set_help(state.help);
    }
    pub fn set_val(&mut self, cmd: &mut Box<Command>, val: Option<Box<::std::any::Any>>) {
        /* gee, this function was really hard to code */
        self.hnk.set_val(cmd, val);
    }
}
impl CommandLine {
    pub fn new(ctx: Arc<Mutex<ReadableContext>>, tx: BackendSender, b: &Builder) -> Rc<RefCell<Self>> {
        let line = CommandLine {
            ctx: ctx,
            cmd: None,
            tx: tx,
            hunks: Vec::new(),
            ready: false,
            line: b.get_object("command-line").unwrap(),
            h_image: b.get_object("line-hint-image").unwrap(),
            h_label: b.get_object("line-hint-label").unwrap(),
        };
        let line = Rc::new(RefCell::new(line));
        Self::update(line.clone());
        line
    }
    pub fn build(selfish2: Rc<RefCell<Self>>, cmd: Box<Command>) {
        {
            let mut selfish = selfish2.borrow_mut();
            let name = cmd.name();
            selfish.clear();
            selfish.cmd = Some(Rc::new(RefCell::new(cmd)));

            let name_lbl = Label::new(None);
            name_lbl.set_markup(&format!("<span weight=\"bold\" fgcolor=\"#666666\">{}</span>", name));
            selfish.line.pack_start(&name_lbl, false, false, 3);
            let mut hunks = Vec::new();
            for (i, hunk) in selfish.cmd.as_ref().unwrap().borrow().get_hunks().into_iter().enumerate() {
                let mut hui = HunkUI::from_hunk(hunk);
                hui.ctl.pack(&selfish.line);
                hui.ctl.bind(selfish2.clone(), i);
                hunks.push(hui);
            }
            selfish.line.show_all();
            if let Some(wdgt) = hunks.get(0) {
                wdgt.ctl.focus();
            }
            selfish.hunks = hunks;
        }
        Self::update(selfish2);
    }
    pub fn set_val(selfish: Rc<RefCell<Self>>, idx: usize, val: Option<Box<::std::any::Any>>) {
        // FIXME: this check is required because some hunks' event handlers may fire on destruction.
        if let ::std::cell::BorrowState::Unused = selfish.borrow_state() {
            {
                let mut selfish = selfish.borrow_mut();
                let cmd = selfish.cmd.as_ref().unwrap().clone();
                let mut cmd = cmd.borrow_mut();
                selfish.hunks[idx].set_val(&mut cmd, val);
            }
            Self::update(selfish);
        }
    }
    pub fn update(selfish: Rc<RefCell<Self>>) {
        let mut selfish = selfish.borrow_mut();
        if selfish.cmd.is_none() {
            selfish.h_image.set_from_icon_name("dialog-question", 1);
            selfish.h_label.set_text("Command line idle.");
            selfish.clear();
            let label = Label::new(None);
            label.set_markup("<span fgcolor=\"#888888\"><i>Select a command with Ctrl+Enter</i></span>");
            selfish.line.pack_start(&label, false, false, 0);
            selfish.line.show_all();
            return;
        }
        assert!(selfish.hunks.len() > 0);
        let mut erred = 0;
        let ctx = selfish.ctx.clone();
        let ctx = ctx.lock().unwrap();
        let cmd = selfish.cmd.as_ref().unwrap().clone();
        let cmd = cmd.borrow();
        for hunk in &mut selfish.hunks {
            hunk.update(&cmd, &ctx);
            match hunk.state {
                HunkFSM::Err => erred += 1,
                HunkFSM::UIErr => erred += 1,
                _ => {}
            }
        }
        if erred > 0 {
            selfish.ready = false;
            selfish.h_image.set_from_icon_name("dialog-error", 1);
            selfish.h_label.set_markup(&format!("{} error(s) <span fgcolor=\"#888888\">- Ctrl+Enter to change or clear command</span>", erred));
        }
        else {
            selfish.ready = true;
            selfish.h_image.set_from_icon_name("dialog-ok", 1);
            selfish.h_label.set_markup("Ready <span fgcolor=\"#888888\">- Ctrl+Enter twice to execute</span>");
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

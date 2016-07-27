use command::{Hunk, HunkTypes, HunkState};
use state::{CommandDescriptor, Message};
use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;
use gtk::{Label, Image, Builder};
use gtk::Box as GtkBox;
use std::sync::{Arc, Mutex};
use backend::BackendSender;
use uuid::Uuid;
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
    data: HunkState,
    ctl: Box<HunkUIController>,
    state: HunkFSM
}

pub struct CommandLine {
    pub cd: Option<CommandDescriptor>,
    pub uuid: Option<Uuid>,
    pub tx: BackendSender,
    pub hunks: Vec<HunkUI>,
    pub ready: bool,
    pub line: GtkBox,
    pub h_image: Image,
    pub h_label: Label,
}
impl HunkUI {
    pub fn from_state(hs: HunkState) -> Self {
        let ctl: Box<HunkUIController> = match &hs.val {
            &HunkTypes::FilePath(..) => Box::new(EntryUIController::new("document-open")),
            &HunkTypes::Identifier(..) => Box::new(EntryUIController::new("edit-find")),
            &HunkTypes::String(..) => Box::new(EntryUIController::new("text-x-generic")),
            &HunkTypes::Label(..) => Box::new(TextUIController::new()),
            &HunkTypes::Volume(..) => Box::new(VolumeUIController::new()),
            &HunkTypes::Time(..) => Box::new(TimeUIController::new())
        };
        HunkUI {
            data: hs,
            ctl: ctl,
            state: HunkFSM::Err
        }
    }
    pub fn update(&mut self, state: HunkState) {
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
        self.ctl.set_val(state.val.unwrap_ref());
        self.ctl.set_help(state.help);
    }
}
impl CommandLine {
    pub fn new(tx: BackendSender, b: &Builder) -> Rc<RefCell<Self>> {
        let line = CommandLine {
            cd: None,
            uuid: None,
            tx: tx,
            hunks: Vec::new(),
            ready: false,
            line: b.get_object("command-line").unwrap(),
            h_image: b.get_object("line-hint-image").unwrap(),
            h_label: b.get_object("line-hint-label").unwrap(),
        };
        let line = Rc::new(RefCell::new(line));
        Self::update(line.clone(), None);
        line
    }
    pub fn build(selfish2: Rc<RefCell<Self>>, cd: CommandDescriptor) {
        {
            let mut selfish = selfish2.borrow_mut();
            selfish.clear();
            selfish.uuid = None;
            selfish.cd = Some(cd);
            let mut hunks = Vec::new();
            for (i, hunk) in selfish.cd.as_ref().unwrap().hunks.iter().enumerate() {
                let mut hui = HunkUI::from_state(hunk.clone());
                hui.ctl.pack(&selfish.line);
                hui.ctl.bind(selfish2.clone(), i, hunk.val.clone());
                hunks.push(hui);
            }
            selfish.line.show_all();
            if let Some(wdgt) = hunks.get(0) {
                wdgt.ctl.focus();
            }
            selfish.hunks = hunks;
        }
        Self::update(selfish2, None);
    }
    pub fn set_val(selfish: Rc<RefCell<Self>>, idx: usize, val: HunkTypes) {
        // FIXME: this check is required because some hunks' event handlers may fire on destruction.
        if let ::std::cell::BorrowState::Unused = selfish.borrow_state() {
            {
                let mut selfish = selfish.borrow_mut();
                selfish.tx.send(Message::SetHunk(selfish.cd.as_ref().unwrap().uuid, idx, val));
                selfish.h_image.set_from_icon_name("dialog-warning", 1);
                selfish.h_label.set_text("Waiting for backend...");
            }
        }
    }
    pub fn update(selfish: Rc<RefCell<Self>>, cd: Option<CommandDescriptor>) {
        let mut selfish = selfish.borrow_mut();
        if selfish.cd.is_none() {
            if selfish.uuid.is_some() {
                selfish.h_image.set_from_icon_name("dialog-warning", 1);
                selfish.h_label.set_text("Waiting for backend...");
                selfish.clear();
                let label = Label::new(None);
                label.set_markup("<i>If you've been staring at this for too long, the backend may have died.</i>");
                selfish.line.pack_start(&label, false, false, 0);
                selfish.line.show_all();
            }
            else {
                selfish.h_image.set_from_icon_name("dialog-question", 1);
                selfish.h_label.set_text("Command line idle.");
                selfish.clear();
                let label = Label::new(None);
                label.set_markup("<span fgcolor=\"#888888\"><i>Select a command with Ctrl+Enter</i></span>");
                selfish.line.pack_start(&label, false, false, 0);
                selfish.line.show_all();
            }
            return;
        }
        assert!(selfish.hunks.len() > 0);
        if let Some(c) = cd {
            selfish.cd = Some(c);
        }
        let mut erred = 0;
        let mut cd = selfish.cd.clone().unwrap().hunks.into_iter();
        for (i, hunk) in selfish.hunks.iter_mut().enumerate() {
            hunk.update(cd.next().unwrap());
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

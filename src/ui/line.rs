use command::{HunkTypes, HunkState};
use commands::CommandSpawner;
use ui::UIState;
use state::{CommandDescriptor, Message};
use std::cell::RefCell;
use std::rc::Rc;
use std::mem;
use gtk::prelude::*;
use gtk::{Label, Image, Builder, ListStore};
use gtk::Box as GtkBox;
use backend::BackendSender;
use uuid::Uuid;
use super::hunks::*;

pub enum HunkFSM {
    Err,
    UIErr,
    Ok
}

pub struct HunkUI {
    ctl: Box<HunkUIController>,
    state: HunkFSM
}

#[derive(Clone)]
pub enum CommandLineFSM {
    Idle,
    AwaitingCreation(Uuid),
    Editing(CommandDescriptor, bool)
}
pub struct CommandLine {
    pub state: CommandLineFSM,
    pub tx: BackendSender,
    pub hunks: Vec<HunkUI>,
    pub ready: bool,
    pub line: GtkBox,
    pub h_image: Image,
    pub h_label: Label,
    pub completion: ListStore,
    uistate: UIState
}
impl HunkUI {
    pub fn from_state(hs: HunkState) -> Self {
        let ctl: Box<HunkUIController> = match &hs.val {
            &HunkTypes::FilePath(..) => Box::new(EntryUIController::new("document-open")),
            &HunkTypes::Identifier(..) => Box::new(IdentUIController::new()),
            &HunkTypes::String(..) => Box::new(EntryUIController::new("text-x-generic")),
            &HunkTypes::Label(..) => Box::new(TextUIController::new()),
            &HunkTypes::Volume(..) => Box::new(VolumeUIController::new()),
            &HunkTypes::Time(..) => Box::new(TimeUIController::new()),
            &HunkTypes::Checkbox(..) => Box::new(CheckboxUIController::new())
        };
        HunkUI {
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
        self.ctl.set_accel(state.accel);
        self.ctl.set_val(state.val.unwrap_ref());
        self.ctl.set_help(state.help);
    }
}
impl CommandLine {
    pub fn new(tx: BackendSender, ts: ListStore, state: UIState, b: &Builder) -> Rc<RefCell<Self>> {
        let line = CommandLine {
            state: CommandLineFSM::Idle,
            completion: ts,
            tx: tx,
            hunks: Vec::new(),
            ready: false,
            line: b.get_object("command-line").unwrap(),
            h_image: b.get_object("line-hint-image").unwrap(),
            h_label: b.get_object("line-hint-label").unwrap(),
            uistate: state
        };
        let line = Rc::new(RefCell::new(line));
        Self::update(line.clone(), None);
        line
    }
    pub fn set_ui_state(selfish: Rc<RefCell<Self>>, state: UIState) {
        {
            selfish.borrow_mut().uistate = state;
        }
        Self::update(selfish, None);
    }
    fn build(selfish2: Rc<RefCell<Self>>, cd: CommandDescriptor, creation: bool) {
        {
            let mut selfish = selfish2.borrow_mut();
            selfish.clear();

            let name_lbl = Label::new(None);
            name_lbl.set_markup(&format!("<span weight=\"bold\" fgcolor=\"#666666\">{}</span>", cd.name));
            selfish.line.pack_start(&name_lbl, false, false, 3);
            let mut hunks = Vec::new();
            let compl = selfish.completion.clone();
            for (i, hunk) in cd.hunks.iter().enumerate() {
                let mut hui = HunkUI::from_state(hunk.clone());
                hui.ctl.pack(&selfish.line);
                hui.ctl.bind(selfish2.clone(), i, hunk.val.clone());
                hui.ctl.bind_completions(compl.clone());
                hunks.push(hui);
            }

            selfish.state = CommandLineFSM::Editing(cd, creation);
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
            let selfish = selfish.borrow_mut();
            if let CommandLineFSM::Editing(ref cd, _) = selfish.state {
                selfish.tx.send(Message::SetHunk(cd.uuid, idx, val)).unwrap();
                selfish.h_image.set_from_icon_name("dialog-warning", 1);
                selfish.h_label.set_text("Waiting for backend...");
            }
        }
    }
    pub fn new_command(selfish: Rc<RefCell<Self>>, spawner: CommandSpawner) {
        CommandLine::reset(selfish.clone());
        {
            let mut selfish = selfish.borrow_mut();
            let uu = Uuid::new_v4();
            selfish.tx.send(Message::NewCmd(uu, spawner)).unwrap();
            selfish.state = CommandLineFSM::AwaitingCreation(uu);
        }
        CommandLine::update(selfish, None);
    }
    pub fn edit_command(selfish: Rc<RefCell<Self>>, cd: CommandDescriptor) {
        CommandLine::reset(selfish.clone());
        CommandLine::build(selfish, cd, false);
    }
    pub fn reset(selfish: Rc<RefCell<Self>>) {
        {
            let mut selfish = selfish.borrow_mut();
            match selfish.state {
                CommandLineFSM::Editing(ref cd, creation) => {
                    if creation {
                        selfish.tx.send(Message::Delete(cd.uuid)).unwrap();
                    }
                },
                _ => {}
            }
            selfish.state = CommandLineFSM::Idle;
        }
        CommandLine::update(selfish, None);
    }
    pub fn update(selfish: Rc<RefCell<Self>>, input: Option<CommandDescriptor>) {
        let state = { selfish.borrow().state.clone() };
        if let CommandLineFSM::AwaitingCreation(uu) = state {
            {
                let mut selfish = selfish.borrow_mut();
                selfish.h_image.set_from_icon_name("dialog-warning", 1);
                selfish.h_label.set_text("Awaiting command creation...");
                selfish.clear();
                let label = Label::new(None);
                label.set_markup("<i>If you've been staring at this for too long, the backend may have died.</i>");
                selfish.line.pack_start(&label, false, false, 0);
                selfish.line.show_all();
            }
            if let Some(newdesc) = input {
                assert!(newdesc.uuid == uu);
                CommandLine::build(selfish, newdesc, true);
            }
            return;
        }
        else if let CommandLineFSM::Idle = state {
            let mut selfish = selfish.borrow_mut();
            if selfish.uistate.live {
                selfish.h_image.set_from_icon_name("media-seek-forward", 1);
                selfish.h_label.set_markup("<b>Live mode</b> - new commands will be immediately executed <span fgcolor=\"#888888\">(Ctrl+Enter O to change to Blind)</span>");
            }
            else {
                selfish.h_image.set_from_icon_name("edit-cut", 1);
                selfish.h_label.set_markup("<b>Blind mode</b> - new commands can be attached to cues <span fgcolor=\"#888888\">(Ctrl+Enter O to change to Live)</span>");
            }
            selfish.clear();
            let label = Label::new(None);
            label.set_markup("<span fgcolor=\"#888888\"><i>Select a command with Ctrl+Enter</i></span>");
            selfish.line.pack_start(&label, false, false, 0);
            selfish.line.show_all();
            return;
        }
        let mut selfish = selfish.borrow_mut();
        if let CommandLineFSM::Editing(mut cd, creation) = selfish.state.clone() {
            assert!(selfish.hunks.len() > 0);
            if let Some(c) = input {
                cd = c;
            }
            let mut erred = 0;
            let mut hunk_states = cd.hunks.into_iter();
            for (i, hunk) in selfish.hunks.iter_mut().enumerate() {
                hunk.update(hunk_states.next().unwrap());
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
                if selfish.uistate.live && creation {
                    selfish.h_label.set_markup("Ready to execute <span fgcolor=\"#888888\">- Ctrl+Enter twice to execute</span>");
                }
                else {
                    selfish.h_label.set_markup("Command OK <span fgcolor=\"#888888\">- Ctrl+Enter twice to apply changes</span>");
                }
            }
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

//! Functions for managing the frontend UI
use command::{Command, Hunk, HunkTypes};
use commands::{get_chooser_grid, GridNode, CommandSpawner};
use state::ReadableContext;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Label, Image, Grid, Entry, Button, Builder, Popover};
use gtk::Box as GtkBox;
use std::ops::Rem;
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

pub struct CommandChooserController {
    grid: Grid,
    status: Label,
    back_btn: Button,
    pop: Popover,
    cl: Rc<RefCell<CommandLine>>,
    pos: Vec<usize>,
    top: Vec<(&'static str, GridNode)>
}

impl CommandChooserController {
    pub fn new(cl: Rc<RefCell<CommandLine>>, b: &Builder) -> Rc<RefCell<Self>> {
        let ret = Rc::new(RefCell::new(CommandChooserController {
            grid: b.get_object("cc-grid").unwrap(),
            status: b.get_object("cc-status-label").unwrap(),
            back_btn: b.get_object("cc-end-button").unwrap(),
            pop: b.get_object("command-chooser-popover").unwrap(),
            pos: vec![],
            cl: cl,
            top: get_chooser_grid()
        }));
        ret.borrow().back_btn.connect_clicked(clone!(ret; |_s| {
            {
                let mut pos = &mut ret.borrow_mut().pos;
                let len = pos.len().saturating_sub(1);
                pos.truncate(len);
            }
            Self::update(ret.clone());
        }));
        ret
    }
    pub fn toggle(selfish_: Rc<RefCell<Self>>) {
        {
            let mut selfish = selfish_.borrow_mut();
            selfish.pos = vec![];
            selfish.pop.show_all();
        }
        Self::update(selfish_);
    }
    pub fn update(selfish_: Rc<RefCell<Self>>) {
        let selfish = selfish_.borrow();
        let mut ptr = &selfish.top;
        if selfish.pos.len() > 0 {
            for i in &selfish.pos {
                if let Some(&(_, GridNode::Grid(ref vec))) = ptr.get(*i) {
                    ptr = vec;
                }
                else {
                    panic!("Grid traversal failed");
                }
            }
            selfish.back_btn.set_sensitive(true);
        }
        else {
            selfish.back_btn.set_sensitive(false);
        }
        for chld in selfish.grid.get_children() {
            chld.destroy();
        }
        for (i, &(st, ref opt)) in ptr.iter().enumerate() {
            let lbl = Label::new(None);
            let btn = Button::new();
            lbl.set_markup(st);
            btn.add(&lbl);
            match opt {
                &GridNode::Choice(spawner) => {
                    let ref cl = selfish.cl;
                    btn.connect_button_press_event(clone!(selfish_, cl; |_s, _e| {
                        let cmd = spawner.spawn();
                        selfish_.borrow().pop.hide();
                        CommandLine::build(cl.clone(), cmd);
                        Inhibit(true)
                    }));
                },
                &GridNode::Grid(_) => {
                    btn.connect_button_press_event(clone!(selfish_; |_s, _e| {
                        {
                            selfish_.borrow_mut().pos.push(i);
                        }
                        Self::update(selfish_.clone());
                        Inhibit(true)
                    }));
                }
            }
            selfish.grid.attach(&btn, i.rem(3) as i32, (i/3) as i32, 1, 1);
        }
        selfish.grid.show_all();
    }
}

pub enum HunkFSM {
    Err,
    Ok
}
struct PopoverUIController {
    popover: Popover,
    state_lbl: Label,
    state_actions: GtkBox,
    err_box: GtkBox,
    err_lbl: Label,
    unset_btn: Button
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
    fn focus(&self) {}
    fn pack(&self, onto: &GtkBox);
    fn set_help(&mut self, _help: &'static str) {}
    fn set_val(&mut self, _val: Option<&Box<::std::any::Any>>) {}
    fn error(&mut self, _err: Option<String>) {}
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
    fn set_err(&self, err: Option<String>) {
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
        self.state_actions.pack_start(&self.unset_btn, false, false, 0);
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
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>) {
        match val {
            Some(txt) => {
                self.lbl.set_markup(&format!("<span fgcolor=\"#888888\"><i>{}</i></span>",txt.downcast_ref::<String>().unwrap()));
            },
            None => {
                self.lbl.set_markup("");
            }
        }
    }
    fn error(&mut self, _: Option<String>) {}
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
    fn focus(&self) {
        self.ent.grab_focus();
    }
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
            if selfish.in_destruction() {
                return;
            }
            let txt = selfish.get_text().unwrap();
            let val: Option<Box<::std::any::Any>> = if txt == "" { None } else { Some(Box::new(txt)) };
            CommandLine::set_val(line.clone(), idx, val);
        });
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>) {
        self.pop.borrow().val_exists(val.is_some());
        match val {
            Some(txt) => {
                self.ent.set_text(&txt.downcast_ref::<String>().unwrap());
            },
            None => {
                self.ent.set_text("");
            }
        }
    }
    fn error(&mut self, err: Option<String>) {
        if err.is_some() {
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-error"));
        }
        else {
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, None);
        }
        self.pop.borrow().set_err(err);
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
        if state.err.is_some() {
            self.state = HunkFSM::Err;
            self.ctl.error(state.err);
        }
        else if state.val.is_none() && state.required {
            self.state = HunkFSM::Err;
            self.ctl.error(Some(format!("This field is required, but contains nothing.")));
        }
        else {
            self.state = HunkFSM::Ok;
            self.ctl.error(None);
        }
        self.ctl.set_val(state.val.as_ref());
        self.ctl.set_help(state.help);
    }
    fn set_val(&mut self, cmd: &mut Box<Command>, val: Option<Box<::std::any::Any>>) {
        /* gee, this function was really hard to code */
        self.hnk.set_val(cmd, val);
    }
}
impl CommandLine {
    pub fn new(ctx: Rc<RefCell<ReadableContext>>, b: &Builder) -> Rc<RefCell<Self>> {
        let line = CommandLine {
            ctx: ctx,
            cmd: None,
            hunks: Vec::new(),
            ready: false,
            line: b.get_object("command-line").unwrap(),
            h_image: b.get_object("line-hint-image").unwrap(),
            h_label: b.get_object("line-hint-label").unwrap()
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
            selfish.line.pack_start(&name_lbl, false, false, 5);
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
    fn set_val(selfish: Rc<RefCell<Self>>, idx: usize, val: Option<Box<::std::any::Any>>) {
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
        let mut erred = 0;
        let ctx = selfish.ctx.clone();
        let ctx = ctx.borrow();
        let cmd = selfish.cmd.as_ref().unwrap().clone();
        let cmd = cmd.borrow();
        for hunk in &mut selfish.hunks {
            hunk.update(&cmd, &ctx);
            match hunk.state {
                HunkFSM::Err => erred += 1,
                _ => {}
            }
        }
        if erred > 0 {
            selfish.ready = false;
            selfish.h_image.set_from_icon_name("dialog-error", 1);
            selfish.h_label.set_text(&format!("{} error(s)", erred));
        }
        else {
            selfish.ready = true;
            selfish.h_image.set_from_icon_name("dialog-ok", 1);
            selfish.h_label.set_text("Ready [press Enter to execute]");
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
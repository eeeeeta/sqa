//! Functions for managing the frontend UI
use command::{Command, Hunk, HunkTypes};
use commands::{get_chooser_grid, GridNode};
use state::ReadableContext;
use streamv2::{lin_db, db_lin};
use std::cell::RefCell;
use std::rc::Rc;
use gdk::enums::key as gkey;
use gtk::prelude::*;
use gtk::{Label, Image, Grid, Entry, Button, Builder, Popover, Scale};
use gtk::Box as GtkBox;
use std::ops::Rem;
use std::sync::{Arc, Mutex};
use backend::{BackendSender, BackendMessage};

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
    back_btn: Button,
    pop: Popover,
    cl: Rc<RefCell<CommandLine>>,
    pos: Vec<usize>,
    top: Vec<(&'static str, gkey::Key, GridNode)>
}

impl CommandChooserController {
    pub fn new(cl: Rc<RefCell<CommandLine>>, b: &Builder) -> Rc<RefCell<Self>> {
        let ret = Rc::new(RefCell::new(CommandChooserController {
            grid: b.get_object("cc-grid").unwrap(),
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
        ret.borrow().pop.connect_key_press_event(clone!(ret; |_s, ek| {
            if ek.get_keyval() == gkey::BackSpace {
                ret.borrow().back_btn.clone().activate();
                return Inhibit(true)
            }
            let mut wdgt: Option<::gtk::Widget> = None;
            {
                let selfish = ret.borrow();
                for (i, &(_, key, _)) in selfish.get_ptr().iter().rev().enumerate() {
                    if ek.get_keyval() == key {
                        wdgt = Some(selfish.grid.get_children()[i].clone());
                        break;
                    }
                }
            }
            if let Some(w) = wdgt {
                let w = w.downcast::<Button>().unwrap();
                if w.is_sensitive() {
                    w.clicked();
                }
                else {
                    let sctx = _s.get_style_context().unwrap();
                    sctx.add_class("err-pulse");
                    ::gdk::beep();
                    timeout_add(450, move || {
                        sctx.remove_class("err-pulse");
                        Continue(false)
                    });
                }
                Inhibit(true)
            }
            else {
                Inhibit(false)
            }
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
    pub fn execute(cl: Rc<RefCell<CommandLine>>, clone: bool) {
        CommandLine::update(cl.clone());
        {
            let cl = cl.borrow();
            if !cl.ready { return; }
        }
        let cmd: Rc<RefCell<Box<Command>>>;
        {
            let mut cl = cl.borrow_mut();
            if clone {
                cmd = Rc::new(RefCell::new(cl.cmd.as_ref().unwrap().borrow().box_clone()));
            }
            else {
                cmd = cl.cmd.take().unwrap();
            }
        }
        CommandLine::update(cl.clone());
        {
            let cl = cl.borrow_mut();
            cl.tx.send(BackendMessage::Execute(Rc::try_unwrap(cmd).ok().unwrap().into_inner())).unwrap();
        }
    }
    fn get_ptr(&self) -> &Vec<(&'static str, gkey::Key, GridNode)> {
        let mut ptr = &self.top;
        if self.pos.len() > 0 {
            for i in &self.pos {
                if let Some(&(_, _, GridNode::Grid(ref vec))) = ptr.get(*i) {
                    ptr = vec;
                }
                else {
                    panic!("Grid traversal failed");
                }
            }
            self.back_btn.set_sensitive(true);
        }
        else {
            self.back_btn.set_sensitive(false);
        }
        ptr
    }
    pub fn update(selfish_: Rc<RefCell<Self>>) {
        let selfish = selfish_.borrow();
        let ptr = selfish.get_ptr();
        for chld in selfish.grid.get_children() {
            chld.destroy();
        }
        for (i, &(st, _, ref opt)) in ptr.iter().enumerate() {
            let lbl = Label::new(None);
            let btn = Button::new();
            lbl.set_markup(st);
            btn.add(&lbl);
            match opt {
                &GridNode::Choice(spawner) => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        let cmd = spawner.spawn();
                        selfish_.borrow().pop.hide();
                        CommandLine::build(cl.clone(), cmd);
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-choice");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                },
                &GridNode::Grid(_) => {
                    btn.connect_clicked(clone!(selfish_; |_s| {
                        {
                            selfish_.borrow_mut().pos.push(i);
                        }
                        Self::update(selfish_.clone());
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode-grid");
                    lbl.get_style_context().unwrap().add_class("gridnode");
                },
                &GridNode::Clear => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        cl.borrow_mut().cmd = None;
                        CommandLine::update(cl.clone());
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if cl.borrow().cmd.is_some() {
                        lbl.get_style_context().unwrap().add_class("gridnode-clear");
                        btn.set_sensitive(true);
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                },
                &GridNode::Execute(clone) => {
                    let ref cl = selfish.cl;
                    btn.connect_clicked(clone!(selfish_, cl; |_s| {
                        Self::execute(cl.clone(), clone);
                        selfish_.borrow().pop.hide();
                    }));
                    lbl.get_style_context().unwrap().add_class("gridnode");
                    if cl.borrow().ready {
                        btn.set_sensitive(true);
                        lbl.get_style_context().unwrap().add_class("gridnode-execute");
                    }
                    else {
                        btn.set_sensitive(false);
                    }
                }
            }
            selfish.grid.attach(&btn, i.rem(3) as i32, (i/3) as i32, 1, 1);
        }
        selfish.grid.show_all();
    }
}

pub enum HunkFSM {
    Err,
    UIErr,
    Ok
}
struct PopoverUIController {
    popover: Popover,
    state_lbl: Label,
    state_actions: GtkBox,
    err_box: GtkBox,
    err_lbl: Label,
    err_vis: bool,
    unset_btn: Button
}
struct EntryUIController {
    pop: Rc<RefCell<PopoverUIController>>,
    ent: Entry
}
struct VolumeUIController {
    entuic: EntryUIController,
    sc: Scale,
    err: Rc<RefCell<bool>>
}
struct TimeUIController {
    entuic: EntryUIController,
    err: Rc<RefCell<Option<String>>>
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
    fn set_error(&mut self, _err: Option<String>) {}
    fn get_error(&self) -> Option<String> { None }
}
pub struct HunkUI {
    hnk: Box<Hunk>,
    ctl: Box<HunkUIController>,
    state: HunkFSM
}
pub struct CommandLine {
    ctx: Arc<Mutex<ReadableContext>>,
    tx: BackendSender,
    cmd: Option<Rc<RefCell<Box<Command>>>>,
    hunks: Vec<HunkUI>,
    ready: bool,
    line: GtkBox,
    h_image: Image,
    h_label: Label,
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
            err_vis: false
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
        self.err_box.set_visible(self.err_vis);
    }
    fn set_help(&self, hlp: &'static str) {
        self.state_lbl.set_text(hlp);
    }
    fn val_exists(&self, exists: bool) {
        self.unset_btn.set_sensitive(exists);
    }
    fn set_err(&mut self, err: Option<String>) {
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
                self.lbl.set_markup(&format!("<span fgcolor=\"#888888\">{}</span>",txt.downcast_ref::<String>().unwrap()));
            },
            None => {
                self.lbl.set_markup("");
            }
        }
    }
    fn set_error(&mut self, _: Option<String>) {}
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
impl TimeUIController {
    fn new() -> Self {
        TimeUIController {
            entuic: EntryUIController::new("appointment-soon"),
            err: Rc::new(RefCell::new(None))
        }
    }
}
impl VolumeUIController {
    fn new() -> Self {
        let ret = VolumeUIController {
            entuic: EntryUIController::new("volume-knob"),
            sc: Scale::new_with_range(::gtk::Orientation::Horizontal, 0.0, db_lin(3.0) as f64, 0.01),
            err: Rc::new(RefCell::new(false))
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
        ret
    }
}
impl HunkUIController for TimeUIController {
    fn focus(&self) {
        self.entuic.focus();
    }
    fn pack(&self, onto: &GtkBox) {
        self.entuic.pack(onto);
    }
    fn set_help(&mut self, help: &'static str) {
        self.entuic.set_help(help);
    }
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize) {
        let ref pop = self.entuic.pop;
        let ref ent = self.entuic.ent;
        let ref uierr = self.err;

        pop.borrow().bind_defaults(line.clone(), idx);
        self.entuic.ent.connect_focus_in_event(clone!(pop; |_s, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.entuic.ent.connect_focus_out_event(clone!(pop, uierr, line, ent; |_s, _y| {
            pop.borrow().visible(false);
            if let Some(strn) = ent.get_text() {
                if let Ok(_) = str::parse::<u64>(&strn) {
                    ent.activate();
                    return Inhibit(false);
                }
                else if strn == "" {
                    CommandLine::set_val(line.clone(), idx, None);
                    *uierr.borrow_mut() = None;
                    return Inhibit(false);
                }
            }
            else {
                CommandLine::set_val(line.clone(), idx, None);
                *uierr.borrow_mut() = None;
                return Inhibit(false);
            }
            *uierr.borrow_mut() = Some(ent.get_text().unwrap().to_owned());
            CommandLine::update(line.clone());
            Inhibit(false)
        }));
        self.entuic.ent.connect_activate(clone!(line, uierr; |ent| {
            *uierr.borrow_mut() = None;
            if let Some(strn) = ent.get_text() {
                if let Ok(ref val) = str::parse::<u64>(&strn) {
                    CommandLine::set_val(line.clone(), idx, Some(Box::new(*val)));
                }
                else if strn == "" {
                    *uierr.borrow_mut() = None;
                    CommandLine::set_val(line.clone(), idx, None);
                }
                else {
                    *uierr.borrow_mut() = Some(strn.to_owned());
                    CommandLine::update(line.clone());
                }
            }
            else {
                CommandLine::set_val(line.clone(), idx, None);
            }
        }));
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>) {
        if self.err.borrow().is_some() {
            return;
        }
        self.entuic.pop.borrow().val_exists(val.is_some());
        match val {
            Some(txt) => {
                self.entuic.ent.set_text(&format!("{}", txt.downcast_ref::<u64>().unwrap()));
            },
            None => {
                self.entuic.ent.set_text("");
            }
        }
    }
    fn set_error(&mut self, err: Option<String>) {
        self.entuic.set_error(err);
    }
    fn get_error(&self) -> Option<String> {
        if self.err.borrow().is_some() {
            Some(format!("Please enter a valid whole number of milliseconds (or unset this value)."))
        }
        else {
            None
        }
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
    fn bind(&mut self, line: Rc<RefCell<CommandLine>>, idx: usize) {
        let ref pop = self.entuic.pop;
        let ref sc = self.sc;
        let ref ent = self.entuic.ent;
        let ref uierr = self.err;
        ent.set_text(&format!("{:.2}", lin_db(sc.get_value() as f32)));
        self.entuic.ent.connect_focus_in_event(clone!(pop; |_s, _y| {
            pop.borrow().visible(true);
            Inhibit(false)
        }));
        self.entuic.ent.connect_focus_out_event(clone!(pop, uierr, line, ent; |_s, _y| {
            pop.borrow().visible(false);
            if let Some(strn) = ent.get_text() {
                if let Ok(_) = str::parse::<f64>(&strn) {
                    return Inhibit(false);
                }
            }
            *uierr.borrow_mut() = true;
            CommandLine::update(line.clone());
            Inhibit(false)
        }));
        self.entuic.ent.connect_key_release_event(clone!(sc, line, uierr; |ent, _e| {
            if let Some(strn) = ent.get_text() {
                if let Ok(mut flt) = str::parse::<f64>(&strn) {
                    *uierr.borrow_mut() = false;
                    CommandLine::set_val(line.clone(), idx, Some(Box::new(flt as f32)));
                    flt = db_lin(flt as f32) as f64;
                    sc.set_value(flt);
                }
            }
            Inhibit(false)
        }));
        self.sc.connect_change_value(clone!(uierr, line, ent; |_sc, _st, val| {
            let mut val = val as f32; /* stupid macro */
            if val < 0.0002 {
                val = ::std::f32::NEG_INFINITY;
            }
            else {
                val = lin_db(val);
            }
            *uierr.borrow_mut() = false;
            ent.set_text(&format!("{:.2}", val));
            CommandLine::set_val(line.clone(), idx, Some(Box::new(val)));
            Inhibit(false)
        }));
    }
    fn set_val(&mut self, val: Option<&Box<::std::any::Any>>) {
        self.sc.set_value((*val.unwrap().downcast_ref::<f32>().unwrap()) as f64);
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
    fn set_error(&mut self, err: Option<String>) {
        if err.is_some() {
            self.ent.get_style_context().unwrap().add_class("entry-error");
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, Some("dialog-error"));
        }
        else {
            self.ent.get_style_context().unwrap().remove_class("entry-error");
            self.ent.set_icon_from_icon_name(::gtk::EntryIconPosition::Secondary, None);
        }
        self.pop.borrow_mut().set_err(err);
    }
}
impl HunkUI {
    fn from_hunk(hnk: Box<Hunk>) -> Self {
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
    fn update(&mut self, cmd: &Box<Command>, ctx: &ReadableContext) {
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
    fn set_val(&mut self, cmd: &mut Box<Command>, val: Option<Box<::std::any::Any>>) {
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

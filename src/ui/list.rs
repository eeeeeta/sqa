//! Control of the main TreeView that lists commands.
use state::{CommandState, ChainType, Chain, CommandDescriptor, Message};
use backend::BackendSender;
use ui::UISender;
use std::collections::{BTreeMap, HashMap};
use gtk::prelude::*;
use gtk::{Builder, TreeStore, ListStore, TreeIter, TreeView, TreeSelection, SelectionMode};
use uuid::Uuid;
use std::ops::Deref;
use std::rc::Rc;
use std::cell::RefCell;
use gdk::enums::key as gkey;

/// Helper function to extract a UUID from a TreeStore item.
fn extract_uuid(ts: &TreeStore, ti: &TreeIter, col: i32) -> Option<Uuid> {
    if let Some(v) = ts.get_value(ti, col).get::<String>() {
        if let Ok(uu) = Uuid::parse_str(&v) {
            return Some(uu);
        }
    }
    None
}
/// Helper function to extract fallthrough state from a TreeStore item.
fn extract_ft(ts: &TreeStore, ti: &TreeIter, col: i32) -> bool {
    if let Some(v) = ts.get_value(ti, col).get::<String>() {
        v.len() != 0
    }
    else {
        false
    }
}
/// The list controller itself, responsible for drawing and updating the command list.
pub struct ListController {
    store: TreeStore,
    view: TreeView,
    sel: TreeSelection,
    pub chains: BTreeMap<ChainType, Chain>,
    pub commands: HashMap<Uuid, CommandDescriptor>,
    identifiers: HashMap<String, Uuid>,
    completions: ListStore,
    sender: UISender,
    tx: BackendSender,
    sel_handler: Option<u64>
}
impl ListController {
    pub fn new(sender: UISender, tx: BackendSender, compl: ListStore, b: &Builder) -> Rc<RefCell<Self>> {
        let view: TreeView = b.get_object("command-tree-view").unwrap();
        view.set_enable_search(false);
        let sel = view.get_selection();
        sel.set_mode(SelectionMode::Single);
        let ret = Rc::new(RefCell::new(ListController {
            store: b.get_object("command-tree").unwrap(),
            view: view,
            sel: sel,
            chains: BTreeMap::new(),
            commands: HashMap::new(),
            identifiers: HashMap::new(),
            sender: sender,
            completions: compl,
            tx: tx,
            sel_handler: None
        }));
        let sel_handler = ret.borrow().sel.connect_changed(clone!(ret; |_s| {
            let (mut sender, uu) = {
                let selfish = ret.borrow();
                if let Some((_, iter)) = selfish.sel.get_selected() {
                    if let Some(uu) = extract_uuid(&selfish.store, &iter, 5) {
                        (selfish.sender.clone(), Some(uu))
                    }
                    else {
                        return;
                    }
                }
                else {
                    (selfish.sender.clone(), None)
                }
            };
            sender.send(Message::UIChangeSel(uu));
        }));
        ret.borrow_mut().sel_handler = Some(sel_handler);
        ret.borrow().view.connect_key_press_event(clone!(ret; |_s, ek| {
            if !ek.get_state().contains(::gdk::CONTROL_MASK) {
                match ek.get_keyval() {
                    gkey::e => {
                        if let Some((mut sender, _, uu)) = Self::get_sender_and_sel(ret.clone()) {
                            sender.send(Message::UIBeginEditing(uu));
                            Inhibit(true)
                        }
                        else {
                            Inhibit(false)
                        }
                    },
                    gkey::x => {
                        if let Some((_, sender, uu)) = Self::get_sender_and_sel(ret.clone()) {
                            sender.send(Message::Delete(uu)).unwrap();
                            Inhibit(true)
                        }
                        else {
                            Inhibit(false)
                        }
                    },
                    gkey::f => {
                        if let Some((mut sender, _, uu)) = Self::get_sender_and_sel(ret.clone()) {
                            sender.send(Message::UIToggleFallthru(uu));
                            Inhibit(true)
                        }
                        else {
                            Inhibit(false)
                        }
                    },
                    _ => Inhibit(false)
                }
            }
            else {
                Inhibit(false)
            }
        }));
        ret
    }
    /// Iterates over every item in the TreeStore, running a closure for each one.
    ///
    /// Stops and returns the value if the closure returns Some.
    fn run_for_each<T, U>(&self, mut cls: T) -> Option<U> where T: FnMut(&TreeIter, &TreeStore) -> Option<U> {
        let mut ti = match self.store.iter_children(None) {
            Some(v) => v,
            None => {
                return None;
            }
        };
        loop {
            if let Some(t) = cls(&ti, &self.store) {
                return Some(t);
            }
            if !self.store.iter_next(&mut ti) {
                break;
            }
        }
        None
    }
    /// Gets the fallthru state for a command..
    pub fn get_fallthru_state(selfish: Rc<RefCell<Self>>, uu: Uuid) -> bool {
        let selfish = selfish.borrow();
        if let Some(ti) = selfish.locate(uu) {
            extract_ft(&selfish.store, &ti, 6)
        }
        else {
            false
        }
    }
    /// Locates a command in the list.
    fn locate(&self, uu: Uuid) -> Option<TreeIter> {
        self.run_for_each(|ti, ts| {
            if let Some(u2) = extract_uuid(ts, ti, 5) {
                if u2 == uu {
                    return Some(ti.clone())
                }
            }
            None
        })
    }
    /// Redraws the entire TreeStore, preserving the user's selection if possible.
    fn redraw(&mut self) {
        ::glib::signal_handler_block(&self.sel, self.sel_handler.unwrap());
        let prev_sel = if let Some((_, iter)) = self.sel.get_selected() {
            if let Some(uu) = extract_uuid(&self.store, &iter, 5) {
                Some(uu)
            }
            else {
                None
            }
        } else { None };
        self.store.clear();
        self.completions.clear();
        for (ref ct, ref chn) in &self.chains {
            for (i, uu) in chn.commands.iter().enumerate() {
                if let Some(v) = self.commands.get(uu) {
                    let iter = self.store.append(None);
                    let (icon, desc) = self.render(&iter, v, *uu);
                    self.chain_render(&iter, ct, i, *chn.fallthru.get(uu).unwrap_or(&false));
                    self.completions_render(desc, icon, ct, i, *uu);
                }
            }
        }
        if let Some(ps) = prev_sel {
            if let Some(iter) = self.locate(ps) {
                self.sel.select_iter(&iter);
            }
            else {
                self.sender.send(Message::UIChangeSel(None));
            }
        }
        ::glib::signal_handler_unblock(&self.sel, self.sel_handler.unwrap());
    }
    /// Updates the list of identifier completions.
    fn update_completions(&mut self) {
        self.completions.clear();
        for (ref ct, ref chn) in &self.chains {
            for (i, uu) in chn.commands.iter().enumerate() {
                if let Some(v) = self.commands.get(uu) {
                    let (icon, desc, _, _) = Self::get_render_data(&v);
                    self.completions_render(desc, icon, ct, i, *uu);
                }
            }
        }
    }
    /// Renders a completion list item.
    fn completions_render(&self, desc: String, icon: String, ct: &ChainType, i: usize, uu: Uuid) {
        self.completions.set(&self.completions.append(), &vec![
            0, // identifier
            1, // uuid
            2, // description
            3, // icon
        ], &vec![
            &format!("{}-{}", ct, i) as &ToValue,
            &format!("{}", uu) as &ToValue,
            &desc as &ToValue,
            &icon as &ToValue,
        ].deref());
        for (k, v) in self.identifiers.iter() {
            if uu == *v {
                self.completions.set(&self.completions.append(), &vec![
                    0, // identifier
                    1, // uuid
                    2, // description
                    3, // icon
                ], &vec![
                    &format!("${}", k) as &ToValue,
                    &format!("{}", uu) as &ToValue,
                    &desc as &ToValue,
                    &icon as &ToValue,
                ].deref());
            }
        }

    }
    /// Renders the chain-specific information pertaining to a command.
    fn chain_render(&self, ti: &TreeIter, ct: &ChainType, cidx: usize, ft: bool) {
        let ident = format!("{}-{}", ct, cidx);
        let ft = if ft {
            format!("go-down")
        }
        else {
            format!("")
        };
        self.store.set(ti, &vec![
            1, // identifier (looking glass column)
            6, // flags (preferences column)
        ], &vec![
            &ident as &ToValue,
            &ft as &ToValue,
        ].deref());
    }
    /// Gets the (icon, description, duration, background colour) of a command for rendering.
    fn get_render_data(v: &CommandDescriptor)
                       -> (String, String, String, String) {
        let (mut icon, desc, mut dur, mut bgc) =
            (format!("dialog-question"),
             v.desc.clone(),
             format!(""),
             format!("white"));
        match v.state {
            CommandState::Incomplete => {
                icon = format!("dialog-error");
                bgc = format!("lightpink");
            },
            CommandState::Ready => {
                icon = format!("");
            },
            CommandState::Loaded => {
                icon = format!("go-home");
                bgc = format!("lemonchiffon");
            },
            CommandState::Running(cd) => {
                let cd = ::chrono::Duration::from_std(cd).unwrap();
                icon = format!("media-seek-forward");
                bgc = format!("powderblue");
                dur = format!("{:02}:{:02}:{:02}",
                              cd.num_hours(),
                              cd.num_minutes() - (60 * cd.num_hours()),
                              cd.num_seconds() - (60 * cd.num_minutes()));
            },
            _ => {}
        }
        (icon, desc, dur, bgc)
    }
    /// Renders non-chain-specific parts of a command.
    fn render(&self, ti: &TreeIter, v: &CommandDescriptor, uu: Uuid) -> (String, String) {
        let (icon, desc, dur, bgc) = Self::get_render_data(v);
        let uu = format!("{}", uu);
        self.store.set(ti, &vec![
            0, // icon
            2, // description
            3, // duration
            4, // background colour
            5, // UUID
        ], &vec![
            &icon as &ToValue,
            &desc as &ToValue,
            &dur as &ToValue,
            &bgc as &ToValue,
            &uu as &ToValue
        ].deref());
        (icon, desc)
    }
    /// Called when a CommandDescriptor is changed or added.
    pub fn update_desc(selfish: Rc<RefCell<Self>>, uu: Uuid, desc: CommandDescriptor) {
        let mut selfish = selfish.borrow_mut();
        selfish.commands.insert(uu, desc.clone());
        if let Some(ti) = selfish.locate(uu) {
            selfish.render(&ti, &desc, uu);
        }
    }
    /// Called when a CommandDescriptor is deleted.
    pub fn delete(selfish: Rc<RefCell<Self>>, uu: Uuid) {
        let mut selfish = selfish.borrow_mut();
        selfish.commands.remove(&uu);
        if let Some(ti) = selfish.locate(uu) {
            selfish.store.remove(&ti);
            selfish.redraw();
        }
    }
    /// Called when a chain is changed or added.
    pub fn update_chain(selfish: Rc<RefCell<Self>>, ct: ChainType, chain: Option<Chain>) {
        let mut selfish = selfish.borrow_mut();
        if let Some(chn) = chain {
            selfish.chains.insert(ct.clone(), chn.clone());
        }
        else {
            selfish.chains.remove(&ct);
        }
        selfish.redraw();
    }
    /// Called when a chain's fallthrough data is changed or added.
    pub fn update_chain_fallthru(selfish: Rc<RefCell<Self>>, ct: ChainType, chain: HashMap<Uuid, bool>) {
        let mut selfish = selfish.borrow_mut();
        let _: Option<()> = selfish.run_for_each(|ti, ts| {
            if let Some(uu) = extract_uuid(ts, ti, 5) {
                if let Some(b) = chain.get(&uu) {
                    let ft = if *b {
                        format!("go-down")
                    }
                    else {
                        format!("")
                    };
                    ts.set(ti, &vec![
                        6, // flags (preferences column)
                    ], &vec![
                        &ft as &ToValue,
                    ].deref());
                }
            }
            None
        });
        selfish.chains.get_mut(&ct).unwrap().fallthru = chain;
    }
    /// Called when the list of named identifiers is updated.
    pub fn update_identifiers(selfish: Rc<RefCell<Self>>, idents: HashMap<String, Uuid>) {
        let mut selfish = selfish.borrow_mut();
        selfish.identifiers = idents;
        selfish.update_completions();
    }
    /// Helper function for getting senders and the current selected command.
    pub fn get_sender_and_sel(selfish: Rc<RefCell<Self>>) -> Option<(UISender, BackendSender, Uuid)> {
        let selfish = selfish.borrow();
        if let Some((_, iter)) = selfish.sel.get_selected() {
            if let Some(uu) = extract_uuid(&selfish.store, &iter, 5) {
                Some((selfish.sender.clone(), selfish.tx.clone(), uu))
            }
            else {
                None
            }
        }
        else {
            None
        }
    }
    /// Called when the currently selected command is changed.
    pub fn update_sel(selfish: Rc<RefCell<Self>>, sel: Option<Uuid>) {
        if let Some(sel) = sel {
            if let Some((_, _, sel2)) = Self::get_sender_and_sel(selfish.clone()) {
                if sel == sel2 {
                    return;
                }
            }
            let selfish = selfish.borrow();
            if let Some(iter) = selfish.locate(sel) {
                ::glib::signal_handler_block(&selfish.sel, selfish.sel_handler.unwrap());
                selfish.sel.select_iter(&iter);
                ::glib::signal_handler_unblock(&selfish.sel, selfish.sel_handler.unwrap());
            }
        }
        else {
            selfish.borrow().sel.unselect_all();
        }
    }
}

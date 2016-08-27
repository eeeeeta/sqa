use state::{CommandState, ChainType, Chain, CommandDescriptor, Message};
use ui::UISender;
use std::collections::{BTreeMap, HashMap};
use gtk::prelude::*;
use gtk::{Builder, TreeStore, ListStore, TreeIter, TreeView, TreeSelection, SelectionMode};
use uuid::Uuid;
use std::ops::Deref;
use std::rc::Rc;
use std::cell::RefCell;
use gdk::enums::key as gkey;

fn extract_uuid(ts: &TreeStore, ti: &TreeIter, col: i32) -> Option<Uuid> {
    if let Some(v) = ts.get_value(ti, col).get::<String>() {
        if let Ok(uu) = Uuid::parse_str(&v) {
            return Some(uu);
        }
    }
    None
}
pub struct ListController {
    store: TreeStore,
    view: TreeView,
    sel: TreeSelection,
    chains: BTreeMap<ChainType, Chain>,
    pub commands: HashMap<Uuid, CommandDescriptor>,
    identifiers: HashMap<String, Uuid>,
    completions: ListStore,
    sender: UISender
}
impl ListController {
    pub fn new(sender: UISender, compl: ListStore, b: &Builder) -> Rc<RefCell<Self>> {
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
            completions: compl
        }));
        ret.borrow().view.connect_key_press_event(clone!(ret; |_s, ek| {
            if !ek.get_state().contains(::gdk::CONTROL_MASK) {
                match ek.get_keyval() {
                    gkey::e => {
                        let (mut sender, uu) = {
                            let selfish = ret.borrow();
                            if let Some((_, iter)) = selfish.sel.get_selected() {
                                if let Some(uu) = extract_uuid(&selfish.store, &iter, 5) {
                                    (selfish.sender.clone(), uu)
                                }
                                else {
                                    return Inhibit(false)
                                }
                            }
                            else {
                                return Inhibit(false)
                            }
                        };
                        sender.send(Message::UIBeginEditing(uu));
                        Inhibit(true)
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
    fn redraw(&mut self) {
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
                    self.chain_render(&iter, ct, i);
                    self.completions_render(desc, icon, ct, i, *uu);
                }
            }
        }
        if let Some(ps) = prev_sel {
            if let Some(iter) = self.locate(ps) {
                self.sel.select_iter(&iter);
            }
        }
    }
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
    fn completions_render(&self, desc: String, icon: String, ct: &ChainType, i: usize, uu: Uuid) {
        self.completions.set(&self.completions.append(), &vec![
            0, // identifier
            1, // uuid
            2, // description
            3, // icon
        ], &vec![
            &format!("{}{}", ct, i) as &ToValue,
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
    fn chain_render(&self, ti: &TreeIter, ct: &ChainType, cidx: usize) {
        let ident = format!("{}<span fgcolor=\"#666666\">{}</span>", ct, cidx);
        self.store.set(ti, &vec![
            1, // identifier (looking glass column)
        ], &vec![
            &ident as &ToValue,
        ].deref());
    }
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
    pub fn update_desc(selfish: Rc<RefCell<Self>>, uu: Uuid, desc: CommandDescriptor) {
        let mut selfish = selfish.borrow_mut();
        selfish.commands.insert(uu, desc.clone());
        if let Some(ti) = selfish.locate(uu) {
            selfish.render(&ti, &desc, uu);
        }
    }
    pub fn delete(selfish: Rc<RefCell<Self>>, uu: Uuid) {
        let mut selfish = selfish.borrow_mut();
        selfish.commands.remove(&uu);
        if let Some(ti) = selfish.locate(uu) {
            selfish.store.remove(&ti);
            selfish.redraw();
        }
    }
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
    pub fn update_identifiers(selfish: Rc<RefCell<Self>>, idents: HashMap<String, Uuid>) {
        let mut selfish = selfish.borrow_mut();
        selfish.identifiers = idents;
        selfish.update_completions();
    }
}

use state::{CommandState, ChainType, Chain, CommandDescriptor};
use std::collections::{BTreeMap, HashMap};
use gtk::prelude::*;
use gtk::{Builder, TreeStore, TreeIter, TreeView};
use uuid::Uuid;
use std::ops::Deref;

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
    chains: HashMap<Uuid, ChainType>,
}
impl ListController {
    pub fn new(b: &Builder) -> Self {
        ListController {
            store: b.get_object("command-tree").unwrap(),
            view: b.get_object("command-tree-view").unwrap(),
            chains: HashMap::new(),
        }
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
    fn chain_render(&mut self, ti: &TreeIter, ct: &ChainType, cidx: usize, chainuu: Uuid) {
        let ident = format!("{}<span fgcolor=\"#666666\">{}</span>", ct, cidx);
        let cu = format!("{}", chainuu);
        self.store.set(ti, &vec![
            1, // identifier (looking glass column)
            6, // chain UUID
        ], &vec![
            &ident as &ToValue,
            &cu as &ToValue
        ].deref());
    }
    fn render(&mut self, ti: &TreeIter, v: CommandDescriptor, uu: Uuid) {
        let (mut icon, desc, mut dur, mut bgc, uu) =
            (format!("dialog-question"),
             v.desc.clone(),
             format!(""),
             format!("white"),
             format!("{}", uu));
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
    }
    pub fn update_desc(&mut self, uu: Uuid, desc: CommandDescriptor) {
        if let Some(ti) = self.locate(uu) {
            self.render(&ti, desc, uu);
        }
        else {
            let ti = self.store.append(None);
            self.render(&ti, desc, uu);
        }
    }
    pub fn delete(&mut self, uu: Uuid) {
        if let Some(ti) = self.locate(uu) {
            self.store.remove(&ti);
        }
        else {
            println!("warn: asked to delete {}, which isn't in my TreeStore!", uu);
        }
    }
    pub fn update_chain(&mut self, ct: ChainType, mut chain: Chain) {
        // # What this function does:
        //
        // The aim of this function is to update the tree so that it reflects
        // the state of a given chain. This is how that is accomplished:
        //
        // - First, generate a "chain UUID
        let mut uu: Option<Uuid> = None;
        for (k, v) in self.chains.iter() {
            if ct == v.clone() { // FIXME: ugh
                uu = Some(*k);
                break;
            }
        }
        let uu = if let Some(uu) = uu {
            uu
        }
        else {
            let uu = Uuid::new_v4();
            self.chains.insert(uu, ct.clone());
            uu
        };
        let mut iters = BTreeMap::new();
        let _: Option<()> = self.run_for_each(|ti, ts| {
            if let Some(cmduu) = extract_uuid(ts, ti, 5) {
                if chain.commands.contains(&cmduu) {
                    iters.insert(cmduu, ti.clone());
                }
                else if let Some(chainuu) = extract_uuid(ts, ti, 6) {
                    if chainuu == uu {
                        self.store.set(&ti, &vec![1, 6], &vec![
                            &format!("") as &ToValue,
                            &format!("") as &ToValue
                        ].deref());
                    }
                }
            }
            None
        });
        let mut sorted_iters = vec![];
        for uu in chain.commands.iter() {
            if let Some(iter) = iters.remove(uu) {
                sorted_iters.push(iter);
            }
        }
        if sorted_iters.len() == 0 { return; }
        let mut insert_after: Option<TreeIter> = None;
        let ct1 = ct.clone();
        let _: Option<()> = self.run_for_each(|ti, ts| {
            if let Some(chainuu) = extract_uuid(ts, ti, 6) {
                if let Some(ct2) = self.chains.get(&chainuu) {
                    if ct1 > ct2.clone() { // ugh again
                        insert_after = Some(ti.clone());
                    }
                    else {
                        return Some(());
                    }
                }
            }
            None
        });
        for (i, ni) in sorted_iters.into_iter().enumerate() {
            self.chain_render(&ni, &ct, i, uu);
            self.store.move_after(&ni, insert_after.as_ref());
            insert_after = Some(ni);
        }
    }
}

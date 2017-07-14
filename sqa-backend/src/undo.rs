use codec::Command;

const MAX_UNDOS: usize = 100;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UndoableChange {
    pub undo: Command,
    pub redo: Command,
    pub desc: String
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UndoContext {
    changes: Vec<UndoableChange>,
    idx: Option<usize>
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UndoState {
    pub undo: Option<String>,
    pub redo: Option<String>
}
impl UndoContext {
    pub fn new() -> Self {
        let gutter = UndoableChange {
            undo: Command::Ping,
            redo: Command::Ping,
            desc: "nothing".into()
        };
        UndoContext {
            changes: vec![gutter],
            idx: None
        }
    }
    pub fn register_change(&mut self, ch: UndoableChange) {
        trace!("registering undoable change \"{}\"", ch.desc);
        if let Some(idx) = self.idx {
            trace!("obliterating redoability");
            self.changes.drain((idx+1)..);
            self.idx = None;
        }
        self.changes.push(ch);
        if self.changes.len() > MAX_UNDOS {
            self.changes.remove(0);
        }
    }
    fn indexes(&self) -> (Option<usize>, Option<usize>) {
        let (mut undo, mut redo) = (None, None);
        let idx = self.idx.unwrap_or(self.changes.len()-1);
        if self.changes.get(idx+1).is_some() {
            redo = Some(idx+1);
        }
        if self.changes[idx].desc != "nothing" {
            undo = Some(idx);
        }
        (undo, redo)
    }
    pub fn undo(&mut self) -> Option<Command> {
        let (undo, _) = self.indexes();
        trace!("attempting to undo, idx {:?}", self.idx);
        if let Some(idx) = undo {
            self.idx = Some(idx-1);
            Some(self.changes[idx].undo.clone())
        }
        else {
            None
        }
    }
    pub fn redo(&mut self) -> Option<Command> {
        let (_, redo) = self.indexes();
        trace!("attempting to redo, idx {:?}", self.idx);
        if let Some(idx) = redo {
            if idx == self.changes.len()-1 {
                self.idx = None;
            }
            else {
                self.idx = Some(idx);
            }
            Some(self.changes[idx].redo.clone())
        }
        else {
            None
        }
    }
    pub fn state(&self) -> UndoState {
        let (undo, redo) = self.indexes();
        UndoState {
            undo: undo.and_then(|idx| self.changes.get(idx)).map(|x| x.desc.clone()),
            redo: redo.and_then(|idx| self.changes.get(idx)).map(|x| x.desc.clone())
        }
    }
}

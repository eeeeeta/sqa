//! Program state management.

use streamv2::FileStreamX;
use mixer::Magister;
use std::collections::BTreeMap;

/// Global context
pub struct Context<'a> {
    pub idents: BTreeMap<String, FileStreamX>,
    pub mstr: Magister<'a>
}
impl<'a> Context<'a> {
    pub fn new() -> Self {
        Context {
            idents: BTreeMap::new(),
            mstr: Magister::new()
        }
    }
}

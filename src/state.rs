//! Program state management.

use streamv2::{FileStream, FileStreamX};
use mixer::{QChannel, Magister, Sink, Source, DeviceSink, FRAMES_PER_CALLBACK};
use std::collections::BTreeMap;
use uuid::Uuid;
use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex};
use gtk::Adjustment;
use chrono::{UTC, Duration, DateTime};

#[derive(Clone)]
/// An object for cross-thread notification.
pub struct ThreadNotifier {
    adj: Adjustment
}
impl ThreadNotifier {
    pub fn new() -> Self {
        ThreadNotifier {
            adj: Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        }
    }
    pub fn notify(&self) {
        let selfish = self.clone();
        ::glib::timeout_add(0, move || {
            selfish.adj.changed();
            ::glib::Continue(false)
        });
    }
    pub fn register_handler<F: Fn() + 'static>(&self, func: F) {
        self.adj.connect_changed(move |_| {
            func()
        });
    }
}
/// I'm pretty sure this is safe. Maybe.
///
/// Seriously: glib::timeout_add() runs its handler _in the main thread_,
/// so we should be fine.
unsafe impl Send for ThreadNotifier {}

#[derive(Clone)]
/// The type of an object stored in the database.
pub enum ObjectType {
    /// A channel of a FileStream created from a given file path.
    FileStream(String, usize),
    /// A numbered QChannel.
    QChannel(usize),
    /// A numbered device output channel.
    DeviceOut(usize)
}
impl ObjectType {
    fn is_same_type(&self, rhs: &Self) -> bool {
        match rhs {
            &ObjectType::FileStream(_, _) => {
                if let &ObjectType::FileStream(_, _) = self {
                    true
                }
                else {
                    false
                }
            },
            &ObjectType::QChannel(_) => {
                if let &ObjectType::QChannel(_) = self {
                    true
                }
                else {
                    false
                }
            },
            &ObjectType::DeviceOut(_) => {
                if let &ObjectType::DeviceOut(_) = self {
                    true
                }
                else {
                    false
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub enum ActionState {
    Stopped,
    Errored(String),
    Paused,
    Running,
    Completed
}
#[derive(Clone, Debug)]
pub struct ActionDescriptor {
    pub desc: String,
    pub state: ActionState,
    pub started: DateTime<UTC>,
    pub runtime: Duration,
    pub uuid: Uuid
}
impl ActionDescriptor {
    pub fn new(desc: String) -> Self {
        ActionDescriptor {
            desc: desc,
            state: ActionState::Stopped,
            started: UTC::now(),
            runtime: Duration::zero(),
            uuid: Uuid::new_v4()
        }
    }
}
impl ::std::cmp::PartialEq for ActionDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.started.eq(&other.started)
    }
}
impl ::std::cmp::Eq for ActionDescriptor {}
impl ::std::cmp::PartialOrd for ActionDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        self.started.partial_cmp(&other.started)
    }
}
impl ::std::cmp::Ord for ActionDescriptor {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        self.started.cmp(&other.started)
    }
}
/// A descriptor for a single-channel object stored in the database.
pub struct Descriptor {
    /// The object's type.
    pub typ: ObjectType,
    /// The object's named identifier (if any).
    pub ident: Option<String>,
    /// The UUID of the object's input (if any).
    pub inp: Option<Uuid>,
    /// The UUID of the object's output (if any).
    pub out: Option<Uuid>,
    /// The controller type of this object. (absent in ReadableContext)
    pub controller: Option<Box<Any>>,
    /// Optional objects related to this object (like other channels).
    /// May include this object itself.
    pub others: Option<Vec<Uuid>>
}
/// This is so we can send ReadableContexts between threads.
/// This relies on the invariant that we will never send a Descriptor
/// if its `controller` field is `Some`.
unsafe impl Send for Descriptor {}
impl Descriptor {
    fn into_readable(&self) -> Self {
        Descriptor {
            typ: self.typ.clone(),
            ident: self.ident.clone(),
            inp: self.inp.clone(),
            out: self.out.clone(),
            controller: None,
            others: self.others.clone()
        }
    }
}
impl fmt::Display for Descriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.typ {
            ObjectType::FileStream(ref src, c) => write!(f, "FileStream (channel {}) of file {}", c, src),
            ObjectType::QChannel(c) => write!(f, "QChannel {}", c),
            ObjectType::DeviceOut(c) => write!(f, "Device output channel {}", c)
        }
    }
}
pub trait Database {
    /// Get the descriptor of a numbered QChannel.
    fn get_qch(&self, qch: usize) -> Option<&Descriptor>;
    /// Resolve a named identifier to a Uuid and ObjectType.
    fn resolve_ident(&self, ident: &str) -> Option<(Uuid, ObjectType)>;
    /// Get the controllers for each channel of a FileStream.
    fn control_filestream(&mut self, uu: &Uuid) -> Option<Vec<&mut FileStreamX>>;
    /// Iterate over all instances of a certain object type, optionally filtering with a given UUID.
    fn iter_mut_type<'a>(&'a mut self, ty: ObjectType, uu: Option<&'a Uuid>) -> Box<Iterator<Item=&mut Descriptor> + 'a>;
    /// Get the type of a given UUID.
    fn type_of(&self, uu: &Uuid) -> Option<&ObjectType>;
}

impl Database for BTreeMap<Uuid, Descriptor> {
    fn get_qch(&self, qch: usize) -> Option<&Descriptor> {
        for (_, v) in self.iter() {
            if let ObjectType::QChannel(x) = v.typ {
                if x == qch {
                    return Some(v);
                }
            }
        }
        None
    }
    fn type_of(&self, uu: &Uuid) -> Option<&ObjectType> {
        if let Some(desc) = self.get(uu) {
            Some(&desc.typ)
        }
        else {
            None
        }
    }
    fn iter_mut_type<'a>(&'a mut self, ty: ObjectType, uu: Option<&'a Uuid>) -> Box<Iterator<Item=&mut Descriptor> + 'a> {
        Box::new(
            self.iter_mut()
                .filter(move |&(k, ref v)| {
                    if v.typ.is_same_type(&ty) {
                        if let Some(id) = uu.as_ref() {
                            if *id == k {
                                true
                            }
                            else {
                                false
                            }
                        }
                        else {
                            true
                        }
                    } else {
                        false
                    }
                })
                .map(|(_, v)| v))
    }
    fn resolve_ident(&self, ident: &str) -> Option<(Uuid, ObjectType)> {
        for (k, v) in self.iter() {
            if let Some(ref id) = v.ident {
                if id == ident {
                    return Some((k.clone(), v.typ.clone()));
                }
            }
        }
        None
    }
    fn control_filestream(&mut self, uu: &Uuid) -> Option<Vec<&mut FileStreamX>> {
        let mut _uus = vec![];
        if let Some(v) = self.get(uu) {
            if let Some(ref others) = v.others {
                _uus = others.clone();
            }
            else {
                return None;
            }
        }
        else {
            return None;
        }
        _uus.sort();
        let mut ret = vec![];
        for (k, v) in self.iter_mut() {
            if _uus.binary_search(k).is_ok() {
                if let Some(ctl) = v.controller.as_mut().and_then(|b| b.downcast_mut()) {
                    ret.push(ctl);
                }
                else {
                    return None;
                }
            }
        }
        Some(ret)
    }
}
pub struct ReadableContext {
    pub db: BTreeMap<Uuid, Descriptor>,
    pub acts: Vec<ActionDescriptor>,
}
impl ReadableContext {
    pub fn new() -> Self {
        ReadableContext {
            db: BTreeMap::new(),
            acts: Vec::new()
        }
    }
}
/// Global context
pub struct WritableContext<'a> {
    pub db: BTreeMap<Uuid, Descriptor>,
    pub acts: Vec<ActionDescriptor>,
    pub readable: Arc<Mutex<ReadableContext>>,
    pub mstr: Magister<'a>,
    pub tn: ThreadNotifier
}
impl<'a> WritableContext<'a> {
    pub fn new(readable: Arc<Mutex<ReadableContext>>, tn: ThreadNotifier) -> Self {
        let mut ctx = WritableContext {
            readable: readable,
            db: BTreeMap::new(),
            mstr: Magister::new(),
            acts: Vec::new(),
            tn: tn
        };
        for i in 0..16 {
            let (qch, qchx) = QChannel::new(44_100);
            ctx.db.insert(Uuid::new_v4(), Descriptor {
                typ: ObjectType::QChannel(i),
                ident: None,
                inp: Some(qchx.uuid()),
                out: Some(qch.uuid()),
                controller: None,
                others: None
            });
            ctx.mstr.add_source(Box::new(qch));
            ctx.mstr.add_sink(Box::new(qchx));
        };
        ctx.update();
        ctx
    }
    pub fn get_action_desc_mut(&mut self, uu: Uuid) -> Option<&mut ActionDescriptor> {
        for ad in self.acts.iter_mut() {
            if ad.uuid == uu {
                return Some(ad);
            }
        }
        None
    }
    pub fn insert_device(&mut self, dev: Vec<DeviceSink<'a>>) -> Uuid {
        let mut descs = vec![];
        for (i, stream) in dev.into_iter().enumerate() {
            let uu = stream.uuid();
            self.mstr.add_sink(Box::new(stream));
            if let Some(qch) = self.db.get_qch(i) {
                self.mstr.wire(qch.out.as_ref().unwrap().clone(), uu).unwrap();
            }
            descs.push((Uuid::new_v4(), Descriptor {
                typ: ObjectType::DeviceOut(i),
                ident: None,
                inp: Some(uu),
                out: None,
                controller: None,
                others: None
            }));
        }
        let ids: Vec<Uuid> = descs.iter().map(|x| x.0.clone()).collect();
        for mut d in descs.into_iter() {
            d.1.others = Some(ids.clone());
            self.db.insert(d.0, d.1);
        }
        self.update();
        ids[0]
    }
    pub fn insert_filestream(&mut self, source: String, fs: Vec<(FileStream, FileStreamX)>) -> Uuid {
        let mut descs = vec![];
        for (i, (stream, x)) in fs.into_iter().enumerate() {
            self.mstr.add_source(Box::new(stream));
            descs.push((Uuid::new_v4(), Descriptor {
                typ: ObjectType::FileStream(source.clone(), i),
                ident: None,
                inp: None,
                out: Some(x.uuid()),
                controller: Some(Box::new(x)),
                others: None
            }));
        }
        let ids: Vec<Uuid> = descs.iter().map(|x| x.0.clone()).collect();
        for mut d in descs.into_iter() {
            d.1.others = Some(ids.clone());
            self.db.insert(d.0, d.1);
        }
        self.update();
        ids[0]
    }
    // FIXME: not very efficient
    pub fn update(&mut self) {
        let mut rd = self.readable.lock().unwrap();
        *rd = ReadableContext::new();
        for (k, v) in self.db.iter() {
            rd.db.insert(k.clone(), v.into_readable());
        }
        self.acts.sort();
        for o in self.acts.iter() {
            rd.acts.push(o.clone());
        }
        self.tn.notify();
    }
}

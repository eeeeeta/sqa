//! Cues, defining them, starting them, and all that

use uuid::Uuid;
use std::collections::BTreeMap;
use rsndfile::SndFile;
use streamv2::{FileStream, FileStreamX, db_lin, lin_db};
use mixer::Magister;
use time::Duration;
use std::rc::Rc;
use std::cell::RefCell;

/// Describes the current state of a cue.
#[derive(Clone, Copy, Debug)]
pub struct QState {
    /// Whether this cue is playing.
    active: bool,
    /// How long this cue has been playing for.
    elapsed: Duration,
    /// If applicable, the duration after which the cue will end.
    len: Option<Duration>
}
/// A controllable parameter of a cue.
#[derive(Debug)]
pub enum QParam {
    /// A volume slider, with a given channel number.
    Volume(usize, f32),
    /// A path to a file.
    FilePath(Option<String>),
    /// A UUID to target.
    UuidTarget(Option<Uuid>),
    /// A duration.
    Duration(Duration),
    /// A vector.
    /// Used with get_param() (& _params()) only.
    Vec(Vec<Box<QParam>>),
    /// An instruction to insert an element into a vector.
    /// Used with set_param() only.
    VecInsert(usize, Box<QParam>),
    /// An instruction to remove an element from a vector.
    /// Used with set_param() only.
    VecRemove(usize),
    /// A boolean value.
    Boolean(bool)
}
/// A named cue parameter.
pub type NamedParam = (String, Uuid, QParam);
#[derive(Debug)]
pub enum QParamSetError {
    InvalidTypeProvided,
    ParamNotFound,
    BadVecIndex,
    BadInput(String)
}
/// A list of cues.
pub struct QList<'a> {
    pub cues: BTreeMap<Uuid, Box<Q + 'a>>
}
impl<'a> QList<'a> {
    pub fn new() -> Self {
        QList {
            cues: BTreeMap::new()
        }
    }
    pub fn insert(&mut self, q: Box<Q + 'a>) {
        self.cues.insert(q.uuid(), q);
    }
}
/// Information about a source or sink, in the form `(is_source, channel_no, uuid)`.
pub struct WireableInfo(pub bool, pub usize, pub Uuid);

/// Describes objects which have the behaviour of cues.
pub trait Q {
    /// Runs this cue.
    fn go(&mut self);
    /// Gives this cue an opportunity to do something
    /// to other cues (via the QList).
    ///
    /// Provides the amount of time elapsed since the last
    /// call
    fn poll(&mut self, _: &mut QList, _: Duration) -> Option<Duration> { None }
    /// Pauses this cue.
    fn pause(&mut self);
    /// Resets this cue to its initial state (ready to call go() again)
    fn reset(&mut self);
    /// Gets this cue's list of warnings, if any.
    ///
    /// A cue with warnings acts as "broken" and will not be used.
    /// Implementors may panic if this invariant is violated.
    fn warnings(&self, ql: &QList) -> Vec<String>;
    /// Gets this cue's current state.
    fn state(&self) -> QState;
    /// Gets a parameter of this cue with a given UUID.
    fn get_param(&self, uuid: Uuid) -> Option<QParam>;
    /// Gets a list of all parameters available in this cue.
    fn get_params(&self) -> Vec<NamedParam>;
    /// Sets a parameter of this cue.
    ///
    /// Changing parameters while cues are playing should, in
    /// a decent implementation, recalculate each cue's warnings() and stop
    /// playing those affected by this change.
    /// Implementors may panic if this invariant is violated.
    fn set_param(&mut self, uuid: Uuid, val: QParam) -> Result<QParam, QParamSetError>;
    /// Gets information about sources and sinks this cue provides.
    fn wireables(&self) -> Vec<WireableInfo> { vec![] }
    /// Gets this cue's Universally Unique Identifier (UUID).
    fn uuid(&self) -> Uuid;
}
pub struct AudioQ<'a> {
    mstr: Rc<RefCell<Magister<'a>>>,
    chans: Vec<FileStreamX>,
    chan_uuids: Vec<Uuid>,
    path: Option<String>,
    path_uuid: Uuid,
    uuid: Uuid
}
impl<'a> AudioQ<'a> {
    pub fn new(mstr: Rc<RefCell<Magister<'a>>>) -> Self {
        AudioQ {
            mstr: mstr,
            chans: Vec::new(),
            chan_uuids: Vec::new(),
            path: None,
            path_uuid: Uuid::new_v4(),
            uuid: Uuid::new_v4()
        }
    }
}

impl<'a> Q for AudioQ<'a> {
    fn wireables(&self) -> Vec<WireableInfo> {
        let mut ret = vec![];
        for (i, c) in self.chan_uuids.iter().enumerate() {
            ret.push(WireableInfo(true, i, c.clone()));
        }
        ret
    }
    fn go(&mut self) {
        assert!(self.chans.len() > 0, "AudioQ::go(): nothing to go");
        for ch in self.chans.iter_mut() {
            ch.unpause();
        }
    }
    fn pause(&mut self) {
        assert!(self.chans.len() > 0, "AudioQ::pause(): nothing to pause");
        for ch in self.chans.iter_mut() {
            ch.pause();
        }
    }
    fn reset(&mut self) {
        for ch in self.chans.iter_mut() {
            ch.reset();
        }
    }
    fn state(&self) -> QState {
        if self.chans.len() == 0 {
            QState {
                active: false,
                elapsed: Duration::zero(),
                len: Some(Duration::zero())
            }
        }
        else {
            let lp = self.chans[0].lp();
            QState {
                active: lp.active,
                elapsed: Duration::milliseconds((lp.pos as f32 / 44.1) as i64),
                len: Some(Duration::milliseconds((lp.end as f32 / 44.1) as i64))
            }
        }

    }
    fn uuid(&self) -> Uuid {
        self.uuid
    }
    fn warnings(&self, _: &QList) -> Vec<String> {
        if self.path.is_none() {
            vec![format!("No file to play back.")]
        }
        else {
            vec![]
        }
    }
    fn get_params(&self) -> Vec<NamedParam> {
        let mut ret = vec![];
        ret.push((format!("Audio file path"), self.path_uuid, QParam::FilePath(self.path.as_ref().map(|x| x.clone()))));
        for (i, c) in self.chan_uuids.iter().enumerate() {
            ret.push((format!("Volume of audio channel #{}", i), c.clone(), QParam::Volume(i, self.chans[i].lp().vol)));
        }
        ret
    }
    fn get_param(&self, uuid: Uuid) -> Option<QParam> {
        if uuid == self.path_uuid {
            return Some(QParam::FilePath(self.path.as_ref().map(|x| x.clone())));
        }
        for (i, c) in self.chan_uuids.iter().enumerate() {
            if &uuid == c {
                return Some(QParam::Volume(i, self.chans[i].lp().vol));
            }
        }
        None
    }
    fn set_param(&mut self, uuid: Uuid, val: QParam) -> Result<QParam, QParamSetError> {
        if uuid == self.path_uuid {
            return if let QParam::FilePath(Some(path)) = val {
                let file = SndFile::open(&path);
                if let Err(snde) = file {
                    return Err(QParamSetError::BadInput(format!("Couldn't open file: {}", snde.expl)));
                }
                if file.as_ref().unwrap().info.samplerate != 44_100 {
                    return Err(QParamSetError::BadInput(format!("SQA does not support sample rates other than 44.1kHz (yet) :(")));
                }
                let streams = FileStream::new(file.unwrap());
                let mut mstr = self.mstr.borrow_mut();
                for fsx in self.chans.iter_mut() {
                    mstr.locate_source(fsx.uuid());
                }
                self.chans = vec![];
                self.chan_uuids = vec![];
                for stream in streams.into_iter() {
                    let x = stream.get_x();
                    self.chan_uuids.push(x.uuid());
                    self.chans.push(x);
                    mstr.add_source(Box::new(stream));
                }
                self.path = Some(path.clone());
                Ok(QParam::FilePath(Some(path)))
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }
        }
        for (i, c) in self.chan_uuids.iter().enumerate() {
            if &uuid == c {
                return if let QParam::Volume(req_i, vol) = val {
                    assert!(req_i == i, "AudioQ::set_param(): requested to change volume of mismatching channel & UUID");
                    self.chans[i].set_vol(vol);
                    Ok(QParam::Volume(i, self.chans[i].lp().vol))
                }
                else {
                    Err(QParamSetError::InvalidTypeProvided)
                }
            }
        }
        Err(QParamSetError::ParamNotFound)
    }
}

static FADEQ_INTERVAL: u64 = 100;

pub struct FadeQ {
    params: Vec<Uuid>,
    cue: Option<Uuid>,
    fade_time: Duration,
    fade: f32,
    pos: Duration,
    active: bool,
    stop_after: bool,

    uuid: Uuid,
    cue_uuid: Uuid,
    ft_uuid: Uuid,
    fade_uuid: Uuid,
    params_uuid: Uuid,
    stop_after_uuid: Uuid
}
impl FadeQ {
    pub fn new() -> Self {
        FadeQ {
            params: vec![],
            cue: None,
            params_uuid: Uuid::new_v4(),
            cue_uuid: Uuid::new_v4(),
            ft_uuid: Uuid::new_v4(),
            fade_uuid: Uuid::new_v4(),
            stop_after_uuid: Uuid::new_v4(),
            fade_time: Duration::milliseconds(1000),
            fade: 0.0,
            pos: Duration::milliseconds(0),
            active: false,
            stop_after: false,
            uuid: Uuid::new_v4()
        }
    }
}
impl Q for FadeQ {
    fn go(&mut self) {
        self.active = true;
    }
    fn pause(&mut self) {
        self.active = false;
    }
    fn reset(&mut self) {
        self.active = false;
        self.pos = Duration::milliseconds(0);
    }
    fn state(&self) -> QState {
        QState {
            active: self.active,
            elapsed: self.pos,
            len: Some(self.fade_time)
        }
    }
    fn uuid(&self) -> Uuid {
        self.uuid
    }
    fn poll(&mut self, ql: &mut QList, dur: Duration) -> Option<Duration> {
        if self.active == false {
            return None;
        }
        let qref = ql.cues.get_mut(self.cue.as_ref().unwrap())
            .expect("FadeQ::poll(): someone deleted our cue in the middle of fading");
        let fade = self.fade;
        println!("pos {}", self.pos);
        self.pos = self.pos + dur;
        for param in self.params.iter() {
            if let Some(QParam::Volume(ch, vol)) = qref.get_param(param.clone()) {
                let mut fade_left = vol - fade;
                if fade_left == 0.0 {
                    continue;
                }
                println!("ft - pos: {}", (self.fade_time - self.pos).num_milliseconds());
                let units_left = (self.fade_time - self.pos).num_milliseconds() as u64 / FADEQ_INTERVAL;
                if units_left == 0 {
                    println!("vol: {}, fl: {}, ul: {}, fade_amt: ", vol, fade_left, units_left);
                    qref.set_param(param.clone(), QParam::Volume(ch, fade)).unwrap();
                }
                else {
                    let fade_amount = fade_left / units_left as f32;
                    println!("vol: {}, fl: {}, ul: {}, fade_amt: {}", vol, fade_left, units_left, fade_amount);
                    qref.set_param(param.clone(), QParam::Volume(ch, vol - fade_amount)).unwrap();
                }
            }
            else {
                panic!("FadeQ::poll(): someone changed params of our cue in the middle of fading");
            }
        }
        if (self.fade_time - self.pos).num_milliseconds() <= 0 {
            println!("Done!");
            self.active = false;
            if self.stop_after {
                qref.reset();
            }
            None
        }
        else {
            Some(Duration::milliseconds(FADEQ_INTERVAL as i64))
        }
    }
    fn warnings(&self, ql: &QList) -> Vec<String> {
        let mut ret = vec![];
        if self.cue.is_none() {
            ret.push(format!("No target cue."));
        }
        else if ql.cues.get(self.cue.as_ref().unwrap()).is_none() {
            ret.push(format!("Target cue could not be found."));
            if self.params.len() > 0 {
                ret.push(format!("Fade parameters exist, but there is no valid target cue."));
            }
        }
        else if self.params.len() == 0 {
            ret.push(format!("No parameters to fade on target cue."));
        }
        else {
            let qref = ql.cues.get(self.cue.as_ref().unwrap()).unwrap();
            for (i, param) in self.params.iter().enumerate() {
                if let Some(QParam::Volume(_, _)) = qref.get_param(param.clone()) {
                    /* Yay */
                }
                else {
                    ret.push(format!("Fade parameter #{} could not be found or is not a Volume parameter", i));
                }
            }
        }
        ret
    }
    fn get_params(&self) -> Vec<NamedParam> {
        let mut ret = vec![];
        let param_vec: Vec<Box<QParam>> = self.params.iter()
            .map(|u| Box::new(QParam::UuidTarget(Some(u.clone()))))
            .collect();
        ret.push((format!("Channels to fade"), self.params_uuid, QParam::Vec(param_vec)));
        ret.push((format!("Cue to target"), self.cue_uuid, QParam::UuidTarget(self.cue)));
        ret.push((format!("Stop target when done"), self.stop_after_uuid, QParam::Boolean(self.stop_after)));
        ret.push((format!("Fade time"), self.ft_uuid, QParam::Duration(self.fade_time)));
        ret.push((format!("Fade to volume"), self.fade_uuid, QParam::Volume(0, self.fade)));
        ret
    }
    fn get_param(&self, uuid: Uuid) -> Option<QParam> {
        if uuid == self.params_uuid {
            Some(QParam::Vec(self.params.iter()
                             .map(|u| Box::new(QParam::UuidTarget(Some(u.clone()))))
                             .collect()))
        }
        else if uuid == self.cue_uuid {
            Some(QParam::UuidTarget(self.cue))
        }
        else if uuid == self.ft_uuid {
            Some(QParam::Duration(self.fade_time))
        }
        else if uuid == self.fade_uuid {
            Some(QParam::Volume(0, self.fade))
        }
        else if uuid == self.stop_after_uuid {
            Some(QParam::Boolean(self.stop_after))
        }
        else {
            None
        }
    }
    fn set_param(&mut self, uuid: Uuid, val: QParam) -> Result<QParam, QParamSetError> {
        if uuid == self.params_uuid {
            if let QParam::VecInsert(pos, qp) = val {
                if pos > self.params.len() {
                    Err(QParamSetError::BadVecIndex)
                }
                else if let QParam::UuidTarget(Some(uuid)) = *qp {
                    self.params.insert(pos, uuid);
                    Ok(QParam::Vec(self.params.iter()
                                   .map(|u| Box::new(QParam::UuidTarget(Some(u.clone()))))
                                   .collect()))
                }
                else {
                    Err(QParamSetError::InvalidTypeProvided)
                }
            }
            else if let QParam::VecRemove(pos) = val {
                if pos >= self.params.len() {
                    Err(QParamSetError::BadVecIndex)
                }
                else {
                    self.params.remove(pos);
                    Ok(QParam::Vec(self.params.iter()
                                   .map(|u| Box::new(QParam::UuidTarget(Some(u.clone()))))
                                   .collect()))
                }
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }
        }
        else if uuid == self.cue_uuid {
            if let QParam::UuidTarget(Some(uuid)) = val {
                self.cue = Some(uuid.clone());
                Ok(QParam::UuidTarget(Some(uuid)))
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }
        }
        else if uuid == self.ft_uuid {
            if let QParam::Duration(dur) = val {
                self.fade_time = dur;
                Ok(QParam::Duration(dur))
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }

        }
        else if uuid == self.fade_uuid {
            if let QParam::Volume(_, vol) = val {
                self.fade = vol;
                Ok(QParam::Volume(0, vol))
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }
        }
        else if uuid == self.stop_after_uuid {
            if let QParam::Boolean(sa) = val {
                self.stop_after = sa;
                Ok(QParam::Boolean(sa))
            }
            else {
                Err(QParamSetError::InvalidTypeProvided)
            }
        }
        else {
            Err(QParamSetError::ParamNotFound)
        }
    }
}


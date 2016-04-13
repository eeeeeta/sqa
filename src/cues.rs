//! Cues, defining them, starting them, and all that

use uuid::Uuid;
use rsndfile::SndFile;
use streamv2::{FileStream, FileStreamX};
use mixer::Magister;
use chrono::duration::Duration;
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
    Volume(usize, f32),
    FilePath(String)
}
#[derive(Debug)]
pub enum QParamSetError {
    InvalidTypeProvided,
    ParamNotFound,
    BadInput(String)
}
/// Information about a source or sink, in the form `(is_source, channel_no, uuid)`.
pub struct WireableInfo(pub bool, pub usize, pub Uuid);

/// Describes objects which have the behaviour of cues.
pub trait Q {
    /// Runs this cue.
    fn go(&mut self);
    /// Pauses this cue.
    fn pause(&mut self);
    /// Resets this cue to its initial state (ready to call go() again)
    fn reset(&mut self);
    /// Gets this cue's list of warnings, if any.
    ///
    /// A cue with warnings acts as "broken" and will not be used.
    /// Implementors may panic if this invariant is violated.
    fn warnings(&self) -> Vec<String>;
    /// Gets this cue's current state.
    fn state(&self) -> QState;
    /// Gets a parameter of this cue with a given UUID.
    fn get_param(&self, uuid: Uuid) -> Option<QParam>;
    /// Gets a list of all parameters available in this cue.
    fn get_params(&self) -> Vec<(Uuid, QParam)>;
    /// Sets a parameter of this cue.
    fn set_param(&mut self, uuid: Uuid, val: QParam) -> Result<QParam, QParamSetError>;
    /// Gets information about sources and sinks this cue provides.
    fn wireables(&self) -> Vec<WireableInfo>;
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
    fn warnings(&self) -> Vec<String> {
        if self.path.is_none() {
            vec![format!("No file to play back.")]
        }
        else {
            vec![]
        }
    }
    fn get_params(&self) -> Vec<(Uuid, QParam)> {
        let mut ret = vec![];
        ret.push((self.path_uuid, QParam::FilePath(format!("{}", self.path.as_ref().map(|x| x as &str).unwrap_or("[none]")))));
        for (i, c) in self.chan_uuids.iter().enumerate() {
            ret.push((c.clone(), QParam::Volume(i, self.chans[i].lp().vol)));
        }
        ret
    }
    fn get_param(&self, uuid: Uuid) -> Option<QParam> {
        if uuid == self.path_uuid {
            return Some(QParam::FilePath(format!("{}", self.path.as_ref().map(|x| x as &str).unwrap_or("[none]"))));
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
            return if let QParam::FilePath(path) = val {
                let file = SndFile::open(&path);
                if let Err(snde) = file {
                    return Err(QParamSetError::BadInput(format!("Couldn't open file: {}", snde.expl)));
                }
                if file.as_ref().unwrap().info.samplerate != 44_100 {
                    return Err(QParamSetError::BadInput(format!("SQA does not support sample rates other than 44.1kHz. :(")));
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
                Ok(QParam::FilePath(path))
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

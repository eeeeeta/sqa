//! Plays back an audio file.

use sqa_engine::{PlainSender, Sender};
use sqa_ffmpeg::MediaFile;
use super::{Parameter, Value, ParameterError, ActionFuture, LoadFuture, ActionController};
use state::Context;
use futures::future;
use futures::Future;
use std::error::Error;
use std::any::Any;
use futures::sync::{mpsc, oneshot};
use std::thread;
use std::panic;
pub struct Controller {
    url: Option<String>,
    senders: Vec<PlainSender>
}
impl Controller {
    fn load(file: MediaFile) -> Result<Vec<PlainSender>, Box<Error + Send>> {
        unimplemented!()
    }
}
impl ActionController for Controller {
    fn desc(&self) -> String {
        format!("Play audio at {}", self.url.as_ref().map(|x| x as &str).unwrap_or("???"))
    }
    fn get_params(&self) -> Vec<Parameter> {
        vec![
            Parameter {
                val: self.url.as_ref().map(|x| x.clone().into()),
                desc: "URL to play".into(),
                name: "url".into()
            }
        ]
    }
    fn set_param(&mut self, id: &str, val: Option<Value>) -> bool {
        match id {
            "url" => self.url = val.map(|x| x.string().unwrap()),
            _ => return false
        }
        true
    }
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError> {
        let mut ret = vec![];
        if let Some(ref st) = self.url {
            let mf = MediaFile::new(&mut ctx.media, &st);
            if let Err(e) = mf {
                ret.push(ParameterError {
                    name: "url".into(),
                    err: format!("Error opening URL: {}", e)
                })
            }
        }
        else {
            ret.push(ParameterError {
                name: "url".into(),
                err: "This field is required.".into()
            })
        }
        ret
    }
    fn load(&mut self, ctx: &mut Context) -> Result<Option<LoadFuture>, Box<Error>> {
        let (tx, rx) = oneshot::channel();
        let url = self.url.clone().unwrap();
        let mf = MediaFile::new(&mut ctx.media, &url)?;
        thread::spawn(move || {
            tx.complete(panic::catch_unwind(|| {
                Controller::load(mf)
            }));
        });
        let fut = rx.map_err(|c| {
            panic!("wat")
        }).and_then(|res| {
            match res {
                Ok(x) => match x {
                    Ok(y) => Ok(Box::new(y) as _),
                    Err(e) => Err(e)
                },
                Err(e) => Err(Box::new(::std::io::Error::new(::std::io::ErrorKind::Other, "failure")) as _)
            }
        });
        Ok(Some(Box::new(fut)))
    }
    fn loaded(&mut self, ctx: &mut Context, a: Box<Any>) -> Result<(), Box<Error>> {
        unimplemented!()
    }
    fn execute(&mut self, time: u64, ctx: &mut Context) -> ActionFuture {
        unimplemented!()
    }
}

//! Lexical environment.

use crate::value::Value;

#[derive(Debug, Default, Clone)]
pub struct Env {
    frames: Vec<Frame>,
}

#[derive(Debug, Default, Clone)]
struct Frame {
    bindings: Vec<(String, Value)>,
}

impl Env {
    pub fn root() -> Self {
        Self { frames: vec![Frame::default()] }
    }

    pub fn child(&self) -> Self {
        let mut e = self.clone();
        e.frames.push(Frame::default());
        e
    }

    pub fn bind(&mut self, name: String, value: Value) {
        let f = self.frames.last_mut().expect("env must have a frame");
        f.bindings.push((name, value));
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        for f in self.frames.iter().rev() {
            for (n, v) in f.bindings.iter().rev() {
                if n == name {
                    return Some(v);
                }
            }
        }
        None
    }
}

//! Requirements: INPUT-001, INPUT-002
//! Safety-critical.

pub mod nested;

use crate::hidden::Original;

pub fn validate() {}

pub fn original() -> Original {
    Original
}

pub struct Frame;

impl Frame {
    pub fn decode() {}
}

pub enum Kind {
    Sbus,
}

pub mod inline {
    pub fn inner() {}

    pub fn from_parent() {
        super::validate();
    }
}

#[cfg(test)]
mod tests {
    fn helper() {}

    #[test]
    fn rejects_stale() {
        helper();
    }
}
